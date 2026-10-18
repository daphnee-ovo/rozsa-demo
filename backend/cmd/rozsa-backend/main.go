package main

import (
	"context"
	"flag"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"sync"
	"time"

	"rozsa/backend/internal/app/bootstrap"
	"rozsa/backend/internal/app/productsettings"
	"rozsa/backend/internal/app/runtime"
	"rozsa/backend/internal/app/session"
	"rozsa/backend/internal/app/settings"
	"rozsa/backend/internal/app/themes"
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
	themeDirectories := roots.ThemeDirs()
	productSettings := productsettings.New(settingsManager, themes.NewStore(themeDirectories[0], themeDirectories[1]))
	backendStartupMark("session-service-ready")
	coordinator := newRuntimeCoordinator(sessions)
	backendStartupMark("runtime-initialization-started")
	go func() {
		model, apiKey, authPath, initErr := bootstrap.ConfiguredModel("", "")
		if initErr != nil {
			backendStartupMark("runtime-initialization-failed")
			coordinator.complete(nil, initErr)
			return
		}
		app, initErr := runtime.New(runtime.Config{
			SessionDir: writeDirectory, SessionDirs: catalogDirectories,
			SessionService: sessions,
			Model:          model, APIKey: apiKey, AuthPath: authPath, Output: io.Discard,
		})
		if initErr != nil {
			backendStartupMark("runtime-initialization-failed")
		} else {
			backendStartupMark("runtime-initialization-complete")
		}
		coordinator.complete(app, initErr)
	}()
	backendStartupMark("rpc-server-starting")
	if err := server.ServeProductApplication(os.Stdin, os.Stdout, coordinator, productMessageApplication{coordinator: coordinator}, productSettings); err != nil {
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
	if err == nil && app != nil && coordinator.selected != nil {
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
	return coordinator.runtime, coordinator.initErr
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
