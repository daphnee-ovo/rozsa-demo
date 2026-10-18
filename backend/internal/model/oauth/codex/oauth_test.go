package codex

import (
	"context"
	"encoding/base64"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"net/url"
	"testing"
	"time"

	"rozsa/backend/internal/model/credentials"
)

func TestRefreshPreservesAccountAndRotatesTokens(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodPost {
			t.Errorf("method=%s", r.Method)
		}
		if err := r.ParseForm(); err != nil {
			t.Fatal(err)
		}
		if r.Form.Get("grant_type") != "refresh_token" || r.Form.Get("refresh_token") != "old-refresh" {
			t.Errorf("form=%v", r.Form)
		}
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"access_token":"new-access","refresh_token":"new-refresh","expires_in":3600,"id_token":""}`))
	}))
	defer server.Close()
	client := NewClient(server.Client())
	client.TokenURL = server.URL
	client.Now = func() time.Time { return time.Unix(100, 0) }
	old := credentials.OAuthCredential{Type: "oauth", Access: "old-access", Refresh: "old-refresh", Expires: 1, Extra: map[string]any{"accountId": "acct", "keep": true}}
	got, err := client.Refresh(context.Background(), old)
	if err != nil {
		t.Fatal(err)
	}
	if got.Access != "new-access" || got.Refresh != "new-refresh" || got.Expires != 3700000 {
		t.Fatalf("credential=%+v", got)
	}
	if got.Extra["accountId"] != "acct" || got.Extra["keep"] != true {
		t.Fatalf("extra=%v", got.Extra)
	}
}

func TestAccountIDExtractsCodexClaim(t *testing.T) {
	payload, _ := json.Marshal(map[string]any{"https://api.openai.com/auth": map[string]string{"chatgpt_account_id": "acct-7"}})
	token := "x." + base64.RawURLEncoding.EncodeToString(payload) + ".y"
	if got := AccountID(token); got != "acct-7" {
		t.Fatalf("account=%q", got)
	}
	if AccountID("bad") != "" {
		t.Fatal("malformed JWT should not yield an account")
	}
}

func TestAuthorizationURLAndLoopbackGuard(t *testing.T) {
	client := NewClient(nil)
	client.AuthorizeURL = "https://login.invalid/authorize"
	client.RedirectURI = "http://localhost:1455/auth/callback"
	parsed, err := url.Parse(client.authorizationURL("challenge", "state"))
	if err != nil {
		t.Fatal(err)
	}
	query := parsed.Query()
	for key, want := range map[string]string{"client_id": ClientID, "response_type": "code", "state": "state", "code_challenge": "challenge", "code_challenge_method": "S256", "originator": "pi"} {
		if query.Get(key) != want {
			t.Errorf("%s=%q", key, query.Get(key))
		}
	}
	if err := ListenAddress("0.0.0.0:1455"); err == nil {
		t.Fatal("non-loopback callback accepted")
	}
	if err := ListenAddress("127.0.0.1:1455"); err != nil {
		t.Fatal(err)
	}
}

func TestLoginHonorsCancellation(t *testing.T) {
	client := NewClient(nil)
	client.CallbackAddress = "127.0.0.1:0"
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	_, err := client.Login(ctx, nil, nil)
	if err == nil {
		t.Fatal("expected cancellation")
	}
}
