package auth_test

import (
	"context"
	"errors"
	"net/http"
	"net/http/httptest"
	"path/filepath"
	. "rozsa/backend/internal/app/auth"
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
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, _ *http.Request) {
		_, _ = writer.Write([]byte(`{"access_token":"late-access","refresh_token":"late-refresh","expires_in":3600}`))
	}))
	defer server.Close()
	path := filepath.Join(t.TempDir(), "models", "auth.json")
	callbackEntered, callbackCancelled, releaseCallback := make(chan struct{}, 1), make(chan struct{}), make(chan struct{})
	callbackCalled := false
	defer func() {
		select {
		case <-releaseCallback:
		default:
			close(releaseCallback)
		}
	}()
	manager, err := NewAnthropic(path, anthropic.NewClientWithConfig(server.Client(), anthropic.Config{TokenURL: server.URL, CallbackAddress: "127.0.0.1:0"}), func(ctx context.Context, _ string) error {
		if callbackCalled {
			return nil
		}
		callbackCalled = true
		callbackEntered <- struct{}{}
		<-ctx.Done()
		close(callbackCancelled)
		<-releaseCallback
		return ctx.Err()
	})
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
	if err != nil || event.Kind != "authorization_url" {
		t.Fatalf("authorization event=%#v err=%v", event, err)
	}
	if err := service.Input(id, "manual-code"); err != nil {
		t.Fatal(err)
	}
	select {
	case <-callbackEntered:
	case <-ctx.Done():
		t.Fatal("login did not reach the public model-change callback")
	}
	logoutDone := make(chan error, 1)
	go func() { logoutDone <- service.Logout(ctx, "anthropic") }()
	select {
	case <-callbackCancelled:
	case <-ctx.Done():
		t.Fatal("logout did not cancel the active login")
	}
	select {
	case err := <-logoutDone:
		t.Fatalf("logout returned before login callback completed: %v", err)
	default:
	}
	close(releaseCallback)
	if err := <-logoutDone; err != nil {
		t.Fatal(err)
	}
	stored, err := credentials.ResolveAuth(ctx, path, "anthropic", time.Now(), nil)
	if err != nil || stored.Access != "" {
		t.Fatalf("logout retained late credential=%#v err=%v", stored, err)
	}
	service.Close()
	if _, err := service.Start("anthropic"); err == nil {
		t.Fatal("closed controller accepted login")
	}
}

func TestControllerCancelledReceptionDoesNotConsumeQueuedEvent(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, _ *http.Request) {
		_, _ = writer.Write([]byte(`{"access_token":"access","refresh_token":"refresh","expires_in":3600}`))
	}))
	defer server.Close()
	path := filepath.Join(t.TempDir(), "models", "auth.json")
	changed := make(chan struct{})
	manager, err := NewAnthropic(path, anthropic.NewClientWithConfig(server.Client(), anthropic.Config{TokenURL: server.URL, CallbackAddress: "127.0.0.1:0"}), func(context.Context, string) error {
		close(changed)
		return nil
	})
	if err != nil {
		t.Fatal(err)
	}
	service := NewController(manager, nil, nil)
	defer service.Close()
	id, err := service.Start("test")
	if err == nil {
		t.Fatal("unsupported provider unexpectedly started")
	}
	id, err = service.Start("anthropic")
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithTimeout(t.Context(), 3*time.Second)
	defer cancel()
	event, err := service.Next(ctx, id)
	if err != nil || event.Kind != "authorization_url" || event.URL == "" {
		t.Fatalf("authorization event=%#v err=%v", event, err)
	}
	if err := service.Input(id, "manual-code"); err != nil {
		t.Fatal(err)
	}
	select {
	case <-changed:
	case <-ctx.Done():
		t.Fatal("authorization did not complete through the public callback")
	}
	cancelledContext, cancel := context.WithCancel(t.Context())
	cancel()
	if _, err := service.Next(cancelledContext, id); !errors.Is(err, context.Canceled) {
		t.Fatalf("cancelled receive=%v", err)
	}
	event, err = service.Next(ctx, id)
	if err != nil || event.Kind != "progress" || event.Message != "Exchanging authorization code for tokens..." {
		t.Fatalf("queued event lost: %#v %v", event, err)
	}
	event, err = service.Next(ctx, id)
	if err != nil || event.Kind != "complete" {
		t.Fatalf("terminal event=%#v err=%v", event, err)
	}
}
