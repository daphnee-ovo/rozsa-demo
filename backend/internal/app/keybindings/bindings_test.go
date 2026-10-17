package keybindings

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func definitionFor(testContext *testing.T, definitions []Definition, action Action) Definition {
	testContext.Helper()
	for _, definition := range definitions {
		if definition.Action == action {
			return definition
		}
	}
	testContext.Fatalf("missing action %q", action)
	return Definition{}
}

func TestLoadDefaultsAndInheritedDefinitions(testContext *testing.T) {
	path := filepath.Join(testContext.TempDir(), "missing", fileName)
	loaded, err := Load(path)
	if err != nil {
		testContext.Fatal(err)
	}
	if len(loaded) != 7 {
		testContext.Fatalf("loaded %d definitions, want 7", len(loaded))
	}
	defaults := map[Action]string{
		ToggleThinking: "Ctrl+T", OpenModelPicker: "Ctrl+P", NewSession: "Ctrl+N",
		OpenSettings: "Ctrl+,", SendMessage: "Enter", InsertNewline: "Shift+Enter", FocusComposer: "/",
	}
	for action, binding := range defaults {
		definition := definitionFor(testContext, loaded, action)
		if definition.Binding != binding || definition.DefaultBinding != binding {
			testContext.Errorf("%s binding/default = %q/%q, want %q", action, definition.Binding, definition.DefaultBinding, binding)
		}
	}
	if definitionFor(testContext, loaded, SendMessage).Scope != Composer || definitionFor(testContext, loaded, InsertNewline).Scope != Composer || definitionFor(testContext, loaded, FocusComposer).Scope != OutsideComposer {
		testContext.Fatal("composer scope assignments do not match the Rust definitions")
	}
	if _, err := os.Stat(path); !os.IsNotExist(err) {
		testContext.Fatalf("loading defaults created the override file: %v", err)
	}
	if Path("/global/config") != filepath.Join("/global/config", fileName) {
		testContext.Fatalf("global path = %q", Path("/global/config"))
	}
	fixed := Fixed()
	if len(fixed) != 6 || fixed[0] != (FixedDefinition{"Escape", "Dismiss the current panel"}) || fixed[5] != (FixedDefinition{"Tab / ↑ / ↓ / Enter", "Confirm or navigate autocomplete"}) {
		testContext.Fatalf("fixed shortcut definitions differ from Rust: %#v", fixed)
	}
	fixed[0].Binding = "changed"
	if Fixed()[0].Binding != "Escape" {
		testContext.Fatal("Fixed exposed mutable package definitions")
	}
}

func TestUpdatePersistsOnlyOverridesAndResetRestoresDefaults(testContext *testing.T) {
	path := filepath.Join(testContext.TempDir(), "nested", fileName)
	updated, err := Update(path, NewSession, "Ctrl+Shift+N")
	if err != nil {
		testContext.Fatal(err)
	}
	if got := definitionFor(testContext, updated, NewSession).Binding; got != "Ctrl+Shift+N" {
		testContext.Fatalf("updated binding = %q", got)
	}
	contents, err := os.ReadFile(path)
	if err != nil {
		testContext.Fatal(err)
	}
	var stored bindingFile
	if err := json.Unmarshal(contents, &stored); err != nil {
		testContext.Fatal(err)
	}
	if stored.Version != fileVersion || len(stored.Bindings) != 1 || stored.Bindings[NewSession] != "Ctrl+Shift+N" {
		testContext.Fatalf("persisted file contains non-override data: %#v", stored)
	}
	reset, err := Reset(path, NewSession)
	if err != nil {
		testContext.Fatal(err)
	}
	if got := definitionFor(testContext, reset, NewSession).Binding; got != "Ctrl+N" {
		testContext.Fatalf("reset binding = %q", got)
	}
	contents, err = os.ReadFile(path)
	if err != nil {
		testContext.Fatal(err)
	}
	if strings.Contains(string(contents), "newSession") {
		testContext.Fatalf("reset retained the override: %s", contents)
	}
}

func TestValidationConflictAndExistingFileErrors(testContext *testing.T) {
	path := filepath.Join(testContext.TempDir(), fileName)
	before, err := Load(path)
	if err != nil {
		testContext.Fatal(err)
	}
	if _, err := Update(path, NewSession, "Ctrl+t"); err == nil || !strings.Contains(err.Error(), "already assigned") {
		testContext.Fatalf("case-insensitive conflict error = %v", err)
	}
	for _, binding := range []string{"", " Ctrl+X", "Ctrl+", "Ctrl+Ctrl+X", "Cmd+X", "Ctrl+UnknownKey", "Ctrl+Meta"} {
		if _, err := Update(path, NewSession, binding); err == nil {
			testContext.Errorf("accepted invalid binding %q", binding)
		}
	}
	after, err := Load(path)
	if err != nil {
		testContext.Fatal(err)
	}
	if definitionFor(testContext, before, NewSession).Binding != definitionFor(testContext, after, NewSession).Binding {
		testContext.Fatal("rejected updates changed the effective registry")
	}
	if _, err := Update(path, Action("unknownAction"), "Ctrl+X"); err == nil {
		testContext.Fatal("accepted an unknown action")
	}
	if err := os.WriteFile(path, []byte("{invalid"), 0o600); err != nil {
		testContext.Fatal(err)
	}
	if _, err := Load(path); err == nil || !strings.Contains(err.Error(), "Invalid") {
		testContext.Fatalf("malformed file error = %v", err)
	}
	for _, fixture := range []string{`{"version":2,"bindings":{}}`, `{"version":1,"bindings":{"unknownAction":"Ctrl+X"}}`, `{"version":1,"bindings":{}} {}`} {
		if err := os.WriteFile(path, []byte(fixture), 0o600); err != nil {
			testContext.Fatal(err)
		}
		if _, err := Load(path); err == nil {
			testContext.Errorf("accepted invalid file %s", fixture)
		}
	}
}

func TestPlatformKeyNamesAndModifiersRemainValid(testContext *testing.T) {
	for _, binding := range []string{"Ctrl+Enter", "Meta+Alt+F11", "Shift+ArrowUp", "Ctrl+,"} {
		if err := validateBinding(binding); err != nil {
			testContext.Errorf("rejected supported binding %q: %v", binding, err)
		}
	}
}

func TestLoadRejectsDuplicatePersistedBindings(testContext *testing.T) {
	path := filepath.Join(testContext.TempDir(), fileName)
	contents := `{"version":1,"bindings":{"newSession":"Ctrl+t"}}`
	if err := os.WriteFile(path, []byte(contents), 0o600); err != nil {
		testContext.Fatal(err)
	}
	if _, err := Load(path); err == nil || !strings.Contains(err.Error(), "Duplicate") {
		testContext.Fatalf("duplicate persisted binding error = %v", err)
	}
}

func TestWriteFailureIsReturnedWithoutSuccessResult(testContext *testing.T) {
	path := filepath.Join(testContext.TempDir(), "not-a-directory", fileName)
	if err := os.WriteFile(filepath.Dir(path), []byte("file"), 0o600); err != nil {
		testContext.Fatal(err)
	}
	updated, err := Update(path, NewSession, "Ctrl+Shift+N")
	if err == nil || updated != nil || !strings.Contains(err.Error(), "Failed to create") {
		testContext.Fatalf("write failure result = (%v, %v)", updated, err)
	}
}
