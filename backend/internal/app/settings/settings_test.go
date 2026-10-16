package settings

import (
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"testing"
)

func writeFixture(t *testing.T, path, contents string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, []byte(contents), 0o600); err != nil {
		t.Fatal(err)
	}
}

func TestLoadLayersPrecedenceAndNestedOverlay(t *testing.T) {
	dir := t.TempDir()
	global, project, local := filepath.Join(dir, "global.json"), filepath.Join(dir, "project.json"), filepath.Join(dir, "local.json")
	writeFixture(t, global, `{"defaultModel":"global","transport":"sse","appearance":{"themeMode":"dark","fontSize":16},"tools":{"read":false,"write":false},"devFlow":{"enabled":false}}`)
	writeFixture(t, project, `{"defaultModel":"project","appearance":{"fontSize":18},"tools":{"read":true},"devFlow":{"enabled":true}}`)
	writeFixture(t, local, `{"defaultModel":"local","tools":{"read":false}}`)
	m, err := Load(global, &project, &local)
	if err != nil {
		t.Fatal(err)
	}
	got := m.Resolved()
	if got["defaultModel"] != "local" || got["transport"] != "sse" {
		t.Fatalf("precedence: %#v", got)
	}
	appearance := got["appearance"].(map[string]any)
	if appearance["themeMode"] != "dark" || appearance["fontSize"] != json.Number("18") {
		t.Fatalf("nested overlay: %#v", appearance)
	}
	tools := got["tools"].(map[string]any)
	if tools["read"] != false || tools["write"] != false {
		t.Fatalf("capability overlay: %#v", tools)
	}
	if got["devFlow"].(map[string]any)["enabled"] != false {
		t.Fatal("devFlow must remain global-only")
	}
}

func TestResetRestoresInheritedValueAndPreservesUnknownKeys(t *testing.T) {
	dir := t.TempDir()
	global, project := filepath.Join(dir, "global.json"), filepath.Join(dir, "nested", "project.json")
	writeFixture(t, global, `{"defaultModel":"global","transport":"sse"}`)
	writeFixture(t, project, `{"defaultModel":"project","future":{"preserve":true}}`)
	m, err := Load(global, &project, nil)
	if err != nil {
		t.Fatal(err)
	}
	if err := m.Reset(Project, "defaultModel"); err != nil {
		t.Fatal(err)
	}
	if m.Resolved()["defaultModel"] != "global" {
		t.Fatalf("reset did not inherit global value: %#v", m.Resolved())
	}
	persisted, err := os.ReadFile(project)
	if err != nil {
		t.Fatal(err)
	}
	var layer map[string]any
	if err := json.Unmarshal(persisted, &layer); err != nil {
		t.Fatal(err)
	}
	if _, exists := layer["defaultModel"]; exists || layer["future"].(map[string]any)["preserve"] != true {
		t.Fatalf("reset persistence: %#v", layer)
	}
}

func TestNestedResetRestoresInheritedCapabilityAndInvalidUpdateIsNotPersisted(t *testing.T) {
	dir := t.TempDir()
	global, project := filepath.Join(dir, "global.json"), filepath.Join(dir, "project.json")
	writeFixture(t, global, `{"tools":{"read":false},"appearance":{"fontSize":16}}`)
	writeFixture(t, project, `{"tools":{"read":true}}`)
	m, err := Load(global, &project, nil)
	if err != nil {
		t.Fatal(err)
	}
	if err := m.Reset(Project, "tools.read"); err != nil {
		t.Fatal(err)
	}
	if m.Resolved()["tools"].(map[string]any)["read"] != false {
		t.Fatalf("nested reset did not inherit: %#v", m.Resolved())
	}
	before, err := os.ReadFile(project)
	if err != nil {
		t.Fatal(err)
	}
	if err := m.Set(Project, "appearance.fontSize", json.Number("31")); err == nil {
		t.Fatal("expected invalid settings update")
	}
	after, err := os.ReadFile(project)
	if err != nil {
		t.Fatal(err)
	}
	if string(before) != string(after) {
		t.Fatal("invalid settings update changed the persisted file")
	}
}

func TestMissingSettingsUseDefaultsButParseAndSchemaFailuresAreVisible(t *testing.T) {
	dir := t.TempDir()
	m, err := Load(filepath.Join(dir, "missing.json"), nil, nil)
	if err != nil {
		t.Fatal(err)
	}
	if m.Resolved()["transport"] != "auto" {
		t.Fatalf("unexpected defaults: %#v", m.Resolved())
	}
	malformed := filepath.Join(dir, "malformed.json")
	writeFixture(t, malformed, `{"transport":`)
	if _, err := Load(malformed, nil, nil); err == nil {
		t.Fatal("expected parse error")
	} else {
		var typed *Error
		if !errors.As(err, &typed) || typed.Kind != ParseError {
			t.Fatalf("wrong error: %v", err)
		}
	}
	invalid := filepath.Join(dir, "invalid.json")
	writeFixture(t, invalid, `{"appearance":{"themeMode":"sepia"}}`)
	if _, err := Load(invalid, nil, nil); err == nil {
		t.Fatal("expected schema validation error")
	}
}

func TestReadAndWriteFailuresRetainTheirKinds(t *testing.T) {
	dir := t.TempDir()
	readPath := filepath.Join(dir, "directory")
	if err := os.Mkdir(readPath, 0o700); err != nil {
		t.Fatal(err)
	}
	if _, err := Load(readPath, nil, nil); err == nil {
		t.Fatal("expected read error for directory")
	} else {
		var typed *Error
		if !errors.As(err, &typed) || typed.Kind != ReadError {
			t.Fatalf("wrong error: %v", err)
		}
	}
	global := filepath.Join(dir, "write-target")
	m, err := Load(global, nil, nil)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.Mkdir(global, 0o700); err != nil {
		t.Fatal(err)
	}
	if err := m.SaveGlobal(); err == nil {
		t.Fatal("expected write error")
	} else {
		var typed *Error
		if !errors.As(err, &typed) || typed.Kind != WriteError {
			t.Fatalf("wrong error: %v", err)
		}
	}
}

func TestValidationRejectsInvalidCompactionAndAppearance(t *testing.T) {
	dir := t.TempDir()
	for _, body := range []string{`{"compaction":{"triggerRatio":0.3,"targetRatio":0.7}}`, `{"appearance":{"fontSize":31}}`} {
		path := filepath.Join(dir, "settings.json")
		writeFixture(t, path, body)
		if _, err := Load(path, nil, nil); err == nil {
			t.Fatalf("expected rejection for %s", body)
		}
	}
}
