package runtime

import (
	"testing"

	"rozsa/backend/internal/model/types"
)

func TestNewAcceptsProtocolOwnedCredentialsWithoutAPIKey(t *testing.T) {
	_, err := New(Config{
		SessionDir: t.TempDir(),
		Model:      types.Model{ID: "amazon.nova-test", Provider: "amazon-bedrock", API: types.APIBedrockConverseStream},
	})
	if err != nil {
		t.Fatalf("New() with protocol-owned credentials: %v", err)
	}
}

func TestNewStillRequiresAPIKeyForManagedCredentialProtocols(t *testing.T) {
	_, err := New(Config{
		SessionDir: t.TempDir(),
		Model:      types.Model{ID: "custom-model", Provider: "custom-provider", API: types.APIAnthropicMessages},
	})
	if err == nil || err.Error() != "No API key for provider: custom-provider" {
		t.Fatalf("New() error = %v, want preserved missing API key error", err)
	}
}
