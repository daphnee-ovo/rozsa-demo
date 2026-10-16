package settings

import (
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"testing"
)

func writeFixture(testContext *testing.T, filePath, contents string) {
	testContext.Helper()
	if err := os.MkdirAll(filepath.Dir(filePath), 0o700); err != nil {
		testContext.Fatal(err)
	}
	if err := os.WriteFile(filePath, []byte(contents), 0o600); err != nil {
		testContext.Fatal(err)
	}
}

func TestLoadLayersPrecedenceAndNestedOverlay(testContext *testing.T) {
	tempDir := testContext.TempDir()
	globalPath := filepath.Join(tempDir, "global.json")
	projectPath := filepath.Join(tempDir, "project.json")
	localPath := filepath.Join(tempDir, "local.json")
	writeFixture(testContext, globalPath, `{"defaultModel":"global","transport":"sse","appearance":{"themeMode":"dark","fontSize":16},"tools":{"read":false,"write":false},"devFlow":{"enabled":false}}`)
	writeFixture(testContext, projectPath, `{"defaultModel":"project","appearance":{"fontSize":18},"tools":{"read":true},"devFlow":{"enabled":true}}`)
	writeFixture(testContext, localPath, `{"defaultModel":"local","tools":{"read":false}}`)
	manager, err := Load(globalPath, &projectPath, &localPath)
	if err != nil {
		testContext.Fatal(err)
	}
	resolvedSettings := manager.Resolved()
	if resolvedSettings["defaultModel"] != "local" || resolvedSettings["transport"] != "sse" {
		testContext.Fatalf("precedence: %#v", resolvedSettings)
	}
	appearanceSettings := resolvedSettings["appearance"].(map[string]any)
	if appearanceSettings["themeMode"] != "dark" || appearanceSettings["fontSize"] != json.Number("18") {
		testContext.Fatalf("nested overlay: %#v", appearanceSettings)
	}
	toolSettings := resolvedSettings["tools"].(map[string]any)
	if toolSettings["read"] != false || toolSettings["write"] != false {
		testContext.Fatalf("capability overlay: %#v", toolSettings)
	}
	if resolvedSettings["devFlow"].(map[string]any)["enabled"] != false {
		testContext.Fatal("devFlow must remain global-only")
	}
}

func TestResetRestoresInheritedValueAndPreservesUnknownKeys(testContext *testing.T) {
	tempDir := testContext.TempDir()
	globalPath := filepath.Join(tempDir, "global.json")
	projectPath := filepath.Join(tempDir, "nested", "project.json")
	writeFixture(testContext, globalPath, `{"defaultModel":"global","transport":"sse"}`)
	writeFixture(testContext, projectPath, `{"defaultModel":"project","future":{"preserve":true}}`)
	manager, err := Load(globalPath, &projectPath, nil)
	if err != nil {
		testContext.Fatal(err)
	}
	if err := manager.Reset(Project, "defaultModel"); err != nil {
		testContext.Fatal(err)
	}
	if manager.Resolved()["defaultModel"] != "global" {
		testContext.Fatalf("reset did not inherit global value: %#v", manager.Resolved())
	}
	persistedContents, err := os.ReadFile(projectPath)
	if err != nil {
		testContext.Fatal(err)
	}
	var projectSettings map[string]any
	if err := json.Unmarshal(persistedContents, &projectSettings); err != nil {
		testContext.Fatal(err)
	}
	if _, exists := projectSettings["defaultModel"]; exists || projectSettings["future"].(map[string]any)["preserve"] != true {
		testContext.Fatalf("reset persistence: %#v", projectSettings)
	}
}

func TestNestedResetRestoresInheritedCapabilityAndInvalidUpdateIsNotPersisted(testContext *testing.T) {
	tempDir := testContext.TempDir()
	globalPath := filepath.Join(tempDir, "global.json")
	projectPath := filepath.Join(tempDir, "project.json")
	writeFixture(testContext, globalPath, `{"tools":{"read":false},"appearance":{"fontSize":16}}`)
	writeFixture(testContext, projectPath, `{"tools":{"read":true}}`)
	manager, err := Load(globalPath, &projectPath, nil)
	if err != nil {
		testContext.Fatal(err)
	}
	if err := manager.Reset(Project, "tools.read"); err != nil {
		testContext.Fatal(err)
	}
	if manager.Resolved()["tools"].(map[string]any)["read"] != false {
		testContext.Fatalf("nested reset did not inherit: %#v", manager.Resolved())
	}
	beforeContents, err := os.ReadFile(projectPath)
	if err != nil {
		testContext.Fatal(err)
	}
	if err := manager.Set(Project, "appearance.fontSize", json.Number("31")); err == nil {
		testContext.Fatal("expected invalid settings update")
	}
	afterContents, err := os.ReadFile(projectPath)
	if err != nil {
		testContext.Fatal(err)
	}
	if string(beforeContents) != string(afterContents) {
		testContext.Fatal("invalid settings update changed the persisted file")
	}
}

func TestMissingSettingsUseDefaultsButParseAndSchemaFailuresAreVisible(testContext *testing.T) {
	tempDir := testContext.TempDir()
	manager, err := Load(filepath.Join(tempDir, "missing.json"), nil, nil)
	if err != nil {
		testContext.Fatal(err)
	}
	if manager.Resolved()["transport"] != "auto" {
		testContext.Fatalf("unexpected defaults: %#v", manager.Resolved())
	}
	malformedPath := filepath.Join(tempDir, "malformed.json")
	writeFixture(testContext, malformedPath, `{"transport":`)
	if _, err := Load(malformedPath, nil, nil); err == nil {
		testContext.Fatal("expected parse error")
	} else {
		var settingsError *Error
		if !errors.As(err, &settingsError) || settingsError.Kind != ParseError {
			testContext.Fatalf("wrong error: %v", err)
		}
	}
	invalidPath := filepath.Join(tempDir, "invalid.json")
	writeFixture(testContext, invalidPath, `{"appearance":{"themeMode":"sepia"}}`)
	if _, err := Load(invalidPath, nil, nil); err == nil {
		testContext.Fatal("expected schema validation error")
	}
}

func TestReadAndWriteFailuresRetainTheirKinds(testContext *testing.T) {
	tempDir := testContext.TempDir()
	readPath := filepath.Join(tempDir, "directory")
	if err := os.Mkdir(readPath, 0o700); err != nil {
		testContext.Fatal(err)
	}
	if _, err := Load(readPath, nil, nil); err == nil {
		testContext.Fatal("expected read error for directory")
	} else {
		var settingsError *Error
		if !errors.As(err, &settingsError) || settingsError.Kind != ReadError {
			testContext.Fatalf("wrong error: %v", err)
		}
	}
	globalPath := filepath.Join(tempDir, "write-target")
	manager, err := Load(globalPath, nil, nil)
	if err != nil {
		testContext.Fatal(err)
	}
	if err := os.Mkdir(globalPath, 0o700); err != nil {
		testContext.Fatal(err)
	}
	if err := manager.SaveGlobal(); err == nil {
		testContext.Fatal("expected write error")
	} else {
		var settingsError *Error
		if !errors.As(err, &settingsError) || settingsError.Kind != WriteError {
			testContext.Fatalf("wrong error: %v", err)
		}
	}
}

func TestValidationRejectsInvalidCompactionAndAppearance(testContext *testing.T) {
	tempDir := testContext.TempDir()
	for _, fixtureBody := range []string{`{"compaction":{"triggerRatio":0.3,"targetRatio":0.7}}`, `{"appearance":{"fontSize":31}}`} {
		settingsPath := filepath.Join(tempDir, "settings.json")
		writeFixture(testContext, settingsPath, fixtureBody)
		if _, err := Load(settingsPath, nil, nil); err == nil {
			testContext.Fatalf("expected rejection for %s", fixtureBody)
		}
	}
}
