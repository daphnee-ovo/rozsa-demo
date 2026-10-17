package productsettings

import (
	"encoding/json"
	"path/filepath"
	"testing"

	"rozsa/backend/internal/app/settings"
	"rozsa/backend/internal/app/themes"
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
