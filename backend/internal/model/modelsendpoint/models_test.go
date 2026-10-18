package modelsendpoint

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"testing"
	"time"
)

func TestRefreshUpdatesCodexOnlyAndFiltersCatalog(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Query().Get("client_version") != ClientVersion {
			t.Errorf("client_version=%q", r.URL.Query().Get("client_version"))
		}
		if r.Header.Get("Authorization") != "Bearer synthetic" || r.Header.Get("ChatGPT-Account-ID") != "acct" {
			t.Errorf("headers=%v", r.Header)
		}
		_, _ = w.Write([]byte(`{"models":[{"slug":"visible","display_name":"Visible","context_window":240000,"supported_in_api":true,"visibility":"list","default_reasoning_level":"medium","input_modalities":["text","image"]},{"slug":"hidden","supported_in_api":true,"visibility":"hide"},{"slug":"unsupported","supported_in_api":false},{"slug":"unspecified","visibility":"list"}]}`))
	}))
	defer server.Close()
	dir := t.TempDir()
	path := filepath.Join(dir, "global", "models.json")
	original := []byte(`{"_last_refreshed":1,"providers":{"other":{"api":"openai-completions"},"codex-oauth":{"api":"old"}}}`)
	if err := os.MkdirAll(filepath.Dir(path), 0700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, original, 0600); err != nil {
		t.Fatal(err)
	}
	client := NewClient(server.Client())
	client.Endpoint = server.URL
	client.Now = func() time.Time { return time.Unix(200, 0) }
	updated, err := client.RefreshIfNeeded(t.Context(), path, "synthetic", "acct", true)
	if err != nil {
		t.Fatal(err)
	}
	if !updated {
		t.Fatal("expected refresh")
	}
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	var config struct {
		LastRefreshed int64                      `json:"_last_refreshed"`
		Providers     map[string]json.RawMessage `json:"providers"`
	}
	if err = json.Unmarshal(data, &config); err != nil {
		t.Fatal(err)
	}
	if config.LastRefreshed != 200 {
		t.Fatalf("timestamp=%d", config.LastRefreshed)
	}
	var preserved map[string]string
	if err = json.Unmarshal(config.Providers["other"], &preserved); err != nil || preserved["api"] != "openai-completions" {
		t.Fatalf("other provider overwritten: %s", config.Providers["other"])
	}
	var provider struct {
		Models []map[string]any `json:"models"`
	}
	if err = json.Unmarshal(config.Providers[Provider], &provider); err != nil {
		t.Fatal(err)
	}
	if len(provider.Models) != 1 || provider.Models[0]["id"] != "visible" {
		t.Fatalf("models=%v", provider.Models)
	}
}

func TestCacheFreshFor24Hours(t *testing.T) {
	path := filepath.Join(t.TempDir(), "models.json")
	if err := os.WriteFile(path, []byte(`{"_last_refreshed":1000,"providers":{}}`), 0600); err != nil {
		t.Fatal(err)
	}
	client := NewClient(nil)
	client.Now = func() time.Time { return time.Unix(1000+int64(CacheDuration/time.Second)-1, 0) }
	fresh, err := client.cacheFresh(path)
	if err != nil || !fresh {
		t.Fatalf("fresh=%v err=%v", fresh, err)
	}
	client.Now = func() time.Time { return time.Unix(1000+int64(CacheDuration/time.Second), 0) }
	fresh, err = client.cacheFresh(path)
	if err != nil || fresh {
		t.Fatalf("fresh=%v err=%v", fresh, err)
	}
}

func TestRefreshHonorsContextCancellation(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	client := NewClient(nil)
	client.Endpoint = "http://127.0.0.1:1"
	if _, err := client.RefreshIfNeeded(ctx, filepath.Join(t.TempDir(), "models.json"), "token", "account", true); err == nil {
		t.Fatal("expected cancelled request")
	}
}
