package modelregistry

import (
	"os"
	"path/filepath"
	"testing"

	modeltypes "rozsa/backend/internal/model/types"
)

const generated = `{"openai":{"gpt":{"id":"gpt","name":"GPT","api":"openai-completions","provider":"openai","baseUrl":"https://api.example/v1","reasoning":true,"input":["text"],"contextWindow":128000,"maxTokens":4096,"cost":{"input":1,"output":2,"cacheRead":0,"cacheWrite":0}}}}`

func TestLoadProjectOverridesAndModelConfig(t *testing.T) {
	global, project := t.TempDir(), t.TempDir()
	if err := os.WriteFile(filepath.Join(global, "base.json"), []byte(`{"providers":{"openai":{"baseUrl":"https://global.example/v1","headers":{"X-Global":"yes"},"modelOverrides":{"gpt":{"contextWindow":64000,"cost":{"input":3}}}}}}`), 0600); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(project, "project.json"), []byte(`{"providers":{"openai":{"baseUrl":"https://project.example/v1","headers":{"X-Project":"yes"},"modelOverrides":{"gpt":{"thinkingEffortMap":{"low":"light","high":null},"maxTokens":2048}}}}}`), 0600); err != nil {
		t.Fatal(err)
	}
	registry, diagnostics, err := Load([]byte(generated), global, project)
	if err != nil {
		t.Fatal(err)
	}
	if len(diagnostics) != 0 {
		t.Fatalf("unexpected diagnostics: %#v", diagnostics)
	}
	model, found := registry.Find("openai", "gpt")
	if !found {
		t.Fatal("model missing")
	}
	if model.BaseURL != "https://project.example/v1" || model.ContextWindow != 64000 || model.MaxTokens != 2048 || model.Cost.Input != 3 {
		t.Fatalf("unexpected merged model: %#v", model)
	}
	if model.Headers["X-Global"] != "yes" || model.Headers["X-Project"] != "yes" {
		t.Fatalf("headers were not merged: %#v", model.Headers)
	}
	if model.SupportsThinking(modeltypes.ThinkingHigh) || !model.SupportsThinking(modeltypes.ThinkingLow) {
		t.Fatal("thinking null and configured values were not preserved")
	}
}

func TestEmptyAndRefreshFailure(t *testing.T) {
	registry := Empty()
	if len(registry.All()) != 0 {
		t.Fatal("empty registry contains models")
	}
	loadedRegistry, _, err := Load([]byte(generated))
	if err != nil {
		t.Fatal(err)
	}
	registry = loadedRegistry
	beforeRefresh := registry.All()
	_, err = registry.Refresh([]byte("{"))
	if err == nil {
		t.Fatal("expected refresh parse error")
	}
	afterRefresh := registry.All()
	if len(afterRefresh) != len(beforeRefresh) || afterRefresh[0].ID != beforeRefresh[0].ID {
		t.Fatalf("failed refresh changed registry: %#v", afterRefresh)
	}
}

func TestCustomProviderURLHeadersAndDiagnostics(t *testing.T) {
	dir := t.TempDir()
	configJSON := `{"providers":{"custom":{"api":"openai-completions","baseUrl":"https://custom.example/v1","headers":{"X-Test":"1"},"models":[{"id":"m","contextWindow":32000,"maxTokens":1000}]}}}`
	if err := os.WriteFile(filepath.Join(dir, "custom.json"), []byte(configJSON), 0600); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "broken.json"), []byte(`{"providers":`), 0600); err != nil {
		t.Fatal(err)
	}
	registry, diagnostics, err := Load([]byte(`{}`), dir)
	if err != nil {
		t.Fatal(err)
	}
	if len(diagnostics) != 1 {
		t.Fatalf("expected one diagnostic, got %#v", diagnostics)
	}
	model, found := registry.Find("custom", "m")
	if !found {
		t.Fatal("valid custom model was discarded")
	}
	if model.BaseURL != "https://custom.example/v1" || model.Headers["X-Test"] != "1" || model.ContextWindow != 32000 {
		t.Fatalf("custom config lost: %#v", model)
	}
}
