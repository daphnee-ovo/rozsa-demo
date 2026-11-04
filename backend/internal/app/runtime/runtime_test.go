package runtime_test

import (
	"testing"

	appRuntime "rozsa/backend/internal/app/runtime"
	"rozsa/backend/internal/app/session"
	"rozsa/backend/internal/model/types"
)

type Config = appRuntime.Config
type Runtime = appRuntime.Runtime
type MessageEvent = appRuntime.MessageEvent
type MessageEventSubscription = appRuntime.MessageEventSubscription

const (
	MessageTextDelta = appRuntime.MessageTextDelta
	MessageCompleted = appRuntime.MessageCompleted
	MessageFailed    = appRuntime.MessageFailed
	MessageCanceled  = appRuntime.MessageCanceled
)

var New = appRuntime.New

func activeSession(t *testing.T, app *Runtime) *session.Manager {
	t.Helper()
	path := app.SessionStatus().ActivePath
	if path == "" {
		t.Fatal("runtime has no active session")
	}
	manager, err := session.OpenManager(path)
	if err != nil {
		t.Fatal(err)
	}
	return manager
}

func TestNewAcceptsProtocolOwnedCredentialsWithoutAPIKey(t *testing.T) {
	useProjectTestCWD(t)
	_, err := New(Config{
		SessionDir: t.TempDir(),
		Model:      types.Model{ID: "amazon.nova-test", Provider: "amazon-bedrock", API: types.APIBedrockConverseStream},
	})
	if err != nil {
		t.Fatalf("New() with protocol-owned credentials: %v", err)
	}
}

func TestNewStillRequiresAPIKeyForManagedCredentialProtocols(t *testing.T) {
	useProjectTestCWD(t)
	_, err := New(Config{
		SessionDir: t.TempDir(),
		Model:      types.Model{ID: "custom-model", Provider: "custom-provider", API: types.APIAnthropicMessages},
	})
	if err == nil || err.Error() != "No API key for provider: custom-provider" {
		t.Fatalf("New() error = %v, want preserved missing API key error", err)
	}
}
