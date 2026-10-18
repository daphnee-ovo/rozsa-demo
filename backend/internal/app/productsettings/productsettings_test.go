package productsettings

import (
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"testing"

	"rozsa/backend/internal/app/modelcatalog"
	"rozsa/backend/internal/app/modelregistry"
	"rozsa/backend/internal/app/settings"
	"rozsa/backend/internal/app/themes"
	"rozsa/backend/internal/model/credentials"
	modeltypes "rozsa/backend/internal/model/types"
)

func newTestService(t *testing.T) Service {
	t.Helper()
	root := t.TempDir()
	globalSettings := filepath.Join(root, "global", "settings.json")
	projectSettings := filepath.Join(root, "project", "settings.json")
	manager, err := settings.Load(globalSettings, &projectSettings, nil)
	if err != nil {
		t.Fatal(err)
	}
	store := themes.NewStore(filepath.Join(root, "global", "themes"), filepath.Join(root, "project", "themes"))
	return New(manager, store)
}

func TestSnapshotUpdateAndResetUseLayeredSettingsAndThemeContract(t *testing.T) {
	service := newTestService(t)
	initial, err := service.Snapshot(Global)
	if err != nil {
		t.Fatal(err)
	}
	if initial.Scope != Global || initial.Settings["defaultThinkingEffort"] != nil || initial.Appearance.ThemeMode != "system" || initial.Appearance.FontSize != 14 || len(initial.Themes) != 2 {
		t.Fatalf("initial product snapshot: %#v", initial)
	}
	project, err := service.Update(Project, "appearance_font_size", json.Number("18"))
	if err != nil {
		t.Fatal(err)
	}
	if project.Appearance.FontSize != 18 {
		t.Fatalf("project appearance: %#v", project.Appearance)
	}
	global, err := service.Update(Global, "appearance_font_size", json.Number("20"))
	if err != nil {
		t.Fatal(err)
	}
	if global.Appearance.FontSize != 18 {
		t.Fatalf("project layer should continue to override global: %#v", global.Appearance)
	}
	project, err = service.Reset(Project, "appearance_font_size")
	if err != nil {
		t.Fatal(err)
	}
	if project.Appearance.FontSize != 20 {
		t.Fatalf("reset did not restore inherited setting: %#v", project.Appearance)
	}
	if _, err := service.Update(Global, "appearance_theme_mode", "sepia"); err == nil {
		t.Fatal("invalid theme mode accepted")
	}
	if _, err := service.Update(Global, "retry_max_retries", "many"); err == nil {
		t.Fatal("invalid retry value accepted")
	}
	if _, err := service.Update(Global, "transport", "websocket"); err == nil {
		t.Fatal("unsupported transport was persisted")
	}
	if current, err := service.Snapshot(Global); err != nil || current.Settings["transport"] != "auto" {
		t.Fatalf("failed transport update changed setting: snapshot=%#v err=%v", current, err)
	}
}

func TestSSETransportPersistsAcrossScopesAndRejectsWebSocket(t *testing.T) {
	service := newTestService(t)
	if _, err := service.Update(Global, "transport", "sse"); err != nil {
		t.Fatal(err)
	}
	if _, err := service.Update(Project, "transport", "auto"); err != nil {
		t.Fatal(err)
	}
	inherited, err := service.Reset(Project, "transport")
	if err != nil || inherited.Settings["transport"] != "sse" {
		t.Fatalf("inherited transport: %#v, %v", inherited.Settings, err)
	}
	for _, value := range []string{"websocket", "websocket-cached"} {
		if _, err := service.Update(Global, "transport", value); err == nil {
			t.Fatalf("accepted %s", value)
		}
	}
	current, err := service.Snapshot(Global)
	if err != nil || current.Settings["transport"] != "sse" {
		t.Fatalf("failed update changed transport: %#v, %v", current.Settings, err)
	}
}

func TestThemeSaveAndSelectionReturnPersistedDefinition(t *testing.T) {
	service := newTestService(t)
	theme := themes.Definition{
		ID: "paper", Name: "Paper", Mode: themes.Light, Accent: "#7D4F59",
		Background: "#FBFAF8", Foreground: "#211D1F", UIFont: "system-ui, sans-serif", CodeFont: "ui-monospace, monospace",
		Variables: map[string]string{"--surface": "#FFFFFF"},
	}
	if err := service.SaveTheme(theme); err != nil {
		t.Fatal(err)
	}
	snapshot, err := service.Update(Project, "appearance_light_theme", "paper")
	if err != nil {
		t.Fatal(err)
	}
	if snapshot.Appearance.LightTheme.ID != "paper" || snapshot.Appearance.LightTheme.Variables["--surface"] != "#FFFFFF" {
		t.Fatalf("selected theme not returned: %#v", snapshot.Appearance.LightTheme)
	}
}

type modelSelectorFixture struct {
	model  modeltypes.Model
	key    string
	effort modeltypes.ThinkingEffort
	err    error
}

func (fixture *modelSelectorFixture) SetModel(model modeltypes.Model, key string, effort modeltypes.ThinkingEffort) error {
	fixture.model, fixture.key = model, key
	fixture.effort = effort
	return fixture.err
}

func (fixture *modelSelectorFixture) SetThinkingEffort(effort modeltypes.ThinkingEffort) (modeltypes.ThinkingEffort, error) {
	fixture.effort = effort
	return effort, fixture.err
}

func TestModelCatalogSelectionPersistsProviderAndModelAndRollsBackOnSetterFailure(t *testing.T) {
	root := t.TempDir()
	modelDir := filepath.Join(root, "models")
	if err := os.MkdirAll(modelDir, 0o700); err != nil {
		t.Fatal(err)
	}
	config := `{"providers":{"anthropic":{"api":"anthropic-messages","baseUrl":"https://example.invalid","apiKey":"test-key","models":[{"id":"claude-test","name":"Claude Test","contextWindow":64000,"maxTokens":4096}]}}}`
	if err := os.WriteFile(filepath.Join(modelDir, "models.json"), []byte(config), 0o600); err != nil {
		t.Fatal(err)
	}
	_, diagnostics, err := modelregistry.Load([]byte("{}"), modelDir)
	if err != nil || len(diagnostics) != 0 {
		t.Fatalf("load test model registry: diagnostics=%#v err=%v", diagnostics, err)
	}
	globalPath := filepath.Join(root, "global", "settings.json")
	projectPath := filepath.Join(root, "project", "settings.json")
	manager, err := settings.Load(globalPath, &projectPath, nil)
	if err != nil {
		t.Fatal(err)
	}
	selector := &modelSelectorFixture{}
	if err := os.WriteFile(filepath.Join(modelDir, "auth.json"), []byte(`{"anthropic":{"type":"api_key","key":"test-key"}}`), 0o600); err != nil {
		t.Fatal(err)
	}
	catalog, err := modelcatalog.New(modelcatalog.Options{Directories: [2]string{modelDir, filepath.Join(root, "project", "models")}, Auth: &modelcatalog.Auth{Path: filepath.Join(modelDir, "auth.json"), Resolver: credentials.NewResolver()}})
	if err != nil {
		t.Fatal(err)
	}
	service := NewWithModelCatalog(manager, themes.NewStore(filepath.Join(root, "global", "themes"), filepath.Join(root, "project", "themes")), catalog, selector)

	initial, err := service.Snapshot(Global)
	if err != nil || len(initial.Models) != 1 || initial.Models[0].ContextWindow != 64000 {
		t.Fatalf("configured model catalog: %#v, err=%v", initial.Models, err)
	}
	selected, err := service.Update(Project, "model", map[string]any{"provider": "anthropic", "id": "claude-test"})
	if err != nil {
		t.Fatal(err)
	}
	if selected.Settings["defaultProvider"] != "anthropic" || selected.Settings["defaultModel"] != "claude-test" || selector.key != "test-key" || selector.model.ID != "claude-test" {
		t.Fatalf("model selection did not reach runtime and layered settings: snapshot=%#v selector=%#v", selected, selector)
	}
	selected, err = service.Update(Project, "small_model", "claude-test")
	if err != nil || selected.Settings["smallModel"] != "claude-test" {
		t.Fatalf("small model was not stored: snapshot=%#v err=%v", selected, err)
	}
	if _, err := service.Update(Project, "small_model", "missing"); err == nil {
		t.Fatal("unknown small model was accepted")
	}

	selector.err = errors.New("runtime rejected model")
	if _, err := service.Update(Global, "model", map[string]any{"provider": "anthropic", "id": "claude-test"}); err == nil {
		t.Fatal("runtime setter failure was hidden")
	}
	globalOverrides, err := manager.Overrides(Global)
	if err != nil {
		t.Fatal(err)
	}
	if globalOverrides["defaultModel"] != nil || globalOverrides["defaultProvider"] != nil {
		t.Fatalf("failed runtime selection left global overrides persisted: %#v", globalOverrides)
	}
}
