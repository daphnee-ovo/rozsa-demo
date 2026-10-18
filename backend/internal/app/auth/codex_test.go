package auth

import (
	"context"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"testing"
	"time"

	"rozsa/backend/internal/model/credentials"
	"rozsa/backend/internal/model/oauth/codex"
)

func TestCodexRefreshAndLogoutUseExplicitAuthPath(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		_, _ = w.Write([]byte(`{"access_token":"renewed","expires_in":300}`))
	}))
	defer server.Close()
	authPath := filepath.Join(t.TempDir(), "global", "models", "auth.json")
	if err := credentials.StoreOAuth(authPath, "codex-oauth", credentials.OAuthCredential{Type: "oauth", Access: "expired", Refresh: "refresh-token", Expires: 1, Extra: map[string]any{"accountId": "acct"}}); err != nil {
		t.Fatal(err)
	}
	client := codex.NewClient(server.Client())
	client.TokenURL = server.URL
	client.Now = func() time.Time { return time.Unix(10, 0) }
	changed := []string{}
	service := &Codex{Client: client, AuthPath: authPath, Changed: func(provider string) { changed = append(changed, provider) }}
	got, err := service.Refresh(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if got.Access != "renewed" || got.Refresh != "refresh-token" {
		t.Fatalf("credential=%+v", got)
	}
	removed, err := service.Logout()
	if err != nil || !removed {
		t.Fatalf("removed=%v err=%v", removed, err)
	}
	data, err := os.ReadFile(authPath)
	if err != nil {
		t.Fatal(err)
	}
	if string(data) != "{}" {
		t.Fatalf("credential remains: %s", data)
	}
	if len(changed) != 2 || changed[0] != "codex-oauth" || changed[1] != "codex-oauth" {
		t.Fatalf("changed=%v", changed)
	}
}
