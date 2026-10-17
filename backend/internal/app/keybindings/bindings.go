// Package keybindings defines and persists the product keyboard shortcuts.
package keybindings

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"
)

const (
	fileName    = "key_bindings.json"
	fileVersion = 1
)

type Action string

const (
	ToggleThinking  Action = "toggleThinking"
	OpenModelPicker Action = "openModelPicker"
	NewSession      Action = "newSession"
	OpenSettings    Action = "openSettings"
	SendMessage     Action = "sendMessage"
	InsertNewline   Action = "insertNewline"
	FocusComposer   Action = "focusComposer"
)

var allActions = [...]Action{ToggleThinking, OpenModelPicker, NewSession, OpenSettings, SendMessage, InsertNewline, FocusComposer}

type Scope string

const (
	Global          Scope = "global"
	Composer        Scope = "composer"
	OutsideComposer Scope = "outsideComposer"
)

type Definition struct {
	Action         Action `json:"action"`
	Title          string `json:"title"`
	Description    string `json:"description"`
	DefaultBinding string `json:"defaultBinding"`
	Binding        string `json:"binding"`
	Scope          Scope  `json:"scope"`
}

// FixedDefinition describes a displayed shortcut that users cannot edit.
type FixedDefinition struct {
	Binding string `json:"binding"`
	Title   string `json:"title"`
}

type bindingFile struct {
	Version  int               `json:"version"`
	Bindings map[Action]string `json:"bindings"`
}

type actionDefinition struct {
	title, description, binding string
	scope                       Scope
}

var definitions = map[Action]actionDefinition{
	ToggleThinking:  {"Toggle thinking", "Expand or collapse thinking blocks", "Ctrl+T", Global},
	OpenModelPicker: {"Choose model", "Open the model picker", "Ctrl+P", Global},
	NewSession:      {"New session", "Start a new session", "Ctrl+N", Global},
	OpenSettings:    {"Open settings", "Open or close Settings", "Ctrl+,", Global},
	SendMessage:     {"Send message", "Send from the focused composer", "Enter", Composer},
	InsertNewline:   {"Insert new line", "Add a line in the focused composer", "Shift+Enter", Composer},
	FocusComposer:   {"Focus composer", "Move focus to the message composer", "/", OutsideComposer},
}

var fixedDefinitions = [...]FixedDefinition{
	{"Escape", "Dismiss the current panel"},
	{"Double Escape", "Abort the active response"},
	{"Y / T / N / H", "Choose a permission response"},
	{"J / K / ↑ / ↓ / Enter", "Move through permission choices"},
	{"1–9 / N / D / Enter", "Choose or advance a question or trust option"},
	{"Tab / ↑ / ↓ / Enter", "Confirm or navigate autocomplete"},
}

// Path returns the keybindings file under the global configuration root.
func Path(globalConfigRoot string) string { return filepath.Join(globalConfigRoot, fileName) }

// Fixed returns the ordered, non-editable shortcuts displayed by settings.
func Fixed() []FixedDefinition { return append([]FixedDefinition(nil), fixedDefinitions[:]...) }

// Load overlays stored user bindings on the complete built-in registry.
func Load(path string) ([]Definition, error) {
	overrides, err := readOverrides(path)
	if err != nil {
		return nil, err
	}
	result := make([]Definition, 0, len(allActions))
	occupied := make(map[string]struct{}, len(allActions))
	for _, action := range allActions {
		metadata := definitions[action]
		binding, exists := overrides[action]
		if !exists {
			binding = metadata.binding
		}
		if err := validateBinding(binding); err != nil {
			return nil, err
		}
		collisionKey := normalize(binding)
		if _, exists := occupied[collisionKey]; exists {
			return nil, fmt.Errorf("Duplicate keyboard shortcut: %s", binding)
		}
		occupied[collisionKey] = struct{}{}
		result = append(result, Definition{action, metadata.title, metadata.description, metadata.binding, binding, metadata.scope})
	}
	return result, nil
}

// Update validates and persists one user override, then returns the effective registry.
func Update(path string, action Action, binding string) ([]Definition, error) {
	metadata, supported := definitions[action]
	if !supported {
		return nil, fmt.Errorf("Unsupported keyboard shortcut action: %s", action)
	}
	if err := validateBinding(binding); err != nil {
		return nil, err
	}
	current, err := Load(path)
	if err != nil {
		return nil, err
	}
	for _, definition := range current {
		if definition.Action != action && normalize(definition.Binding) == normalize(binding) {
			return nil, fmt.Errorf("%s is already assigned to %s", binding, definition.Title)
		}
	}
	overrides := overridesFrom(current)
	if binding == metadata.binding {
		delete(overrides, action)
	} else {
		overrides[action] = binding
	}
	if err := writeOverrides(path, overrides); err != nil {
		return nil, err
	}
	return Load(path)
}

// Reset removes the selected override, restoring its default binding.
func Reset(path string, action Action) ([]Definition, error) {
	if _, supported := definitions[action]; !supported {
		return nil, fmt.Errorf("Unsupported keyboard shortcut action: %s", action)
	}
	current, err := Load(path)
	if err != nil {
		return nil, err
	}
	overrides := overridesFrom(current)
	delete(overrides, action)
	if err := writeOverrides(path, overrides); err != nil {
		return nil, err
	}
	return Load(path)
}

func readOverrides(path string) (map[Action]string, error) {
	if _, err := os.Stat(path); err != nil {
		return map[Action]string{}, nil
	}
	contents, err := os.ReadFile(path)
	if errors.Is(err, os.ErrNotExist) {
		return map[Action]string{}, nil
	}
	if err != nil {
		return nil, fmt.Errorf("Failed to read %s: %w", path, err)
	}
	decoder := json.NewDecoder(bytes.NewReader(contents))
	var stored bindingFile
	if err := decoder.Decode(&stored); err != nil {
		return nil, fmt.Errorf("Invalid %s: %w", path, err)
	}
	var extra any
	if err := decoder.Decode(&extra); !errors.Is(err, io.EOF) {
		if err == nil {
			err = errors.New("multiple JSON values")
		}
		return nil, fmt.Errorf("Invalid %s: %w", path, err)
	}
	if stored.Version != fileVersion {
		return nil, fmt.Errorf("Unsupported keyboard shortcut file version %d in %s", stored.Version, path)
	}
	for action := range stored.Bindings {
		if _, supported := definitions[action]; !supported {
			return nil, fmt.Errorf("Invalid %s: unsupported keyboard shortcut action %q", path, action)
		}
	}
	return stored.Bindings, nil
}

func overridesFrom(current []Definition) map[Action]string {
	overrides := make(map[Action]string)
	for _, definition := range current {
		if definition.Binding != definition.DefaultBinding {
			overrides[definition.Action] = definition.Binding
		}
	}
	return overrides
}

func writeOverrides(path string, overrides map[Action]string) error {
	parent := filepath.Dir(path)
	if err := os.MkdirAll(parent, 0o777); err != nil {
		return fmt.Errorf("Failed to create %s: %w", parent, err)
	}
	contents, err := json.MarshalIndent(bindingFile{Version: fileVersion, Bindings: overrides}, "", "  ")
	if err != nil {
		return fmt.Errorf("Failed to serialize keyboard shortcuts: %w", err)
	}
	contents = append(contents, '\n')
	if err := os.WriteFile(path, contents, 0o666); err != nil {
		return fmt.Errorf("Failed to write %s: %w", path, err)
	}
	return nil
}

func normalize(binding string) string { return strings.ToLower(binding) }

func validateBinding(binding string) error {
	if strings.TrimSpace(binding) != binding || binding == "" {
		return errors.New("Keyboard shortcut cannot be empty or padded with spaces")
	}
	parts := strings.Split(binding, "+")
	key := parts[len(parts)-1]
	if key == "" || key == "Ctrl" || key == "Meta" || key == "Alt" || key == "Shift" {
		return errors.New("Keyboard shortcut must end with a non-modifier key")
	}
	seen := make(map[string]struct{}, len(parts)-1)
	for _, modifier := range parts[:len(parts)-1] {
		if modifier != "Ctrl" && modifier != "Meta" && modifier != "Alt" && modifier != "Shift" {
			return fmt.Errorf("Unsupported keyboard shortcut modifier: %s", modifier)
		}
		if _, exists := seen[modifier]; exists {
			return fmt.Errorf("Duplicate keyboard shortcut modifier: %s", modifier)
		}
		seen[modifier] = struct{}{}
	}
	if len([]rune(key)) > 1 && key != "Enter" && key != "Escape" && key != "Tab" && key != "Space" && key != "ArrowUp" && key != "ArrowDown" && key != "ArrowLeft" && key != "ArrowRight" && key != "Backspace" && key != "Delete" && !strings.HasPrefix(key, "F") {
		return fmt.Errorf("Unsupported keyboard shortcut key: %s", key)
	}
	return nil
}
