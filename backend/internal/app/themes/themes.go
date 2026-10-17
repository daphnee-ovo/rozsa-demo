// Package themes owns theme definitions, layered persistence, and appearance
// selection. Platform appearance application belongs to the desktop host.
package themes

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"io/fs"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"unicode/utf8"

	"rozsa/backend/internal/app/settings"
)

const (
	BuiltinLightID = "rozsa"
	BuiltinDarkID  = "rozsa-dark"
)

type Mode string

const (
	Light Mode = "light"
	Dark  Mode = "dark"
)

func (mode Mode) valid() bool { return mode == Light || mode == Dark }

func (mode *Mode) UnmarshalJSON(data []byte) error {
	var value string
	if err := json.Unmarshal(data, &value); err != nil {
		return err
	}
	parsed := Mode(value)
	if !parsed.valid() {
		return fmt.Errorf("unknown theme mode %q", value)
	}
	*mode = parsed
	return nil
}

type Definition struct {
	ID         string            `json:"id"`
	Name       string            `json:"name"`
	Mode       Mode              `json:"mode"`
	Accent     string            `json:"accent"`
	Background string            `json:"background"`
	Foreground string            `json:"foreground"`
	UIFont     string            `json:"uiFont"`
	CodeFont   string            `json:"codeFont"`
	Variables  map[string]string `json:"variables"`
}

type Summary struct {
	ID   string `json:"id"`
	Name string `json:"name"`
	Mode Mode   `json:"mode"`
}

type ErrorKind string

const (
	ReadError       ErrorKind = "read"
	ParseError      ErrorKind = "parse"
	ValidationError ErrorKind = "validation"
	NotFoundError   ErrorKind = "not_found"
)

type Error struct {
	Kind ErrorKind
	Path string
	ID   string
	Err  error
}

func (themeError *Error) Error() string {
	switch themeError.Kind {
	case ReadError:
		return fmt.Sprintf("failed to read theme file %s: %v", themeError.Path, themeError.Err)
	case ParseError:
		return fmt.Sprintf("failed to parse theme file %s: %v", themeError.Path, themeError.Err)
	case NotFoundError:
		return fmt.Sprintf("theme %s was not found", themeError.ID)
	case ValidationError:
		if themeError.ID == "" {
			return fmt.Sprintf("invalid appearance: %v", themeError.Err)
		}
		return fmt.Sprintf("invalid theme %s: %v", themeError.ID, themeError.Err)
	default:
		return fmt.Sprintf("invalid theme %s: %v", themeError.ID, themeError.Err)
	}
}

func (themeError *Error) Unwrap() error { return themeError.Err }

// Store is the public persistence contract. Layer selection stays internal.
type Store interface {
	List() ([]Summary, error)
	Get(id string, mode Mode) (Definition, error)
	Save(theme Definition) error
}

type fileStore struct {
	roots        []string
	writableRoot string
}

func NewStore(globalRoot, projectRoot string) Store {
	roots := make([]string, 0, 2)
	if globalRoot != "" {
		roots = append(roots, globalRoot)
	}
	if projectRoot != "" && projectRoot != globalRoot {
		roots = append(roots, projectRoot)
	}
	return &fileStore{roots: roots, writableRoot: globalRoot}
}

func (store *fileStore) List() ([]Summary, error) {
	customByID := make(map[string]Summary)
	for _, root := range store.roots {
		entries, err := os.ReadDir(root)
		if errors.Is(err, fs.ErrNotExist) {
			continue
		}
		if err != nil {
			return nil, &Error{Kind: ReadError, Path: root, Err: err}
		}
		for _, entry := range entries {
			if filepath.Ext(entry.Name()) != ".json" {
				continue
			}
			id := strings.TrimSuffix(entry.Name(), ".json")
			path := filepath.Join(root, entry.Name())
			file, err := store.readFile(path, id)
			if err != nil {
				return nil, err
			}
			if err := validateID(id); err != nil {
				return nil, invalidTheme(id, err)
			}
			name := "Custom Theme"
			if file.Name != nil {
				name = *file.Name
			}
			customByID[id] = Summary{ID: id, Name: name, Mode: file.Mode}
		}
	}

	ids := make([]string, 0, len(customByID))
	for id := range customByID {
		ids = append(ids, id)
	}
	sort.Strings(ids)
	custom := make([]Summary, 0, len(ids))
	for _, id := range ids {
		custom = append(custom, customByID[id])
	}
	sort.SliceStable(custom, func(left, right int) bool { return custom[left].Name < custom[right].Name })

	return append([]Summary{
		{ID: BuiltinLightID, Name: "Rozsa", Mode: Light},
		{ID: BuiltinDarkID, Name: "Rozsa Dark", Mode: Dark},
	}, custom...), nil
}

func (store *fileStore) Get(id string, mode Mode) (Definition, error) {
	if err := validateID(id); err != nil {
		return Definition{}, invalidTheme(id, err)
	}
	if id == BuiltinLightID {
		if mode != Light {
			return Definition{}, invalidTheme(id, errors.New("Rozsa is a light theme"))
		}
		return builtin(Light), nil
	}
	if id == BuiltinDarkID {
		if mode != Dark {
			return Definition{}, invalidTheme(id, errors.New("Rozsa Dark is a dark theme"))
		}
		return builtin(Dark), nil
	}
	if !mode.valid() {
		return Definition{}, invalidTheme(id, fmt.Errorf("invalid requested mode %q", mode))
	}

	for index := len(store.roots) - 1; index >= 0; index-- {
		path := filepath.Join(store.roots[index], id+".json")
		_, err := os.Stat(path)
		if errors.Is(err, fs.ErrNotExist) {
			continue
		}
		if err != nil {
			return Definition{}, &Error{Kind: ReadError, Path: path, Err: err}
		}
		file, err := store.readFile(path, id)
		if err != nil {
			return Definition{}, err
		}
		if file.Mode != mode {
			return Definition{}, invalidTheme(id, fmt.Errorf("theme mode is %s, requested %s", file.Mode, mode))
		}
		return fromFile(id, file)
	}
	return Definition{}, &Error{Kind: NotFoundError, ID: id}
}

func (store *fileStore) Save(theme Definition) error {
	if store.writableRoot == "" {
		return &Error{Kind: ValidationError, Err: errors.New("global theme directory is required for saving")}
	}
	if err := validateID(theme.ID); err != nil {
		return invalidTheme(theme.ID, err)
	}
	if theme.ID == BuiltinLightID || theme.ID == BuiltinDarkID {
		return invalidTheme(theme.ID, errors.New("built-in themes are read-only"))
	}
	if err := validateDefinition(theme); err != nil {
		return invalidTheme(theme.ID, err)
	}
	if err := os.MkdirAll(store.writableRoot, 0o777); err != nil {
		return &Error{Kind: ReadError, Path: store.writableRoot, Err: err}
	}
	data, err := json.MarshalIndent(themeFileFromDefinition(theme), "", "  ")
	if err != nil {
		return invalidTheme(theme.ID, fmt.Errorf("cannot serialize theme: %w", err))
	}
	path := filepath.Join(store.writableRoot, theme.ID+".json")
	if err := os.WriteFile(path, data, 0o666); err != nil {
		return &Error{Kind: ReadError, Path: path, Err: err}
	}
	return nil
}

type themeFile struct {
	Mode       Mode              `json:"mode"`
	Name       *string           `json:"name,omitempty"`
	Accent     *string           `json:"accent,omitempty"`
	Background *string           `json:"background,omitempty"`
	Foreground *string           `json:"foreground,omitempty"`
	UIFont     *string           `json:"uiFont,omitempty"`
	CodeFont   *string           `json:"codeFont,omitempty"`
	Variables  map[string]string `json:"variables"`
}

func (store *fileStore) readFile(path, id string) (themeFile, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return themeFile{}, &Error{Kind: ReadError, Path: path, Err: err}
	}
	if !utf8.Valid(data) {
		return themeFile{}, &Error{Kind: ReadError, Path: path, Err: errors.New("theme file is not valid UTF-8")}
	}
	decoder := json.NewDecoder(bytes.NewReader(data))
	var file themeFile
	if err := decoder.Decode(&file); err != nil {
		return themeFile{}, &Error{Kind: ParseError, Path: path, Err: err}
	}
	var trailing any
	if err := decoder.Decode(&trailing); !errors.Is(err, io.EOF) {
		if err == nil {
			err = errors.New("multiple JSON values")
		}
		return themeFile{}, &Error{Kind: ParseError, Path: path, Err: err}
	}
	if file.Mode == "" {
		return themeFile{}, &Error{Kind: ParseError, Path: path, Err: errors.New("missing field 'mode'")}
	}
	if err := validateFile(id, file); err != nil {
		return themeFile{}, invalidTheme(id, err)
	}
	return file, nil
}

func fromFile(id string, file themeFile) (Definition, error) {
	theme := builtin(file.Mode)
	theme.ID = id
	theme.Name = id
	if file.Name != nil {
		theme.Name = *file.Name
	}
	if file.Accent != nil {
		theme.Accent = *file.Accent
	}
	if file.Background != nil {
		theme.Background = *file.Background
	}
	if file.Foreground != nil {
		theme.Foreground = *file.Foreground
	}
	if file.UIFont != nil {
		theme.UIFont = *file.UIFont
	}
	if file.CodeFont != nil {
		theme.CodeFont = *file.CodeFont
	}
	for key, value := range file.Variables {
		theme.Variables[key] = value
	}
	if err := validateDefinition(theme); err != nil {
		return Definition{}, invalidTheme(id, err)
	}
	return theme, nil
}

func themeFileFromDefinition(theme Definition) themeFile {
	name, accent := theme.Name, theme.Accent
	background, foreground := theme.Background, theme.Foreground
	uiFont, codeFont := theme.UIFont, theme.CodeFont
	return themeFile{Mode: theme.Mode, Name: &name, Accent: &accent, Background: &background,
		Foreground: &foreground, UIFont: &uiFont, CodeFont: &codeFont, Variables: cloneVariables(theme.Variables)}
}

func validateFile(id string, file themeFile) error {
	if err := validateID(id); err != nil {
		return err
	}
	if !file.Mode.valid() {
		return fmt.Errorf("invalid theme mode %q", file.Mode)
	}
	for key, value := range file.Variables {
		if err := validateVariable(key, value); err != nil {
			return err
		}
	}
	for _, value := range []*string{file.Accent, file.Background, file.Foreground, file.UIFont, file.CodeFont} {
		if value != nil {
			if err := validateCSSValue(*value); err != nil {
				return err
			}
		}
	}
	return nil
}

func validateDefinition(theme Definition) error {
	if !theme.Mode.valid() {
		return fmt.Errorf("invalid theme mode %q", theme.Mode)
	}
	for _, value := range []string{theme.Accent, theme.Background, theme.Foreground, theme.UIFont, theme.CodeFont} {
		if err := validateCSSValue(value); err != nil {
			return err
		}
	}
	for key, value := range theme.Variables {
		if err := validateVariable(key, value); err != nil {
			return err
		}
	}
	return nil
}

func validateCSSValue(value string) error {
	if strings.TrimSpace(value) == "" {
		return errors.New("CSS values cannot be empty")
	}
	if len(value) > 512 || strings.ContainsAny(value, "\n\r;{}") {
		return errors.New("CSS values contain unsupported characters or are too long")
	}
	return nil
}

func validateVariable(key, value string) error {
	if !strings.HasPrefix(key, "--") || len(key) < 3 {
		return fmt.Errorf("invalid CSS variable name: %s", key)
	}
	for _, character := range key[2:] {
		if !((character >= 'a' && character <= 'z') || (character >= 'A' && character <= 'Z') ||
			(character >= '0' && character <= '9') || character == '-') {
			return fmt.Errorf("invalid CSS variable name: %s", key)
		}
	}
	return validateCSSValue(value)
}

func validateID(id string) error {
	if id == "" || id == "." || id == ".." {
		return errors.New("theme id must contain only letters, numbers, '-' or '_'")
	}
	for _, character := range id {
		if !((character >= 'a' && character <= 'z') || (character >= 'A' && character <= 'Z') ||
			(character >= '0' && character <= '9') || character == '-' || character == '_') {
			return errors.New("theme id must contain only letters, numbers, '-' or '_'")
		}
	}
	return nil
}

func invalidTheme(id string, err error) *Error {
	return &Error{Kind: ValidationError, ID: id, Err: err}
}

func cloneVariables(source map[string]string) map[string]string {
	copy := make(map[string]string, len(source))
	for key, value := range source {
		copy[key] = value
	}
	return copy
}

func builtin(mode Mode) Definition {
	definition := Definition{
		Mode:     mode,
		UIFont:   "-apple-system, BlinkMacSystemFont, 'SF Pro Text', 'Segoe UI', system-ui, sans-serif",
		CodeFont: "'JetBrains Mono', 'Cascadia Code', 'Fira Code', ui-monospace, Menlo, monospace",
	}
	if mode == Light {
		definition.ID, definition.Name = BuiltinLightID, "Rozsa"
		definition.Accent, definition.Background, definition.Foreground = "#D7827E", "#FFFFFF", "#575279"
		definition.Variables = map[string]string{
			"--surface": "oklch(100% 0 0)", "--muted": "oklch(55% 0.01 350)", "--border": "oklch(90% 0.006 350)",
			"--accent-hover": "oklch(54% 0.08 355)", "--accent-btn": "oklch(50% 0.08 355)", "--accent-bg": "oklch(96% 0.02 355)",
			"--accent-border": "oklch(88% 0.035 355)", "--success": "oklch(48% 0.10 155)", "--success-bg": "oklch(96% 0.025 155)",
			"--error": "oklch(52% 0.14 25)", "--error-bg": "oklch(96% 0.025 25)", "--warning": "oklch(70% 0.12 85)",
			"--warning-bg": "oklch(97% 0.03 85)", "--user-bg": "oklch(94% 0.015 355)", "--code-bg": "oklch(96% 0.003 260)",
			"--code-border": "oklch(90% 0.005 260)", "--sidebar-bg": "oklch(97.5% 0.004 350)", "--titlebar-bg": "oklch(98.5% 0.003 350)",
		}
		return definition
	}
	definition.ID, definition.Name = BuiltinDarkID, "Rozsa Dark"
	definition.Accent, definition.Background, definition.Foreground = "#d88991", "#1d1a1c", "#f1e9eb"
	definition.Variables = map[string]string{
		"--surface": "#282326", "--muted": "#b6a8ad", "--border": "#493f43", "--accent-hover": "#efabb1",
		"--accent-btn": "#c8757e", "--accent-bg": "#3b282d", "--accent-border": "#66434a", "--success": "#82c59a",
		"--success-bg": "#20392a", "--error": "#f09a9a", "--error-bg": "#43292b", "--warning": "#e5bf75",
		"--warning-bg": "#413722", "--user-bg": "#35262c", "--code-bg": "#171517", "--code-border": "#40383c",
		"--sidebar-bg": "#211d1f", "--titlebar-bg": "#181618",
	}
	return definition
}

// SettingsOwner is the narrow public view themes needs from the settings
// module; layered storage remains owned by settings.Manager.
type SettingsOwner interface {
	Resolved() map[string]any
	Set(scope settings.Scope, key string, value any) error
}

type AppearanceSnapshot struct {
	ThemeMode          string     `json:"themeMode"`
	FontSize           int        `json:"fontSize"`
	TranslucentSidebar bool       `json:"translucentSidebar"`
	LightTheme         Definition `json:"lightTheme"`
	DarkTheme          Definition `json:"darkTheme"`
}

type Appearance interface {
	Snapshot() (AppearanceSnapshot, error)
	Select(scope settings.Scope, mode Mode, id string) (AppearanceSnapshot, error)
	Update(scope settings.Scope, field string, value any) (AppearanceSnapshot, error)
}

type appearanceService struct {
	store    Store
	settings SettingsOwner
}

func NewAppearance(store Store, owner SettingsOwner) Appearance {
	return &appearanceService{store: store, settings: owner}
}

// Select validates the requested theme before persisting its scoped selection.
// The returned snapshot is produced only after settings persistence succeeds;
// callers then apply platform appearance and publish it to the renderer.
func (service *appearanceService) Select(scope settings.Scope, mode Mode, id string) (AppearanceSnapshot, error) {
	if !mode.valid() {
		return AppearanceSnapshot{}, invalidTheme(id, fmt.Errorf("invalid theme mode %q", mode))
	}
	if _, err := service.store.Get(id, mode); err != nil {
		return AppearanceSnapshot{}, err
	}
	key := "appearance.lightTheme"
	if mode == Dark {
		key = "appearance.darkTheme"
	}
	if err := service.settings.Set(scope, key, id); err != nil {
		return AppearanceSnapshot{}, err
	}
	return service.Snapshot()
}

// Update persists one appearance preference and returns the resulting theme
// state. Theme IDs are validated before persistence, matching the Rust
// selection path; platform changes happen after this method returns.
func (service *appearanceService) Update(scope settings.Scope, field string, value any) (AppearanceSnapshot, error) {
	if field == "lightTheme" || field == "darkTheme" {
		id, ok := value.(string)
		if !ok {
			return AppearanceSnapshot{}, invalidTheme("", fmt.Errorf("appearance.%s must be a string", field))
		}
		mode := Light
		if field == "darkTheme" {
			mode = Dark
		}
		return service.Select(scope, mode, id)
	}
	var persistedValue any
	switch field {
	case "themeMode":
		mode, ok := value.(string)
		if !ok || !validAppearanceMode(mode) {
			return AppearanceSnapshot{}, &Error{Kind: ValidationError, Err: fmt.Errorf("invalid theme mode: %v", value)}
		}
		persistedValue = mode
	case "fontSize":
		fontSize, err := integer(value)
		if err != nil {
			return AppearanceSnapshot{}, &Error{Kind: ValidationError, Err: fmt.Errorf("invalid font size: %v", value)}
		}
		if fontSize < 5 || fontSize > 30 {
			return AppearanceSnapshot{}, &Error{Kind: ValidationError, Err: fmt.Errorf("font size must be between 5 and 30: %d", fontSize)}
		}
		persistedValue = fontSize
	case "translucentSidebar":
		translucentSidebar, ok := value.(bool)
		if !ok {
			return AppearanceSnapshot{}, &Error{Kind: ValidationError, Err: errors.New("appearance.translucentSidebar must be a boolean")}
		}
		persistedValue = translucentSidebar
	default:
		return AppearanceSnapshot{}, &Error{Kind: ValidationError, Err: fmt.Errorf("unknown appearance setting: %s", field)}
	}
	if err := service.settings.Set(scope, "appearance."+field, persistedValue); err != nil {
		return AppearanceSnapshot{}, err
	}
	return service.Snapshot()
}

// Snapshot resolves both selected definitions from the persisted settings.
// It does not resolve system light/dark preference or touch native UI state.
func (service *appearanceService) Snapshot() (AppearanceSnapshot, error) {
	resolved := service.settings.Resolved()
	appearance, ok := resolved["appearance"].(map[string]any)
	if !ok {
		return AppearanceSnapshot{}, &Error{Kind: ValidationError, Err: errors.New("settings.appearance must be an object")}
	}
	mode, ok := appearance["themeMode"].(string)
	if !ok || !validAppearanceMode(mode) {
		return AppearanceSnapshot{}, &Error{Kind: ValidationError, Err: fmt.Errorf("invalid theme mode %v", appearance["themeMode"])}
	}
	fontSize, err := integer(appearance["fontSize"])
	if err != nil {
		return AppearanceSnapshot{}, &Error{Kind: ValidationError, Err: fmt.Errorf("invalid appearance font size: %w", err)}
	}
	translucentSidebar, ok := appearance["translucentSidebar"].(bool)
	if !ok {
		return AppearanceSnapshot{}, &Error{Kind: ValidationError, Err: errors.New("appearance.translucentSidebar must be a boolean")}
	}
	lightID, ok := appearance["lightTheme"].(string)
	if !ok {
		return AppearanceSnapshot{}, &Error{Kind: ValidationError, Err: errors.New("appearance.lightTheme must be a string")}
	}
	darkID, ok := appearance["darkTheme"].(string)
	if !ok {
		return AppearanceSnapshot{}, &Error{Kind: ValidationError, Err: errors.New("appearance.darkTheme must be a string")}
	}
	lightTheme, err := service.store.Get(lightID, Light)
	if err != nil {
		return AppearanceSnapshot{}, err
	}
	darkTheme, err := service.store.Get(darkID, Dark)
	if err != nil {
		return AppearanceSnapshot{}, err
	}
	return AppearanceSnapshot{
		ThemeMode: mode, FontSize: fontSize, TranslucentSidebar: translucentSidebar,
		LightTheme: lightTheme, DarkTheme: darkTheme,
	}, nil
}

func validAppearanceMode(mode string) bool {
	return mode == "system" || mode == string(Light) || mode == string(Dark)
}

func integer(value any) (int, error) {
	switch number := value.(type) {
	case int:
		return number, nil
	case int8:
		return int(number), nil
	case int16:
		return int(number), nil
	case int32:
		return int(number), nil
	case int64:
		return int(number), nil
	case json.Number:
		parsed, err := number.Int64()
		if err == nil {
			return int(parsed), nil
		}
	case float64:
		if number == float64(int(number)) {
			return int(number), nil
		}
	}
	return 0, fmt.Errorf("expected an integer, got %T", value)
}
