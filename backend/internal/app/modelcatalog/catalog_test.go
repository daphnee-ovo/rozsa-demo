package modelcatalog_test

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"reflect"
	"testing"
	"time"

	. "rozsa/backend/internal/app/modelcatalog"
	"rozsa/backend/internal/model/credentials"
)

func testCatalog(t *testing.T, fixture string, env map[string]string, dynamic ...map[string]DynamicRefresh) (Service, *Auth, [2]string) {
	t.Helper()
	root := t.TempDir()
	dirs := [2]string{filepath.Join(root, "global", "models"), filepath.Join(root, "project", "models")}
	contents, err := os.ReadFile(filepath.Join("../../../../testdata/fixtures/backend/modelcatalog", fixture))
	if err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(dirs[0], 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dirs[0], "models.json"), contents, 0o600); err != nil {
		t.Fatal(err)
	}
	auth := &Auth{Path: filepath.Join(dirs[0], "auth.json"), Resolver: credentials.Resolver{
		LookupEnv: func(name string) (string, bool) {
			value, ok := env[name]
			if name == "ROZSA_CONFIG_DIR" {
				return filepath.Dir(dirs[0]), true
			}
			return value, ok
		},
		HomeDir: func() (string, error) { return root, nil },
	}}
	options := Options{Directories: dirs, Auth: auth}
	if len(dynamic) > 0 {
		options.Dynamic = dynamic[0]
	}
	service, err := New(options)
	if err != nil {
		t.Fatal(err)
	}
	return service, auth, dirs
}
func providerNames(snapshot Snapshot) []string {
	names := []string{}
	for _, model := range snapshot.Models {
		names = append(names, model.Provider)
	}
	return names
}
func storeToken(t *testing.T, path, provider string, credential credentials.OAuthCredential) {
	t.Helper()
	if err := credentials.StoreOAuth(path, provider, credential); err != nil {
		t.Fatal(err)
	}
}
func freshToken(access string) credentials.OAuthCredential {
	return credentials.OAuthCredential{Access: access, Refresh: "refresh", Expires: time.Now().Add(time.Hour).UnixMilli()}
}

func TestAvailabilityPreservesRustProviderRules(t *testing.T) {
	service, auth, _ := testCatalog(t, "availability.json", map[string]string{"OPENAI_API_KEY": "environment", "ANTHROPIC_API_KEY": "ignored-for-list"})
	initial, err := service.Available(t.Context())
	if err != nil {
		t.Fatal(err)
	}
	if got := providerNames(initial); !reflect.DeepEqual(got, []string{"custom", "openai"}) {
		t.Fatalf("providers = %v", got)
	}
	if len(initial.Diagnostics) != 1 || initial.Diagnostics[0].Provider != "missing" {
		t.Fatalf("diagnostics: %#v", initial.Diagnostics)
	}
	storeToken(t, auth.Path, "anthropic", freshToken("anthropic-access"))
	storeToken(t, auth.Path, "codex-oauth", freshToken("codex-access"))
	storeToken(t, auth.Path, "github-copilot", freshToken("copilot-access"))
	withAuth, err := service.Available(t.Context())
	if err != nil {
		t.Fatal(err)
	}
	if got := providerNames(withAuth); !reflect.DeepEqual(got, []string{"anthropic", "custom", "github-copilot", "openai"}) {
		t.Fatalf("providers with auth: %v", got)
	}
	codex := freshToken("codex-access")
	codex.Extra = map[string]any{"account_id": "account"}
	storeToken(t, auth.Path, "codex-oauth", codex)
	withAccount, err := service.Available(t.Context())
	if err != nil || len(withAccount.Models) != 5 {
		t.Fatalf("account models: %#v, %v", withAccount, err)
	}
	if _, err := credentials.RemoveStored(auth.Path, "anthropic"); err != nil {
		t.Fatal(err)
	}
	loggedOut, err := service.Available(t.Context())
	if err != nil || reflect.DeepEqual(providerNames(loggedOut), providerNames(withAccount)) {
		t.Fatalf("logout did not remove provider: %#v %v", loggedOut, err)
	}
}

func TestRefreshPersistsTokenAndUpdatesOnlyProviderWithProjectOverride(t *testing.T) {
	discoveries := 0
	var dynamicModelPath string
	dynamic := map[string]DynamicRefresh{"codex-oauth": func(_ context.Context, current credentials.OAuthCredential, force bool) (bool, error) {
		discoveries++
		if current.Access != "new-token" || !force {
			t.Fatalf("discovery credential/force: %#v %v", current, force)
		}
		contents, err := os.ReadFile("../../../../testdata/fixtures/backend/modelcatalog/refresh-dynamic.json")
		if err != nil {
			return false, err
		}
		if err := os.WriteFile(dynamicModelPath, contents, 0o600); err != nil {
			return false, err
		}
		return true, nil
	}}
	service, auth, dirs := testCatalog(t, "refresh-global.json", nil, dynamic)
	dynamicModelPath = filepath.Join(dirs[0], "dynamic.json")
	if _, exists := service.Find("codex-oauth", "discovered"); exists {
		t.Fatal("dynamic-only model exists before refresh")
	}
	projectContents, err := os.ReadFile("../../../../testdata/fixtures/backend/modelcatalog/refresh-project.json")
	if err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(dirs[1], 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dirs[1], "override.json"), projectContents, 0o600); err != nil {
		t.Fatal(err)
	}
	current := freshToken("old-token")
	current.Expires = 1
	current.Extra = map[string]any{"accountId": "account"}
	storeToken(t, auth.Path, "codex-oauth", current)
	refreshes := 0
	auth.Refresh = func(ctx context.Context, provider string, current credentials.OAuthCredential) (credentials.OAuthCredential, error) {
		refreshes++
		current.Access = "new-token"
		current.Expires = time.Now().Add(time.Hour).UnixMilli()
		return current, nil
	}
	snapshot, err := service.Available(t.Context())
	if err != nil || len(snapshot.Diagnostics) != 0 {
		t.Fatalf("refresh: %#v, %v", snapshot, err)
	}
	if refreshes != 1 || discoveries != 1 {
		t.Fatalf("refresh/discovery calls: %d/%d", refreshes, discoveries)
	}
	stored, err := credentials.ResolveAuth(t.Context(), auth.Path, "codex-oauth", time.Now(), nil)
	if err != nil || stored.Access != "new-token" {
		t.Fatalf("persisted: %#v %v", stored, err)
	}
	model, ok := service.Find("codex-oauth", "new")
	if !ok || model.Name != "Project model" {
		t.Fatalf("project override missing for refreshed model: %#v", model)
	}
	if model, ok := service.Find("codex-oauth", "discovered"); !ok || model.Name != "Discovered model" {
		t.Fatalf("dynamic model missing after discovery: %#v", model)
	}
	if _, ok := service.Find("custom", "unchanged"); !ok {
		t.Fatal("refresh removed unrelated provider")
	}
}

func TestFailedRefreshRetainsDiagnosticAndDoesNotHideOtherProviders(t *testing.T) {
	service, auth, _ := testCatalog(t, "failed-refresh.json", nil)
	current := freshToken("expired")
	current.Expires = 1
	storeToken(t, auth.Path, "anthropic", current)
	auth.Refresh = func(context.Context, string, credentials.OAuthCredential) (credentials.OAuthCredential, error) {
		return credentials.OAuthCredential{}, errors.New("refresh HTTP 503")
	}
	snapshot, err := service.Available(t.Context())
	if err != nil || !reflect.DeepEqual(providerNames(snapshot), []string{"custom"}) || len(snapshot.Diagnostics) != 1 {
		t.Fatalf("failure: %#v, %v", snapshot, err)
	}
	if err := service.Refresh(t.Context(), "anthropic", false); err == nil {
		t.Fatal("explicit refresh concealed failure")
	}
}

func TestDiscoveryFailureRemainsVisibleUntilProviderRefreshRecovers(t *testing.T) {
	calls, fail := 0, true
	dynamic := map[string]DynamicRefresh{"codex-oauth": func(context.Context, credentials.OAuthCredential, bool) (bool, error) {
		calls++
		if fail {
			return false, errors.New("discovery HTTP 503")
		}
		return false, nil
	}}
	service, auth, _ := testCatalog(t, "discovery.json", nil, dynamic)
	current := freshToken("access")
	current.Extra = map[string]any{"accountId": "account"}
	storeToken(t, auth.Path, "codex-oauth", current)
	for range 2 {
		snapshot, err := service.Available(t.Context())
		if err != nil || len(snapshot.Diagnostics) != 1 || snapshot.Diagnostics[0].Provider != "codex-oauth" || !reflect.DeepEqual(providerNames(snapshot), []string{"custom"}) {
			t.Fatalf("discovery failure: %#v, %v", snapshot, err)
		}
	}
	if calls != 1 {
		t.Fatalf("repeated discovery calls = %d", calls)
	}
	fail = false
	if err := service.Refresh(t.Context(), "codex-oauth", true); err != nil {
		t.Fatal(err)
	}
	snapshot, err := service.Available(t.Context())
	if err != nil || len(snapshot.Diagnostics) != 0 || len(snapshot.Models) != 2 || calls != 2 {
		t.Fatalf("recovery: %#v, %v, calls=%d", snapshot, err, calls)
	}
}

func TestWatchObservesLoginLogoutAndStopsOnCancellation(t *testing.T) {
	service, auth, _ := testCatalog(t, "watch-anthropic.json", nil)
	ctx, cancel := context.WithCancel(t.Context())
	defer cancel()
	updates := make(chan Snapshot, 8)
	done := make(chan error, 1)
	go func() { done <- service.Watch(ctx, func(snapshot Snapshot) { updates <- snapshot }) }()
	waitModels := func(count int) {
		t.Helper()
		select {
		case snapshot := <-updates:
			if len(snapshot.Models) != count {
				t.Fatalf("models: %#v", snapshot)
			}
		case <-time.After(4 * time.Second):
			t.Fatal("model update timeout")
		}
	}
	waitModels(0)
	storeToken(t, auth.Path, "anthropic", freshToken("login"))
	waitModels(1)
	if _, err := credentials.RemoveStored(auth.Path, "anthropic"); err != nil {
		t.Fatal(err)
	}
	waitModels(0)
	cancel()
	select {
	case err := <-done:
		if !errors.Is(err, context.Canceled) {
			t.Fatal(err)
		}
	case <-time.After(time.Second):
		t.Fatal("watcher leaked")
	}
}

func TestWatchReportsDirectoryFailureAndRecovers(t *testing.T) {
	service, _, dirs := testCatalog(t, "custom.json", nil)
	ctx, cancel := context.WithCancel(t.Context())
	defer cancel()
	updates := make(chan Snapshot, 8)
	done := make(chan error, 1)
	go func() { done <- service.Watch(ctx, func(snapshot Snapshot) { updates <- snapshot }) }()
	wait := func(models, diagnostics int) {
		t.Helper()
		select {
		case snapshot := <-updates:
			if len(snapshot.Models) != models || len(snapshot.Diagnostics) != diagnostics {
				t.Fatalf("snapshot = %#v", snapshot)
			}
		case err := <-done:
			t.Fatalf("watch stopped: %v", err)
		case <-time.After(4 * time.Second):
			t.Fatal("model update timeout")
		}
	}
	wait(1, 0)
	if err := os.MkdirAll(filepath.Dir(dirs[1]), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(dirs[1], []byte("not a directory"), 0o600); err != nil {
		t.Fatal(err)
	}
	wait(0, 1)
	if err := os.Remove(dirs[1]); err != nil {
		t.Fatal(err)
	}
	wait(1, 0)
	cancel()
	if err := <-done; !errors.Is(err, context.Canceled) {
		t.Fatal(err)
	}
}
