package credentials

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"sync/atomic"
	"testing"
	"time"
)

func testResolver(env map[string]string, home string) Resolver {
	return Resolver{
		LookupEnv: func(name string) (string, bool) { value, exists := env[name]; return value, exists },
		HomeDir:   func() (string, error) { return home, nil },
		Shell:     "/bin/zsh",
	}
}

func TestResolveEnvironmentVariablePriorityAndLiteralProfiles(t *testing.T) {
	temp := t.TempDir()
	config := filepath.Join(temp, "config")
	home := filepath.Join(temp, "home")
	if err := os.MkdirAll(config, 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(home, 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(config, ".env"), []byte("export TOKEN=\"private value\"\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(home, ".zshenv"), []byte("export TOKEN=from-profile\nexport IGNORED=$(echo unsafe)\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	resolver := testResolver(map[string]string{"ROZSA_CONFIG_DIR": config, "TOKEN": "process"}, home)
	value, err := resolver.ResolveEnvironmentVariable("TOKEN")
	if err != nil || value != "process" {
		t.Fatalf("process value = %q, %v", value, err)
	}
	resolver.LookupEnv = func(name string) (string, bool) {
		if name == "ROZSA_CONFIG_DIR" {
			return config, true
		}
		return "", false
	}
	value, err = resolver.ResolveEnvironmentVariable("TOKEN")
	if err != nil || value != "private value" {
		t.Fatalf("private file value = %q, %v", value, err)
	}
	value, err = resolver.ResolveEnvironmentVariable("MISSING")
	if err != nil || value != "" {
		t.Fatalf("missing value = %q, %v", value, err)
	}
	if _, err := resolver.ResolveEnvironmentVariable("BAD-NAME"); err == nil {
		t.Fatal("expected invalid variable name")
	}
}

func TestResolveEnvironmentVariableShellProfileDoesNotExecuteExpressions(t *testing.T) {
	home := t.TempDir()
	if err := os.WriteFile(filepath.Join(home, ".zshenv"), []byte("export PROFILE_KEY='from profile'\nexport UNSAFE=$(touch marker)\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	resolver := testResolver(map[string]string{"ROZSA_CONFIG_DIR": filepath.Join(home, "missing")}, home)
	value, err := resolver.ResolveEnvironmentVariable("PROFILE_KEY")
	if err != nil || value != "from profile" {
		t.Fatalf("profile value = %q, %v", value, err)
	}
	value, err = resolver.ResolveEnvironmentVariable("UNSAFE")
	if err != nil || value != "" {
		t.Fatalf("expression value = %q, %v", value, err)
	}
	if _, err := os.Stat(filepath.Join(home, "marker")); !errors.Is(err, os.ErrNotExist) {
		t.Fatalf("shell expression ran: %v", err)
	}
}

func TestResolveAPIKeyUsesConfigThenEnvironmentThenOAuth(t *testing.T) {
	path := filepath.Join(t.TempDir(), "auth.json")
	if err := StoreOAuth(path, "anthropic", OAuthCredential{Access: "oauth", Refresh: "refresh", Expires: time.Now().Add(time.Hour).UnixMilli()}); err != nil {
		t.Fatal(err)
	}
	resolver := testResolver(map[string]string{"ROZSA_CONFIG_DIR": filepath.Dir(path), "KEY": "configured-env", "ANTHROPIC_API_KEY": "provider-env"}, t.TempDir())
	value, err := resolver.ResolveAPIKey(context.Background(), "anthropic", "literal-config", "ANTHROPIC_API_KEY", path, nil)
	if err != nil || value != "literal-config" {
		t.Fatalf("config key = %q, %v", value, err)
	}
	value, err = resolver.ResolveAPIKey(context.Background(), "anthropic", "", "ANTHROPIC_API_KEY", path, nil)
	if err != nil || value != "provider-env" {
		t.Fatalf("environment key = %q, %v", value, err)
	}
	resolver.LookupEnv = func(name string) (string, bool) { return "", false }
	value, err = resolver.ResolveAPIKey(context.Background(), "anthropic", "", "", path, nil)
	if err != nil || value != "oauth" {
		t.Fatalf("OAuth key = %q, %v", value, err)
	}
}

func TestInvalidConfiguredReferenceDoesNotFallBack(t *testing.T) {
	resolver := testResolver(map[string]string{"ANTHROPIC_API_KEY": "fallback"}, t.TempDir())
	_, err := resolver.ResolveAPIKey(context.Background(), "anthropic", "$MISSING", "ANTHROPIC_API_KEY", "", nil)
	if err == nil {
		t.Fatal("missing configured reference must be reported")
	}
	if _, err := resolver.ResolveConfigValue("!echo secret"); err == nil {
		t.Fatal("shell commands must be rejected")
	}
}

func TestOAuthRefreshConcurrentAndPreservesExtraFields(t *testing.T) {
	path := filepath.Join(t.TempDir(), "auth.json")
	if err := StoreOAuth(path, "codex-oauth", OAuthCredential{Access: "old", Refresh: "refresh", Expires: 1, Extra: map[string]any{"accountId": "account"}}); err != nil {
		t.Fatal(err)
	}
	var refreshCalls atomic.Int32
	refresh := func(ctx context.Context, _ string, credential OAuthCredential) (OAuthCredential, error) {
		refreshCalls.Add(1)
		time.Sleep(20 * time.Millisecond)
		credential.Access, credential.Expires = "new", time.Now().Add(time.Hour).UnixMilli()
		return credential, nil
	}
	results := make(chan OAuthCredential, 2)
	failures := make(chan error, 2)
	for range 2 {
		go func() {
			credential, err := ResolveAuth(context.Background(), path, "codex-oauth", time.Now(), refresh)
			results <- credential
			failures <- err
		}()
	}
	for range 2 {
		if err := <-failures; err != nil {
			t.Fatal(err)
		}
		if credential := <-results; credential.Access != "new" {
			t.Fatalf("access = %q", credential.Access)
		}
	}
	if refreshCalls.Load() != 1 {
		t.Fatalf("refresh calls = %d, want 1", refreshCalls.Load())
	}
	loaded, err := ResolveAuth(context.Background(), path, "codex-oauth", time.Now(), nil)
	if err != nil || loaded.Extra["accountId"] != "account" {
		t.Fatalf("preserved credential extra = %#v, %v", loaded.Extra, err)
	}
}

func TestCancelledRefreshDoesNotWriteCredential(t *testing.T) {
	path := filepath.Join(t.TempDir(), "auth.json")
	if err := StoreOAuth(path, "anthropic", OAuthCredential{Access: "old", Refresh: "refresh", Expires: 1}); err != nil {
		t.Fatal(err)
	}
	original, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	_, err = ResolveAuth(ctx, path, "anthropic", time.Now(), func(ctx context.Context, _ string, credential OAuthCredential) (OAuthCredential, error) {
		cancel()
		return OAuthCredential{Access: "new", Refresh: credential.Refresh, Expires: time.Now().Add(time.Hour).UnixMilli()}, nil
	})
	if !errors.Is(err, context.Canceled) {
		t.Fatalf("error = %v, want cancellation", err)
	}
	after, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if string(after) != string(original) {
		t.Fatalf("cancelled refresh mutated auth.json: %s", after)
	}
}

func TestStoreAndLogoutOnlyChangeRequestedProviderAndProtectFile(t *testing.T) {
	path := filepath.Join(t.TempDir(), "nested", "auth.json")
	for provider, token := range map[string]string{"anthropic": "a", "codex-oauth": "b"} {
		if err := StoreOAuth(path, provider, OAuthCredential{Access: token, Refresh: "r", Expires: 1}); err != nil {
			t.Fatal(err)
		}
	}
	removed, err := RemoveStored(path, "anthropic")
	if err != nil || !removed {
		t.Fatalf("remove = %v, %v", removed, err)
	}
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if string(data) == "" || !contains(string(data), "codex-oauth") || contains(string(data), "anthropic") {
		t.Fatalf("unexpected auth.json after logout: %s", data)
	}
	info, err := os.Stat(path)
	if err != nil {
		t.Fatal(err)
	}
	if runtime.GOOS != "windows" && info.Mode().Perm() != 0o600 {
		t.Fatalf("auth.json mode = %o", info.Mode().Perm())
	}
	removed, err = RemoveStored(path, "missing")
	if err != nil || removed {
		t.Fatalf("missing remove = %v, %v", removed, err)
	}
}

func TestPrivateEnvironmentParseErrorAndDollarValue(t *testing.T) {
	config := t.TempDir()
	if err := os.WriteFile(filepath.Join(config, ".env"), []byte("TOKEN=$literal-secret\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	resolver := testResolver(map[string]string{"ROZSA_CONFIG_DIR": config}, t.TempDir())
	value, err := resolver.ResolveEnvironmentVariable("TOKEN")
	if err != nil || value != "$literal-secret" {
		t.Fatalf("private literal = %q, %v", value, err)
	}
	if err := os.WriteFile(filepath.Join(config, ".env"), []byte("malformed entry\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	if _, err := resolver.ResolveEnvironmentVariable("TOKEN"); err == nil || !strings.Contains(err.Error(), "expected NAME=VALUE") {
		t.Fatalf("parse error = %v", err)
	}
}

func TestResolveAuthAcceptsStoredAPIKeyAndReportsMalformedCredential(t *testing.T) {
	path := filepath.Join(t.TempDir(), "auth.json")
	if err := os.WriteFile(path, []byte(`{"custom":{"type":"api_key","key":"saved-key"}}`), 0o600); err != nil {
		t.Fatal(err)
	}
	credential, err := ResolveAuth(context.Background(), path, "custom", time.Now(), nil)
	if err != nil || credential.Access != "saved-key" {
		t.Fatalf("stored key = %#v, %v", credential, err)
	}
	resolver := testResolver(map[string]string{}, t.TempDir())
	resolved, err := resolver.ResolveAPIKey(context.Background(), "custom", "", "", path, nil)
	if err != nil || resolved != "saved-key" {
		t.Fatalf("resolved stored key = %q, %v", resolved, err)
	}
	if err := os.WriteFile(path, []byte(`{"custom":{"type":"oauth","access":4}}`), 0o600); err != nil {
		t.Fatal(err)
	}
	if _, err := ResolveAuth(context.Background(), path, "custom", time.Now(), nil); err == nil {
		t.Fatal("malformed OAuth credential must return an error")
	}
}

func contains(value, part string) bool { return strings.Contains(value, part) }
