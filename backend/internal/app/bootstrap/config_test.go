package bootstrap

import (
	"os"
	"path/filepath"
	"testing"
)

func TestConfiguredModelUsesGlobalModelsAuthPath(t *testing.T) {
	global, project := t.TempDir(), t.TempDir()
	t.Setenv("ROZSA_CONFIG_DIR", global)
	t.Setenv("ROZSA_PROJECT_CONFIG_DIR", project)
	if err := os.MkdirAll(filepath.Join(global, "models"), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(global, "models", "test.json"), []byte(`{"providers":{"test":{"api":"openai-responses","baseUrl":"https://example.invalid/v1","models":[{"id":"model"}]}}}`), 0o600); err != nil {
		t.Fatal(err)
	}
	_, _, path, err := ConfiguredModel("test", "model")
	if err != nil || path != filepath.Join(global, "models", "auth.json") {
		t.Fatalf("auth path %q, %v", path, err)
	}
}
