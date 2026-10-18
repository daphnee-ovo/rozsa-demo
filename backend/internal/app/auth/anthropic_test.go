package auth

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"testing"
	"time"

	"rozsa/backend/internal/model/credentials"
	"rozsa/backend/internal/model/oauth/anthropic"
)

func TestSupersededLoginCannotPersistLateToken(t *testing.T) {
	firstRequest := make(chan struct{})
	releaseFirst := make(chan struct{})
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		var body struct {
			Code string `json:"code"`
		}
		if err := json.NewDecoder(request.Body).Decode(&body); err != nil {
			t.Errorf("decode token request: %v", err)
		}
		if body.Code == "first-code" {
			close(firstRequest)
			<-releaseFirst
			_, _ = writer.Write([]byte(`{"access_token":"late-access","refresh_token":"late-refresh","expires_in":3600}`))
			return
		}
		writer.Header().Set("Content-Type", "application/json")
		_, _ = writer.Write([]byte(`{"access_token":"current-access","refresh_token":"current-refresh","expires_in":3600}`))
	}))
	defer server.Close()

	path := filepath.Join(t.TempDir(), "models", "auth.json")
	if err := credentials.StoreOAuth(path, "codex-oauth", credentials.OAuthCredential{Access: "other-provider", Refresh: "other-refresh", Expires: time.Now().Add(time.Hour).UnixMilli()}); err != nil {
		t.Fatal(err)
	}
	var changes []string
	manager, err := NewAnthropic(path, anthropic.NewClientWithConfig(server.Client(), anthropic.Config{
		TokenURL:        server.URL,
		CallbackAddress: "127.0.0.1:0",
	}), func(_ context.Context, provider string) error {
		changes = append(changes, provider)
		return nil
	})
	if err != nil {
		t.Fatal(err)
	}

	firstContext, cancelFirst := context.WithCancel(context.Background())
	firstEvents := make(chan anthropic.Event, 1)
	firstInputs := make(chan anthropic.Input, 1)
	firstResult := make(chan error, 1)
	go func() {
		_, loginErr := manager.Login(firstContext, firstEvents, firstInputs)
		firstResult <- loginErr
	}()
	<-firstEvents
	firstInputs <- anthropic.Input{Kind: anthropic.InputAuthorizationResponse, Value: "first-code"}
	<-firstRequest

	secondEvents := make(chan anthropic.Event, 1)
	secondInputs := make(chan anthropic.Input, 1)
	secondResult := make(chan error, 1)
	go func() {
		_, loginErr := manager.Login(context.Background(), secondEvents, secondInputs)
		secondResult <- loginErr
	}()
	<-secondEvents
	secondInputs <- anthropic.Input{Kind: anthropic.InputAuthorizationResponse, Value: "second-code"}
	if err := <-secondResult; err != nil {
		t.Fatal(err)
	}
	if err := <-firstResult; err == nil {
		t.Fatal("superseded login unexpectedly succeeded")
	}
	cancelFirst()
	close(releaseFirst)

	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	var stored map[string]json.RawMessage
	if err := json.Unmarshal(data, &stored); err != nil {
		t.Fatal(err)
	}
	var anthropicCredential credentials.OAuthCredential
	if err := json.Unmarshal(stored["anthropic"], &anthropicCredential); err != nil {
		t.Fatal(err)
	}
	if anthropicCredential.Access != "current-access" || anthropicCredential.Refresh != "current-refresh" {
		t.Fatalf("stale login replaced current credential: %#v", anthropicCredential)
	}
	var codexCredential credentials.OAuthCredential
	if err := json.Unmarshal(stored["codex-oauth"], &codexCredential); err != nil || codexCredential.Access != "other-provider" {
		t.Fatalf("login changed another provider: %#v, %v", codexCredential, err)
	}
	if len(changes) != 1 || changes[0] != "anthropic" {
		t.Fatalf("model change callback = %#v", changes)
	}
}

func TestLogoutRemovesOnlyAnthropicAndRefreshesItsModels(t *testing.T) {
	path := filepath.Join(t.TempDir(), "models", "auth.json")
	for provider := range map[string]struct{}{"anthropic": {}, "github-copilot": {}} {
		if err := credentials.StoreOAuth(path, provider, credentials.OAuthCredential{Access: provider + "-access", Refresh: "refresh", Expires: time.Now().Add(time.Hour).UnixMilli()}); err != nil {
			t.Fatal(err)
		}
	}
	var changed string
	manager, err := NewAnthropic(path, anthropic.NewClient(nil), func(_ context.Context, provider string) error {
		changed = provider
		return nil
	})
	if err != nil {
		t.Fatal(err)
	}
	if err := manager.Logout(context.Background()); err != nil {
		t.Fatal(err)
	}
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	var stored map[string]json.RawMessage
	if err := json.Unmarshal(data, &stored); err != nil {
		t.Fatal(err)
	}
	if _, exists := stored["anthropic"]; exists {
		t.Fatal("Anthropic credential remains after logout")
	}
	if _, exists := stored["github-copilot"]; !exists {
		t.Fatal("logout removed an unrelated provider credential")
	}
	if changed != "anthropic" {
		t.Fatalf("model change callback provider = %q", changed)
	}
}
