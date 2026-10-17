package themes

import (
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"rozsa/backend/internal/app/settings"
)

func writeThemeFixture(testContext *testing.T, path, body string) {
	testContext.Helper()
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		testContext.Fatal(err)
	}
	if err := os.WriteFile(path, []byte(body), 0o600); err != nil {
		testContext.Fatal(err)
	}
}

func TestBuiltinThemesMatchProductDefinitions(testContext *testing.T) {
	themeStore := NewStore(filepath.Join(testContext.TempDir(), "global"), filepath.Join(testContext.TempDir(), "project"))
	summaries, err := themeStore.List()
	if err != nil {
		testContext.Fatal(err)
	}
	if len(summaries) != 2 || summaries[0] != (Summary{ID: BuiltinLightID, Name: "Rozsa", Mode: Light}) ||
		summaries[1] != (Summary{ID: BuiltinDarkID, Name: "Rozsa Dark", Mode: Dark}) {
		testContext.Fatalf("built-in summaries: %#v", summaries)
	}
	light, err := themeStore.Get(BuiltinLightID, Light)
	if err != nil {
		testContext.Fatal(err)
	}
	if light.Accent != "#D7827E" || light.Background != "#FFFFFF" || light.Foreground != "#575279" ||
		light.Variables["--surface"] != "oklch(100% 0 0)" || light.UIFont == "" || light.CodeFont == "" {
		testContext.Fatalf("light product definition: %#v", light)
	}
	dark, err := themeStore.Get(BuiltinDarkID, Dark)
	if err != nil {
		testContext.Fatal(err)
	}
	if dark.Accent != "#d88991" || dark.Variables["--sidebar-bg"] != "#211d1f" {
		testContext.Fatalf("dark product definition: %#v", dark)
	}
	if _, err := themeStore.Get(BuiltinLightID, Dark); err == nil {
		testContext.Fatal("built-in theme mode mismatch was accepted")
	}
}

func TestLayeredThemeLookupAndListUseProjectOverrideAndGlobalSave(testContext *testing.T) {
	root := testContext.TempDir()
	globalRoot, projectRoot := filepath.Join(root, "global"), filepath.Join(root, "project")
	themeStore := NewStore(globalRoot, projectRoot)
	globalTheme := builtin(Light)
	globalTheme.ID, globalTheme.Name = "shared", "Global"
	if err := themeStore.Save(globalTheme); err != nil {
		testContext.Fatal(err)
	}
	projectTheme := globalTheme
	projectTheme.Name = "Project"
	if err := os.MkdirAll(projectRoot, 0o700); err != nil {
		testContext.Fatal(err)
	}
	projectJSON, err := json.Marshal(themeFileFromDefinition(projectTheme))
	if err != nil {
		testContext.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(projectRoot, "shared.json"), projectJSON, 0o600); err != nil {
		testContext.Fatal(err)
	}

	loaded, err := themeStore.Get("shared", Light)
	if err != nil || loaded.Name != "Project" {
		testContext.Fatalf("project layer did not override global: theme=%#v error=%v", loaded, err)
	}
	listed, err := themeStore.List()
	if err != nil {
		testContext.Fatal(err)
	}
	count := 0
	for _, summary := range listed {
		if summary.ID == "shared" {
			count++
			if summary.Name != "Project" {
				testContext.Fatalf("list returned stale layer: %#v", summary)
			}
		}
	}
	if count != 1 {
		testContext.Fatalf("expected one merged shared theme, found %d", count)
	}

	saved := loaded
	saved.Name = "Updated Global"
	if err := themeStore.Save(saved); err != nil {
		testContext.Fatal(err)
	}
	contents, err := os.ReadFile(filepath.Join(globalRoot, "shared.json"))
	if err != nil || !json.Valid(contents) {
		testContext.Fatalf("save did not write a valid global theme: error=%v", err)
	}
	globalOnlyStore := NewStore(globalRoot, "")
	savedTheme, err := globalOnlyStore.Get("shared", Light)
	if err != nil || savedTheme.Name != "Updated Global" || savedTheme.Variables["--surface"] == "" {
		testContext.Fatalf("saved definition did not round-trip: theme=%#v error=%v", savedTheme, err)
	}
	loaded, err = themeStore.Get("shared", Light)
	if err != nil || loaded.Name != "Project" {
		testContext.Fatalf("save unexpectedly replaced project precedence: theme=%#v error=%v", loaded, err)
	}
}

func TestThemeParsingDefaultsValidationAndErrorKinds(testContext *testing.T) {
	root := filepath.Join(testContext.TempDir(), "themes")
	writeThemeFixture(testContext, filepath.Join(root, "partial.json"), `{"mode":"light","variables":{"--surface":"#fffdf8"}}`)
	themeStore := NewStore(root, filepath.Join(testContext.TempDir(), "project"))
	partial, err := themeStore.Get("partial", Light)
	if err != nil {
		testContext.Fatal(err)
	}
	if partial.Name != "partial" || partial.Accent != "#D7827E" || partial.Variables["--surface"] != "#fffdf8" ||
		partial.Variables["--muted"] == "" {
		testContext.Fatalf("partial theme defaults: %#v", partial)
	}

	malformedPath := filepath.Join(root, "malformed.json")
	writeThemeFixture(testContext, malformedPath, `{not json`)
	_, err = themeStore.Get("malformed", Light)
	var themeError *Error
	if !errors.As(err, &themeError) || themeError.Kind != ParseError {
		testContext.Fatalf("parse error kind: %v", err)
	}
	if _, err := themeStore.List(); !errors.As(err, &themeError) || themeError.Kind != ParseError {
		testContext.Fatalf("list must expose malformed theme: %v", err)
	}
	writeThemeFixture(testContext, filepath.Join(root, "missing-mode.json"), `{"name":"No mode"}`)
	if _, err := themeStore.Get("missing-mode", Light); !errors.As(err, &themeError) || themeError.Kind != ParseError {
		testContext.Fatalf("missing required mode must remain a parse error: %v", err)
	}
	invalidUTF8Path := filepath.Join(root, "invalid-utf8.json")
	if err := os.WriteFile(invalidUTF8Path, []byte{'{', '"', 'm', 'o', 'd', 'e', '"', ':', '"', 0xff, '"', '}'}, 0o600); err != nil {
		testContext.Fatal(err)
	}
	if _, err := themeStore.Get("invalid-utf8", Light); !errors.As(err, &themeError) || themeError.Kind != ReadError {
		testContext.Fatalf("invalid UTF-8 must remain a read error: %v", err)
	}

	badPath := filepath.Join(testContext.TempDir(), "bad", "unsafe.json")
	writeThemeFixture(testContext, badPath, `{"mode":"light","variables":{"background":"red"}}`)
	_, err = NewStore(filepath.Dir(badPath), "").Get("unsafe", Light)
	if !errors.As(err, &themeError) || themeError.Kind != ValidationError {
		testContext.Fatalf("validation error kind: %v", err)
	}
	if _, err := themeStore.Get("missing", Light); !errors.As(err, &themeError) || themeError.Kind != NotFoundError {
		testContext.Fatalf("not-found error kind: %v", err)
	}
	if err := themeStore.Save(Definition{ID: "unsafe;id"}); !errors.As(err, &themeError) || themeError.Kind != ValidationError {
		testContext.Fatalf("invalid save error kind: %v", err)
	}
}

func TestThemeReadAndWriteFailuresRetainPathsAndKinds(testContext *testing.T) {
	root := testContext.TempDir()
	directoryTheme := filepath.Join(root, "directory.json")
	if err := os.Mkdir(directoryTheme, 0o700); err != nil {
		testContext.Fatal(err)
	}
	_, err := NewStore(root, "").Get("directory", Light)
	var themeError *Error
	if !errors.As(err, &themeError) || themeError.Kind != ReadError || themeError.Path != directoryTheme {
		testContext.Fatalf("read error details: %#v, %v", themeError, err)
	}

	globalPath := filepath.Join(root, "not-a-directory")
	if err := os.WriteFile(globalPath, []byte("occupied"), 0o600); err != nil {
		testContext.Fatal(err)
	}
	customTheme := builtin(Light)
	customTheme.ID = "custom"
	if err := NewStore(globalPath, "").Save(customTheme); err == nil {
		testContext.Fatal("expected theme write failure")
	} else if !errors.As(err, &themeError) || themeError.Kind != ReadError || themeError.Path != globalPath ||
		!strings.HasPrefix(err.Error(), "failed to read theme file ") {
		testContext.Fatalf("write error details: %#v, %v", themeError, err)
	}
}

func TestAppearanceSelectionPersistsBeforeReturningCompleteSnapshot(testContext *testing.T) {
	root := testContext.TempDir()
	globalSettings := filepath.Join(root, "global-settings.json")
	projectSettings := filepath.Join(root, "project", "settings.json")
	globalThemes, projectThemes := filepath.Join(root, "global-themes"), filepath.Join(root, "project-themes")
	writeThemeFixture(testContext, globalSettings, `{"appearance":{"themeMode":"system","fontSize":16,"translucentSidebar":true,"darkTheme":"ink"}}`)
	writeThemeFixture(testContext, filepath.Join(globalThemes, "paper.json"), `{"mode":"light","name":"Paper","variables":{"--surface":"#fffdf8"}}`)
	writeThemeFixture(testContext, filepath.Join(globalThemes, "ink.json"), `{"mode":"dark","name":"Ink","variables":{"--surface":"#242124"}}`)
	settingsManager, err := settings.Load(globalSettings, &projectSettings, nil)
	if err != nil {
		testContext.Fatal(err)
	}
	appearance := NewAppearance(NewStore(globalThemes, projectThemes), settingsManager)

	snapshot, err := appearance.Select(settings.Project, Light, "paper")
	if err != nil {
		testContext.Fatal(err)
	}
	if snapshot.ThemeMode != "system" || snapshot.FontSize != 16 || !snapshot.TranslucentSidebar ||
		snapshot.LightTheme.Name != "Paper" || snapshot.DarkTheme.Name != "Ink" ||
		snapshot.LightTheme.Variables["--surface"] != "#fffdf8" {
		testContext.Fatalf("appearance snapshot: %#v", snapshot)
	}
	persisted, err := os.ReadFile(projectSettings)
	if err != nil {
		testContext.Fatal(err)
	}
	var projectLayer map[string]any
	if err := json.Unmarshal(persisted, &projectLayer); err != nil {
		testContext.Fatal(err)
	}
	if projectLayer["appearance"].(map[string]any)["lightTheme"] != "paper" {
		testContext.Fatalf("selected theme was not persisted before snapshot: %s", persisted)
	}
	snapshot, err = appearance.Update(settings.Project, "themeMode", "dark")
	if err != nil || snapshot.ThemeMode != "dark" {
		testContext.Fatalf("theme mode update: snapshot=%#v error=%v", snapshot, err)
	}
	snapshot, err = appearance.Update(settings.Project, "fontSize", 18)
	if err != nil || snapshot.FontSize != 18 {
		testContext.Fatalf("font size update: snapshot=%#v error=%v", snapshot, err)
	}
	if _, err := appearance.Update(settings.Project, "fontSize", 31); err == nil {
		testContext.Fatal("out-of-range font size was accepted")
	}
	settingsManager, err = settings.Load(globalSettings, &projectSettings, nil)
	if err != nil {
		testContext.Fatal(err)
	}
	reopened, err := NewAppearance(NewStore(globalThemes, projectThemes), settingsManager).Snapshot()
	if err != nil || reopened.ThemeMode != "dark" || reopened.FontSize != 18 || reopened.LightTheme.Name != "Paper" || reopened.DarkTheme.Name != "Ink" {
		testContext.Fatalf("reopened appearance did not restore persisted selections: snapshot=%#v error=%v", reopened, err)
	}
}

func TestAppearanceSelectionRejectsUnknownThemeBeforeSettingsWrite(testContext *testing.T) {
	root := testContext.TempDir()
	globalSettings := filepath.Join(root, "global-settings.json")
	projectSettings := filepath.Join(root, "project", "settings.json")
	writeThemeFixture(testContext, globalSettings, `{}`)
	writeThemeFixture(testContext, projectSettings, `{"unrelated":"preserve"}`)
	settingsManager, err := settings.Load(globalSettings, &projectSettings, nil)
	if err != nil {
		testContext.Fatal(err)
	}
	appearance := NewAppearance(NewStore(filepath.Join(root, "themes"), ""), settingsManager)
	before, err := os.ReadFile(projectSettings)
	if err != nil {
		testContext.Fatal(err)
	}
	if _, err := appearance.Select(settings.Project, Light, "missing"); err == nil {
		testContext.Fatal("unknown theme selection was accepted")
	}
	after, err := os.ReadFile(projectSettings)
	if err != nil {
		testContext.Fatal(err)
	}
	if string(before) != string(after) {
		testContext.Fatalf("failed theme selection changed settings: before=%s after=%s", before, after)
	}
}
