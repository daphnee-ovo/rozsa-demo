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
	r, ds, err := Load([]byte(generated), global, project)
	if err != nil {
		t.Fatal(err)
	}
	if len(ds) != 0 {
		t.Fatalf("unexpected diagnostics: %#v", ds)
	}
	m, ok := r.Find("openai", "gpt")
	if !ok {
		t.Fatal("model missing")
	}
	if m.BaseURL != "https://project.example/v1" || m.ContextWindow != 64000 || m.MaxTokens != 2048 || m.Cost.Input != 3 {
		t.Fatalf("unexpected merged model: %#v", m)
	}
	if m.Headers["X-Global"] != "yes" || m.Headers["X-Project"] != "yes" {
		t.Fatalf("headers were not merged: %#v", m.Headers)
	}
	if m.SupportsThinking(modeltypes.ThinkingHigh) || !m.SupportsThinking(modeltypes.ThinkingLow) {
		t.Fatal("thinking null and configured values were not preserved")
	}
}

func TestEmptyAndRefreshFailure(t *testing.T) {
	r := Empty()
	if len(r.All()) != 0 {
		t.Fatal("empty registry contains models")
	}
	r, _, err := Load([]byte(generated))
	if err != nil {
		t.Fatal(err)
	}
	before := r.All()
	_, err = r.Refresh([]byte("{"))
	if err == nil {
		t.Fatal("expected refresh parse error")
	}
	after := r.All()
	if len(after) != len(before) || after[0].ID != before[0].ID {
		t.Fatalf("failed refresh changed registry: %#v", after)
	}
}

func TestCustomProviderURLHeadersAndDiagnostics(t *testing.T) {
	dir := t.TempDir()
	data := `{"providers":{"custom":{"api":"openai-completions","baseUrl":"https://custom.example/v1","headers":{"X-Test":"1"},"models":[{"id":"m","contextWindow":32000,"maxTokens":1000}]}}}`
	if err := os.WriteFile(filepath.Join(dir, "custom.json"), []byte(data), 0600); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "broken.json"), []byte(`{"providers":`), 0600); err != nil {
		t.Fatal(err)
	}
	r, ds, err := Load([]byte(`{}`), dir)
	if err != nil {
		t.Fatal(err)
	}
	if len(ds) != 1 {
		t.Fatalf("expected one diagnostic, got %#v", ds)
	}
	m, ok := r.Find("custom", "m")
	if !ok {
		t.Fatal("valid custom model was discarded")
	}
	if m.BaseURL != "https://custom.example/v1" || m.Headers["X-Test"] != "1" || m.ContextWindow != 32000 {
		t.Fatalf("custom config lost: %#v", m)
	}
}
