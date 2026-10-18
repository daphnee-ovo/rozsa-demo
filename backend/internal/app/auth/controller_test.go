package auth

import (
	"context"
	"errors"
	"net/http"
	"net/http/httptest"
	"path/filepath"
	"testing"
	"time"

	"rozsa/backend/internal/model/credentials"
	"rozsa/backend/internal/model/oauth/anthropic"
)

func TestControllerCarriesTypedLoginInputAndUpdatesOnlyProvider(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, _ *http.Request) {
		_, _ = writer.Write([]byte(`{"access_token":"access","refresh_token":"refresh","expires_in":3600}`))
	}))
	defer server.Close()
	path := filepath.Join(t.TempDir(), "models", "auth.json")
	changed := make(chan string, 1)
	manager, err := NewAnthropic(path, anthropic.NewClientWithConfig(server.Client(), anthropic.Config{TokenURL: server.URL, CallbackAddress: "127.0.0.1:0"}), func(_ context.Context, provider string) error { changed <- provider; return nil })
	if err != nil {
		t.Fatal(err)
	}
	service := NewController(manager, nil, nil)
	defer service.Close()
	id, err := service.Start("anthropic")
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithTimeout(t.Context(), 3*time.Second)
	defer cancel()
	event, err := service.Next(ctx, id)
	if err != nil || event.Kind != "authorization_url" || event.URL == "" || event.Provider != "anthropic" {
		t.Fatalf("event=%#v err=%v", event, err)
	}
	if err := service.Input(id, "manual-code"); err != nil {
		t.Fatal(err)
	}
	for event.Kind != "complete" {
		event, err = service.Next(ctx, id)
		if err != nil || event.Kind == "error" {
			t.Fatalf("terminal=%#v err=%v", event, err)
		}
	}
	if provider := <-changed; provider != "anthropic" {
		t.Fatalf("changed %s", provider)
	}
	current, err := credentials.ResolveAuth(t.Context(), path, "anthropic", time.Now(), nil)
	if err != nil || current.Access != "access" {
		t.Fatalf("stored=%#v err=%v", current, err)
	}
	if _, err := service.Next(ctx, id); err == nil {
		t.Fatal("terminal event was replayed")
	}
	if err := service.Logout(ctx, "anthropic"); err != nil {
		t.Fatal(err)
	}
	current, err = credentials.ResolveAuth(ctx, path, "anthropic", time.Now(), nil)
	if err != nil || current.Access != "" {
		t.Fatalf("logout=%#v err=%v", current, err)
	}
}

func TestControllerLogoutWaitsForCancelledLoginBeforeRemovingCredentials(t *testing.T) {
	stored := false
	service := &controller{providers: map[string]flow{}, operations: map[string]*loginOperation{}, active: map[string]*loginOperation{}}
	started := make(chan struct{})
	service.providers["test"] = flow{
		login: func(ctx context.Context, _ <-chan string, _ func(Event) error) error {
			close(started)
			<-ctx.Done()
			stored = true
			return ctx.Err()
		},
		logout: func(context.Context) error { stored = false; return nil },
	}
	id, err := service.Start("test")
	if err != nil {
		t.Fatal(err)
	}
	<-started
	if err := service.Logout(t.Context(), "test"); err != nil {
		t.Fatal(err)
	}
	if stored {
		t.Fatal("late login survived logout")
	}
	event, err := service.Next(t.Context(), id)
	if err != nil || event.Kind != "error" {
		t.Fatalf("cancel event=%#v %v", event, err)
	}
	service.Close()
	if _, err := service.Start("test"); err == nil {
		t.Fatal("closed controller accepted login")
	}
}

func TestControllerCancelledReceptionDoesNotConsumeQueuedEvent(t *testing.T) {
	service := &controller{providers: map[string]flow{}, operations: map[string]*loginOperation{}, active: map[string]*loginOperation{}}
	service.providers["test"] = flow{login: func(_ context.Context, _ <-chan string, emit func(Event) error) error {
		return emit(Event{Kind: "progress"})
	}}
	id, err := service.Start("test")
	if err != nil {
		t.Fatal(err)
	}
	defer service.Close()
	operation, _ := service.operation(id)
	<-operation.done
	ctx, cancel := context.WithCancel(t.Context())
	cancel()
	if _, err := service.Next(ctx, id); !errors.Is(err, context.Canceled) {
		t.Fatalf("cancelled receive=%v", err)
	}
	event, err := service.Next(t.Context(), id)
	if err != nil || event.Kind != "progress" {
		t.Fatalf("queued event lost: %#v %v", event, err)
	}
}
