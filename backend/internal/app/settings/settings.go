// Package settings loads and persists layered product settings.
package settings

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"io/fs"
	"os"
	"path/filepath"
	"strconv"
	"strings"
)

type Scope string

const (
	Global  Scope = "global"
	Project Scope = "project"
	Local   Scope = "local"
)

type ErrorKind string

const (
	ReadError       ErrorKind = "read"
	ParseError      ErrorKind = "parse"
	WriteError      ErrorKind = "write"
	ValidationError ErrorKind = "validation"
)

// Error preserves the failure class and file involved for callers.
type Error struct {
	Kind ErrorKind
	Path string
	Err  error
}

func (e *Error) Error() string {
	if e.Path == "" {
		return fmt.Sprintf("settings %s error: %v", e.Kind, e.Err)
	}
	return fmt.Sprintf("settings %s error at %s: %v", e.Kind, e.Path, e.Err)
}
func (e *Error) Unwrap() error { return e.Err }

// Manager owns the merged view and the paths for its independent layers.
type Manager struct {
	paths    map[Scope]string
	resolved map[string]any
}

// Load reads defaults, global, project, then local settings. Missing files are
// empty layers; other read, parse, and schema failures are returned.
func Load(globalPath string, projectPath, localPath *string) (*Manager, error) {
	paths := map[Scope]string{Global: globalPath}
	if projectPath != nil {
		paths[Project] = *projectPath
	}
	if localPath != nil {
		paths[Local] = *localPath
	}
	resolved, err := resolve(paths, nil, nil)
	if err != nil {
		return nil, err
	}
	return &Manager{paths: paths, resolved: resolved}, nil
}

// Resolved returns a deep copy so callers cannot bypass validation by mutation.
func (m *Manager) Resolved() map[string]any { return cloneObject(m.resolved) }

func (m *Manager) Path(scope Scope) (string, bool) { p, ok := m.paths[scope]; return p, ok }

// Overrides returns only values explicitly written at the requested scope.
func (m *Manager) Overrides(scope Scope) (map[string]any, error) {
	path, ok := m.paths[scope]
	if !ok {
		return nil, fmt.Errorf("settings path for %s scope is unavailable", scope)
	}
	value, err := readLayer(path)
	if err != nil {
		return nil, err
	}
	return normalizeAliases(value), nil
}

// Set writes an override (a dot-separated path may address a nested field),
// while preserving unrelated and unknown keys.
// A nil value removes the override so the prior layer becomes effective.
func (m *Manager) Set(scope Scope, key string, value any) error {
	path, ok := m.paths[scope]
	if !ok {
		return fmt.Errorf("settings path for %s scope is unavailable", scope)
	}
	keys := strings.Split(key, ".")
	if key == "" {
		return errors.New("settings key must be non-empty")
	}
	for _, part := range keys {
		if part == "" {
			return errors.New("settings key path must not contain empty fields")
		}
	}
	root, err := readLayer(path)
	if err != nil {
		return err
	}
	if err := setPath(root, keys, value); err != nil {
		return err
	}
	candidate, err := resolve(m.paths, &scope, root)
	if err != nil {
		return err
	}
	if err := writeObject(path, root); err != nil {
		return err
	}
	m.resolved = candidate
	return nil
}

func (m *Manager) Reset(scope Scope, key string) error { return m.Set(scope, key, nil) }

// SaveGlobal writes the fully resolved settings at the global path.
func (m *Manager) SaveGlobal() error { return writePretty(m.paths[Global], m.resolved, false) }

func optionalPath(paths map[Scope]string, scope Scope) *string {
	if path, ok := paths[scope]; ok {
		return &path
	}
	return nil
}

func resolve(paths map[Scope]string, replaceScope *Scope, replacement map[string]any) (map[string]any, error) {
	resolved := defaults()
	globalDevFlow := cloneObject(resolved["devFlow"].(map[string]any))
	for _, scope := range []Scope{Global, Project, Local} {
		file, exists := paths[scope]
		if !exists {
			continue
		}
		var layer map[string]any
		var err error
		if replaceScope != nil && *replaceScope == scope {
			layer = replacement
		} else {
			layer, err = readLayer(file)
		}
		if err != nil {
			return nil, err
		}
		resolved = merge(resolved, normalizeAliases(layer))
		if scope == Global {
			globalDevFlow = cloneObject(resolved["devFlow"].(map[string]any))
		}
	}
	resolved["devFlow"] = globalDevFlow
	if err := Validate(resolved); err != nil {
		return nil, err
	}
	return resolved, nil
}

func setPath(root map[string]any, keys []string, value any) error {
	current := root
	for index, key := range keys[:len(keys)-1] {
		next, exists := current[key]
		if !exists || next == nil {
			if value == nil {
				return nil
			}
			nested := map[string]any{}
			current[key] = nested
			current = nested
			continue
		}
		nested, ok := next.(map[string]any)
		if !ok {
			return fmt.Errorf("settings.%s must be an object", strings.Join(keys[:index+1], "."))
		}
		current = nested
	}
	last := keys[len(keys)-1]
	if value == nil {
		delete(current, last)
	} else {
		current[last] = value
	}
	return nil
}

func readLayer(path string) (map[string]any, error) {
	data, err := os.ReadFile(path)
	if errors.Is(err, fs.ErrNotExist) {
		return map[string]any{}, nil
	}
	if err != nil {
		return nil, &Error{Kind: ReadError, Path: path, Err: err}
	}
	var root map[string]any
	decoder := json.NewDecoder(bytes.NewReader(data))
	decoder.UseNumber()
	if err := decoder.Decode(&root); err != nil {
		return nil, &Error{Kind: ParseError, Path: path, Err: err}
	}
	if root == nil {
		return nil, &Error{Kind: ValidationError, Path: path, Err: errors.New("settings root must be an object")}
	}
	var trailing any
	if err := decoder.Decode(&trailing); err == nil {
		return nil, &Error{Kind: ParseError, Path: path, Err: errors.New("multiple JSON values")}
	} else if !errors.Is(err, io.EOF) {
		return nil, &Error{Kind: ParseError, Path: path, Err: err}
	}
	if err := validateLayer(root, path); err != nil {
		return nil, err
	}
	return root, nil
}

func writeObject(path string, value map[string]any) error {
	return writePretty(path, value, true)
}

func writePretty(path string, value map[string]any, trailingNewline bool) error {
	parent := filepath.Dir(path)
	if err := os.MkdirAll(parent, 0o777); err != nil {
		return &Error{Kind: WriteError, Path: parent, Err: err}
	}
	data, err := json.MarshalIndent(value, "", "  ")
	if err != nil {
		return &Error{Kind: WriteError, Path: path, Err: err}
	}
	if trailingNewline {
		data = append(data, '\n')
	}
	if err := os.WriteFile(path, data, 0o666); err != nil {
		return &Error{Kind: WriteError, Path: path, Err: err}
	}
	return nil
}

func defaults() map[string]any {
	return map[string]any{
		"defaultProvider": nil, "defaultModel": nil, "smallModel": nil, "defaultThinkingEffort": nil,
		"compaction": map[string]any{"enabled": true, "triggerRatio": json.Number("0.85"), "targetRatio": json.Number("0.30")},
		"retry":      map[string]any{"timeoutMs": nil, "maxRetries": json.Number("3"), "maxRetryDelayMs": json.Number("60000")},
		"transport":  "auto", "blockImages": false, "hideThinking": false, "autoSessionNaming": true,
		"steeringMode": "one-at-a-time", "followUpMode": "one-at-a-time", "runningSendMode": "queue",
		"permission":               map[string]any{"deny": []any{}, "ask": []any{}, "allow": defaultAllowRules(), "mode": "on-request"},
		"contextWindowPreferences": map[string]any{}, "lspMode": "disabled", "tools": map[string]any{}, "skills": map[string]any{},
		"appearance": map[string]any{"themeMode": "system", "fontSize": json.Number("14"), "translucentSidebar": false, "showRateLimits": true, "showHourlyRateLimit": true, "showWeeklyRateLimit": true, "rateLimitDisplayMode": "remained", "lightTheme": "rozsa", "darkTheme": "rozsa-dark"},
		"devFlow":    map[string]any{"enabled": true, "showSidebarStatus": true, "showDashboardButton": true, "executablePath": nil},
	}
}

func defaultAllowRules() []any {
	rules := []any{"subagent(*)", "askUserQuestion(*)"}
	for _, command := range []string{"head", "tail", "cat", "grep", "sort", "pwd", "ls", "basename", "dirname", "realpath", "readlink", "stat", "file", "wc", "diff", "cmp", "comm", "cut", "tr", "uniq", "strings", "od", "xxd"} {
		rules = append(rules, "Bash("+command+"*)")
	}
	return rules
}

func merge(base, overlay map[string]any) map[string]any {
	result := cloneObject(base)
	for key, value := range overlay {
		if value == nil {
			continue
		}
		if key == "contextWindowPreferences" {
			result[key] = clone(value)
			continue
		}
		if nested, ok := value.(map[string]any); ok {
			if current, ok := result[key].(map[string]any); ok {
				result[key] = merge(current, nested)
			} else {
				result[key] = cloneObject(nested)
			}
			continue
		}
		if key == "tools" || key == "skills" {
			if additions, ok := value.(map[string]any); ok {
				result[key] = merge(cloneObject(result[key].(map[string]any)), additions)
				continue
			}
		}
		result[key] = clone(value)
	}
	return result
}

func normalizeAliases(layer map[string]any) map[string]any {
	copy := cloneObject(layer)
	if value, ok := copy["permissions"]; ok {
		if _, also := copy["permission"]; !also {
			copy["permission"] = value
		}
		delete(copy, "permissions")
	}
	if value, ok := copy["default_thinking_level"]; ok {
		if _, also := copy["defaultThinkingEffort"]; !also {
			copy["defaultThinkingEffort"] = value
		}
		delete(copy, "default_thinking_level")
	}
	return copy
}

func cloneObject(input map[string]any) map[string]any {
	output := make(map[string]any, len(input))
	for key, value := range input {
		output[key] = clone(value)
	}
	return output
}

func clone(value any) any {
	switch v := value.(type) {
	case map[string]any:
		return cloneObject(v)
	case []any:
		out := make([]any, len(v))
		for i := range v {
			out[i] = clone(v[i])
		}
		return out
	default:
		return v
	}
}

// Validate checks the persisted settings fields that have product constraints.
// Unknown keys are intentionally accepted for forward-compatible settings.
func Validate(value map[string]any) error {
	if err := validateLayer(value, ""); err != nil {
		return err
	}
	compaction := object(value["compaction"])
	trigger := number(compaction["triggerRatio"])
	target := number(compaction["targetRatio"])
	if trigger <= 0 || trigger > 1 || target <= 0 || target > 1 {
		return invalid("compaction ratios must be greater than 0 and at most 1")
	}
	if target >= trigger {
		return invalid("compaction target ratio must be less than trigger ratio")
	}
	appearance := object(value["appearance"])
	mode := stringValue(appearance["themeMode"])
	if mode != "system" && mode != "light" && mode != "dark" {
		return invalid("invalid theme mode: " + mode)
	}
	size := integer(appearance["fontSize"])
	if size < 5 || size > 30 {
		return invalid(fmt.Sprintf("font size must be between 5 and 30: %d", size))
	}
	display := stringValue(appearance["rateLimitDisplayMode"])
	if display != "used" && display != "remained" {
		return invalid("invalid rate limit display mode: " + display)
	}
	for _, field := range []string{"lightTheme", "darkTheme"} {
		id := stringValue(appearance[field])
		if id == "" {
			return invalid("invalid " + field + " id: " + id)
		}
		for _, char := range id {
			if !(char >= 'a' && char <= 'z' || char >= 'A' && char <= 'Z' || char >= '0' && char <= '9' || char == '-' || char == '_') {
				return invalid("invalid " + field + " id: " + id)
			}
		}
	}
	return nil
}

func validateLayer(value map[string]any, path string) error {
	for key, raw := range value {
		if raw == nil {
			continue
		}
		kind := ""
		switch key {
		case "defaultProvider", "defaultModel", "smallModel", "transport", "steeringMode", "followUpMode", "runningSendMode", "lspMode", "defaultThinkingEffort":
			kind = "string"
		case "blockImages", "hideThinking", "autoSessionNaming":
			kind = "bool"
		case "compaction":
			kind = "object"
		case "retry":
			kind = "object"
		case "permission":
			kind = "object"
		case "contextWindowPreferences", "tools", "skills":
			kind = "map"
		case "appearance", "devFlow":
			kind = "object"
		}
		if kind == "" {
			continue
		}
		if !matchesKind(raw, kind) {
			return validationAt(path, fmt.Sprintf("settings.%s must be %s", key, kind))
		}
		if key == "defaultThinkingEffort" {
			effort := stringValue(raw)
			if effort != "off" && effort != "low" && effort != "medium" && effort != "high" && effort != "xhigh" && effort != "max" {
				return validationAt(path, "invalid defaultThinkingEffort: "+effort)
			}
		}
		switch key {
		case "compaction":
			fields := object(raw)
			if err := checkFields(fields, map[string]string{"enabled": "bool", "triggerRatio": "number", "targetRatio": "number"}, path, "compaction"); err != nil {
				return err
			}
		case "retry":
			if err := checkFields(object(raw), map[string]string{"timeoutMs": "uint64", "maxRetries": "uint32", "maxRetryDelayMs": "uint64"}, path, "retry"); err != nil {
				return err
			}
		case "permission":
			if err := checkFields(object(raw), map[string]string{"deny": "strings", "ask": "strings", "allow": "strings", "mode": "string"}, path, "permission"); err != nil {
				return err
			}
		case "appearance":
			if err := checkFields(object(raw), map[string]string{"themeMode": "string", "fontSize": "uint", "translucentSidebar": "bool", "showRateLimits": "bool", "showHourlyRateLimit": "bool", "showWeeklyRateLimit": "bool", "rateLimitDisplayMode": "string", "lightTheme": "string", "darkTheme": "string"}, path, "appearance"); err != nil {
				return err
			}
		case "devFlow":
			if err := checkFields(object(raw), map[string]string{"enabled": "bool", "showSidebarStatus": "bool", "showDashboardButton": "bool", "executablePath": "string"}, path, "devFlow"); err != nil {
				return err
			}
		case "tools", "skills":
			for name, enabled := range object(raw) {
				if _, ok := enabled.(bool); !ok {
					return validationAt(path, fmt.Sprintf("settings.%s.%s must be bool", key, name))
				}
			}
		case "contextWindowPreferences":
			for name, v := range object(raw) {
				if !matchesKind(v, "uint") {
					return validationAt(path, fmt.Sprintf("settings.contextWindowPreferences.%s must be an unsigned integer", name))
				}
			}
		}
	}
	return nil
}

func checkFields(fields map[string]any, schema map[string]string, path, parent string) error {
	for name, kind := range schema {
		value, exists := fields[name]
		if !exists || value == nil {
			continue
		}
		if !matchesKind(value, kind) {
			return validationAt(path, fmt.Sprintf("settings.%s.%s must be %s", parent, name, kind))
		}
	}
	return nil
}
func matchesKind(value any, kind string) bool {
	switch kind {
	case "string":
		_, ok := value.(string)
		return ok
	case "bool":
		_, ok := value.(bool)
		return ok
	case "object", "map":
		_, ok := value.(map[string]any)
		return ok
	case "number":
		switch value.(type) {
		case json.Number, float64, int:
			return true
		}
		return false
	case "uint", "uint64", "uint32":
		n, ok := unsigned(value)
		if !ok {
			return false
		}
		if kind == "uint32" {
			return n <= uint64(^uint32(0))
		}
		return true
	case "strings":
		items, ok := value.([]any)
		if !ok {
			return false
		}
		for _, item := range items {
			if _, ok := item.(string); !ok {
				return false
			}
		}
		return true
	default:
		return false
	}
}
func unsigned(value any) (uint64, bool) {
	switch v := value.(type) {
	case json.Number:
		n, err := strconv.ParseUint(v.String(), 10, 64)
		return n, err == nil
	case uint64:
		return v, true
	case uint32:
		return uint64(v), true
	case int:
		return uint64(v), v >= 0
	case int64:
		return uint64(v), v >= 0
	case float64:
		if v < 0 || v != float64(uint64(v)) {
			return 0, false
		}
		return uint64(v), true
	default:
		return 0, false
	}
}
func numberAsInt(value any) (int64, error) {
	switch v := value.(type) {
	case json.Number:
		return v.Int64()
	case int:
		return int64(v), nil
	case float64:
		if v != float64(int64(v)) {
			return 0, errors.New("not an integer")
		}
		return int64(v), nil
	}
	return 0, errors.New("not an integer")
}
func number(value any) float64 {
	switch v := value.(type) {
	case json.Number:
		f, _ := v.Float64()
		return f
	case float64:
		return v
	case int:
		return float64(v)
	}
	return 0
}
func integer(value any) int           { n, _ := numberAsInt(value); return int(n) }
func stringValue(value any) string    { s, _ := value.(string); return s }
func object(value any) map[string]any { v, _ := value.(map[string]any); return v }
func invalid(message string) error    { return &Error{Kind: ValidationError, Err: errors.New(message)} }
func validationAt(path, message string) error {
	return &Error{Kind: ValidationError, Path: path, Err: errors.New(message)}
}
