package codex_test

import (
	"context"
	"crypto/sha256"
	"encoding/base64"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"net/url"
	"sync/atomic"
	"testing"
	"time"

	"rozsa/backend/internal/model/credentials"
	. "rozsa/backend/internal/model/oauth/codex"
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
	client.CallbackAddress = "127.0.0.1:0"
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	var challenge string
	var tokenRequests atomic.Int32
	tokenServer := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		tokenRequests.Add(1)
		if err := r.ParseForm(); err != nil {
			t.Errorf("parse token form: %v", err)
		}
		if r.Form.Get("grant_type") != "authorization_code" || r.Form.Get("code") != "authorization-code" {
			t.Errorf("token form = %v", r.Form)
		}
		verifier := r.Form.Get("code_verifier")
		hash := sha256.Sum256([]byte(verifier))
		if verifier == "" || base64.RawURLEncoding.EncodeToString(hash[:]) != challenge {
			t.Errorf("token exchange verifier did not match authorization challenge")
		}
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"access_token":"access","refresh_token":"refresh","expires_in":3600}`))
	}))
	defer tokenServer.Close()
	client.TokenURL = tokenServer.URL
	events := make(chan Event, 1)
	inputs := make(chan string, 1)
	type loginResult struct {
		credential credentials.OAuthCredential
		err        error
	}
	loginDone := make(chan loginResult, 1)
	go func() {
		credential, err := client.Login(ctx, inputs, events)
		loginDone <- loginResult{credential: credential, err: err}
	}()
	var authorizationEvent Event
	select {
	case authorizationEvent = <-events:
	case <-ctx.Done():
		t.Fatal("login did not emit authorization URL before deadline")
	}
	parsed, err := url.Parse(authorizationEvent.URL)
	if err != nil {
		t.Fatal(err)
	}
	query := parsed.Query()
	for key, want := range map[string]string{"client_id": client.ClientID, "response_type": "code", "code_challenge_method": "S256", "originator": "pi"} {
		if query.Get(key) != want {
			t.Errorf("%s=%q", key, query.Get(key))
		}
	}
	challenge = query.Get("code_challenge")
	if challenge == "" || query.Get("state") == "" {
		t.Fatalf("authorization URL omitted state or PKCE challenge: %v", query)
	}
	inputs <- "code=authorization-code&state=" + url.QueryEscape(query.Get("state"))
	select {
	case result := <-loginDone:
		if result.err != nil {
			t.Fatalf("login exchange: %v", result.err)
		}
		if result.credential.Access != "access" || result.credential.Refresh != "refresh" || result.credential.Expires <= 0 {
			t.Fatalf("login credential = %#v", result.credential)
		}
	case <-ctx.Done():
		t.Fatalf("login did not finish before deadline: %v", ctx.Err())
	}
	if requests := tokenRequests.Load(); requests != 1 {
		t.Fatalf("token exchange requests = %d; want exactly one", requests)
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
