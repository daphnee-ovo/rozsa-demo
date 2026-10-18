package main

import (
	"context"
	"errors"
	"flag"
	"fmt"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"sync"
	"time"

	"rozsa/backend/internal/app/auth"
	"rozsa/backend/internal/app/bootstrap"
	"rozsa/backend/internal/app/modelcatalog"
	"rozsa/backend/internal/app/productsettings"
	"rozsa/backend/internal/app/runtime"
	"rozsa/backend/internal/app/session"
	"rozsa/backend/internal/app/settings"
	"rozsa/backend/internal/app/themes"
	"rozsa/backend/internal/model/credentials"
	"rozsa/backend/internal/model/modelsendpoint"
	"rozsa/backend/internal/model/oauth/anthropic"
	modeltypes "rozsa/backend/internal/model/types"
	"rozsa/backend/internal/platform/configpaths"
	"rozsa/backend/internal/transport/server"
	"rozsa/backend/internal/transport/types"
)

type directories []string

func (directories *directories) String() string { return fmt.Sprint([]string(*directories)) }
func (directories *directories) Set(value string) error {
	if value == "" {
		return fmt.Errorf("session directory cannot be empty")
	}
	*directories = append(*directories, value)
	return nil
}
func main() {
	backendStartupMark("process-entry")
	var dirs directories
	flag.Var(&dirs, "session-dir", "session directory; repeat in layer order")
	flag.Parse()
	if flag.NArg() != 0 {
		fmt.Fprintln(os.Stderr, "unexpected positional arguments")
		os.Exit(2)
	}
	cwd, err := os.Getwd()
	if err != nil {
		fail(err.Error())
	}
	writeDirectory := ""
	catalogDirectories := []string(dirs)
	if len(catalogDirectories) == 0 {
		writeDirectory, catalogDirectories, err = bootstrap.SessionDirectories(cwd)
		if err != nil {
			fail(err.Error())
		}
	} else {
		for index, directory := range catalogDirectories {
			catalogDirectories[index], err = filepath.Abs(directory)
			if err != nil {
				fail(err.Error())
			}
		}
		writeDirectory = catalogDirectories[0]
	}
	sessions := session.NewService(session.Catalog{Directories: catalogDirectories})
	roots, err := configpaths.Discover(cwd)
	if err != nil {
		fail(err.Error())
	}
	settingsPaths := roots.SettingsPaths()
	projectSettingsPath := settingsPaths[1]
	settingsManager, err := settings.Load(settingsPaths[0], &projectSettingsPath, nil)
	if err != nil {
		fail(err.Error())
	}
	modelDirectories := roots.ModelDirs()
	authSource := modelcatalog.NewAuth(roots.AuthPath(), nil)
	discovery := modelsendpoint.NewClient(&http.Client{Timeout: 20 * time.Second})
	registry, err := modelcatalog.New(modelcatalog.Options{Directories: modelDirectories, Auth: authSource, Dynamic: map[string]modelcatalog.DynamicRefresh{
		"codex-oauth": func(ctx context.Context, credential credentials.OAuthCredential, force bool) (bool, error) {
			return discovery.RefreshIfNeeded(ctx, filepath.Join(modelDirectories[0], "codex-oauth.json"), credential.Access, credentials.AccountID(credential), force)
		},
	}})
	if err != nil {
		fail(err.Error())
	}
	themeDirectories := roots.ThemeDirs()
	coordinator := newRuntimeCoordinator(sessions)
	productSettings := productsettings.NewWithModelCatalog(settingsManager, themes.NewStore(themeDirectories[0], themeDirectories[1]), registry, coordinator)
	anthropicAuth, err := auth.NewAnthropic(roots.AuthPath(), anthropic.NewClient(nil), func(ctx context.Context, provider string) error { return registry.Refresh(ctx, provider, false) })
	if err != nil {
		fail(err.Error())
	}
	codexAuth := auth.NewCodex(roots.AuthPath(), filepath.Join(modelDirectories[0], "codex-oauth.json"), nil)
	codexAuth.RefreshModels = func(ctx context.Context, _ credentials.OAuthCredential) error {
		return registry.Refresh(ctx, "codex-oauth", true)
	}
	copilotAuth := &auth.Copilot{AuthPath: roots.AuthPath(), OnModelsChanged: func(ctx context.Context, provider string) error { return registry.Refresh(ctx, provider, false) }}
	authService := auth.NewController(anthropicAuth, codexAuth, copilotAuth)
	defer authService.Close()
	backendStartupMark("session-service-ready")
	backendStartupMark("runtime-initialization-started")
	go func() {
		ctx, cancel := context.WithTimeout(context.Background(), 25*time.Second)
		defer cancel()
		initial, initErr := registry.Available(ctx)
		if initErr != nil {
			coordinator.complete(nil, initErr)
			return
		}
		model, apiKey, authPath, initErr := bootstrap.ConfiguredModel("", "")
		if initErr != nil {
			backendStartupMark("runtime-initialization-failed")
			coordinator.complete(nil, initErr)
			return
		}
		available := false
		for _, option := range initial.Models {
			if option.Provider == model.Provider && option.ID == model.ID {
				available = true
				break
			}
		}
		if !available {
			coordinator.complete(nil, fmt.Errorf("provider %s is unavailable for model %s; check model credentials and diagnostics", model.Provider, model.ID))
			return
		}
		resolvedSettings := settingsManager.Resolved()
		thinkingEffort, _ := resolvedSettings["defaultThinkingEffort"].(string)
		app, initErr := runtime.New(runtime.Config{
			SessionDir: writeDirectory, SessionDirs: catalogDirectories,
			SessionService: sessions, ModelRegistry: registry, Credentials: authSource,
			Model: model, ThinkingEffort: modeltypes.ThinkingEffort(thinkingEffort), ResolvedSettings: settingsManager.Resolved, APIKey: apiKey, AuthPath: authPath, Output: io.Discard,
		})
		if initErr != nil {
			backendStartupMark("runtime-initialization-failed")
		} else {
			backendStartupMark("runtime-initialization-complete")
		}
		coordinator.complete(app, initErr)
	}()
	backendStartupMark("rpc-server-starting")
	if err := server.ServeProductApplication(os.Stdin, os.Stdout, coordinator, productMessageApplication{coordinator: coordinator, auth: authService}, productSettings); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}

func backendStartupMark(stage string) {
	if os.Getenv("ROZSA_PERF_TRACE") == "1" {
		fmt.Fprintf(os.Stderr, "ROZSA_PERF_BACKEND %s %d\n", stage, time.Now().UnixMilli())
	}
}

type runtimeCoordinator struct {
	mu           sync.Mutex
	sessions     *session.Service
	runtime      *runtime.Runtime
	selected     *session.LoadedSnapshot
	ready        chan struct{}
	initErr      error
	finished     bool
	closing      bool
	catalogTrace sync.Once
	historyTrace sync.Once
}

func newRuntimeCoordinator(sessions *session.Service) *runtimeCoordinator {
	return &runtimeCoordinator{sessions: sessions, ready: make(chan struct{})}
}

func (coordinator *runtimeCoordinator) complete(app *runtime.Runtime, err error) {
	coordinator.mu.Lock()
	defer coordinator.mu.Unlock()
	if coordinator.finished {
		return
	}
	if coordinator.closing && app != nil {
		ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
		err = errors.Join(err, app.Shutdown(ctx))
		cancel()
	} else if err == nil && app != nil && coordinator.selected != nil {
		_, err = app.Switch(context.Background(), coordinator.selected.Path, coordinator.selected.CatalogID)
	}
	if err == nil && app == nil {
		err = fmt.Errorf("runtime initialization returned no runtime")
	}
	coordinator.runtime, coordinator.initErr = app, err
	coordinator.finished = true
	close(coordinator.ready)
}

func (coordinator *runtimeCoordinator) RuntimeStatus() types.RuntimeStatusResult {
	coordinator.mu.Lock()
	defer coordinator.mu.Unlock()
	if coordinator.closing {
		return types.RuntimeStatusResult{State: "failed", Error: "runtime is shutting down"}
	}
	if !coordinator.finished {
		return types.RuntimeStatusResult{State: "initializing"}
	}
	if coordinator.initErr != nil {
		return types.RuntimeStatusResult{State: "failed", Error: coordinator.initErr.Error()}
	}
	return types.RuntimeStatusResult{State: "ready"}
}

func (coordinator *runtimeCoordinator) await(ctx context.Context) (*runtime.Runtime, error) {
	select {
	case <-ctx.Done():
		return nil, ctx.Err()
	case <-coordinator.ready:
	}
	coordinator.mu.Lock()
	defer coordinator.mu.Unlock()
	if coordinator.closing {
		return nil, errors.New("runtime is shutting down")
	}
	return coordinator.runtime, coordinator.initErr
}

func (coordinator *runtimeCoordinator) SetModel(model modeltypes.Model, configuredAPIKey string, effort modeltypes.ThinkingEffort) error {
	app, err := coordinator.await(context.Background())
	if err != nil {
		return err
	}
	return app.SetModel(model, configuredAPIKey, effort)
}

func (coordinator *runtimeCoordinator) SetThinkingEffort(effort modeltypes.ThinkingEffort) (modeltypes.ThinkingEffort, error) {
	app, err := coordinator.await(context.Background())
	if err != nil {
		return "", err
	}
	return app.SetThinkingEffort(effort)
}

func (coordinator *runtimeCoordinator) Shutdown(ctx context.Context) error {
	coordinator.mu.Lock()
	coordinator.closing = true
	coordinator.mu.Unlock()
	select {
	case <-coordinator.ready:
	case <-ctx.Done():
		return fmt.Errorf("runtime initialization did not drain: %w", ctx.Err())
	}
	coordinator.mu.Lock()
	app := coordinator.runtime
	coordinator.mu.Unlock()
	if app != nil {
		return app.Shutdown(ctx)
	}
	return nil
}

func (coordinator *runtimeCoordinator) GetSessions(ctx context.Context) ([]session.ListEntry, error) {
	firstCall := false
	coordinator.catalogTrace.Do(func() { firstCall = true })
	if firstCall {
		backendStartupMark("session-catalog-started")
	}
	entries, err := coordinator.sessions.GetSessions(ctx)
	if firstCall {
		backendStartupMark("session-catalog-complete")
	}
	return entries, err
}

func (coordinator *runtimeCoordinator) Switch(ctx context.Context, path, catalogID string) (session.LoadedSnapshot, error) {
	coordinator.mu.Lock()
	defer coordinator.mu.Unlock()
	if coordinator.closing {
		return session.LoadedSnapshot{}, errors.New("runtime is shutting down")
	}
	var snapshot session.LoadedSnapshot
	var err error
	firstCall := false
	coordinator.historyTrace.Do(func() { firstCall = true })
	if firstCall {
		backendStartupMark("session-history-started")
	}
	if coordinator.runtime != nil {
		snapshot, err = coordinator.runtime.Switch(ctx, path, catalogID)
	} else {
		snapshot, err = coordinator.sessions.Switch(ctx, path, catalogID)
	}
	if err == nil {
		coordinator.selected = &snapshot
	}
	if firstCall {
		backendStartupMark("session-history-complete")
	}
	return snapshot, err
}

func (coordinator *runtimeCoordinator) Subscribe() session.EventSubscription {
	return coordinator.sessions.Subscribe()
}

func (coordinator *runtimeCoordinator) Snapshot(ctx context.Context) (session.StateSnapshot, error) {
	return coordinator.sessions.Snapshot(ctx)
}

func fail(message string) {
	fmt.Fprintln(os.Stderr, message)
	os.Exit(1)
}
