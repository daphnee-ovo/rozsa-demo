package input

import (
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"unicode"
	"unicode/utf16"
	"unicode/utf8"

	"rozsa/backend/internal/app/skills"
)

type Suggestion struct {
	Value       string `json:"value"`
	Label       string `json:"label"`
	Description string `json:"description"`
}

type HighlightRange struct {
	Start int `json:"start"`
	End   int `json:"end"`
}

type Completion struct {
	Prefix          string           `json:"prefix"`
	Items           []Suggestion     `json:"items"`
	ValidMatch      bool             `json:"validMatch"`
	HighlightRanges []HighlightRange `json:"highlightRanges"`
}

type Request struct {
	Text   string
	Cursor int
	CWD    string
	Skills []skills.Skill
}

type FileSystem interface {
	ReadDir(name string) ([]os.DirEntry, error)
	Stat(name string) (os.FileInfo, error)
}

type Service interface {
	Complete(Request) (Completion, error)
}

type defaultFileSystem struct{}

func (defaultFileSystem) ReadDir(name string) ([]os.DirEntry, error) { return os.ReadDir(name) }
func (defaultFileSystem) Stat(name string) (os.FileInfo, error)      { return os.Stat(name) }

type completionService struct{ files FileSystem }

func NewService(files FileSystem) Service {
	if files == nil {
		files = defaultFileSystem{}
	}
	return completionService{files: files}
}

func (service completionService) Complete(request Request) (Completion, error) {
	result := Completion{Items: []Suggestion{}, HighlightRanges: []HighlightRange{}}
	cursor := utf16OffsetToByteIndex(request.Text, request.Cursor)
	head := request.Text[:cursor]
	result.HighlightRanges = service.highlightRanges(request.Text, request.CWD, request.Skills)

	if prefix, ok := slashPrefix(head); ok {
		result.Prefix = "/" + prefix
		result.Items = slashSuggestions(prefix, request.Skills)
		for _, item := range result.Items {
			if strings.TrimPrefix(strings.ToLower(item.Label), "/") == prefix {
				result.ValidMatch = true
				break
			}
		}
		return result, nil
	}
	if prefix, ok := filePrefix(head); ok {
		if request.CWD == "" {
			return Completion{}, fmt.Errorf("complete file reference %q: working directory is required", prefix)
		}
		items, err := service.fileSuggestions(prefix, request.CWD)
		if err != nil {
			return Completion{}, fmt.Errorf("complete file reference %q: %w", prefix, err)
		}
		result.Prefix = prefix
		result.Items = items
		path := strings.TrimPrefix(strings.TrimPrefix(prefix, `@"`), "@")
		path = strings.TrimSuffix(path, `"`)
		if path != "" {
			resolved := resolvePath(path, request.CWD)
			_, err := service.files.Stat(resolved)
			result.ValidMatch = err == nil
		}
		return result, nil
	}
	return result, nil
}

// IsSlashCompletion reports whether the text immediately before the UTF-16
// cursor is a slash token, so callers only load the skill catalog when needed.
func IsSlashCompletion(text string, cursor int) bool {
	byteIndex := utf16OffsetToByteIndex(text, cursor)
	_, ok := slashPrefix(text[:byteIndex])
	return ok
}

// HasSkillToken reports whether text contains an explicit /skill:<name> token.
// It lets callers load skill metadata for highlighting after the caret moves
// past the token, without loading the catalog for ordinary composer input.
func HasSkillToken(text string) bool {
	for _, candidate := range slashTokens(text) {
		command := strings.ToLower(candidate.command)
		if strings.HasPrefix(command, "skill:") && len(command) > len("skill:") {
			return true
		}
	}
	return false
}

type command struct{ name, description string }

var builtinCommands = []command{
	{"settings", "Open settings menu"},
	{"model", "Select model (opens selector UI)"},
	{"scoped-models", "Enable/disable models for Ctrl+P cycling"},
	{"export", "Export session (HTML default, or specify path: .html/.jsonl)"},
	{"import", "Import and resume a session from a JSONL file"},
	{"share", "Share session as a secret GitHub gist"},
	{"copy", "Copy last agent message to clipboard"},
	{"name", "Set session display name"},
	{"session", "Show session info and stats"},
	{"subagents", "List or switch subagent views"},
	{"main", "Switch back to the main agent view"},
	{"changelog", "Show changelog entries"},
	{"help", "Show help (topics: permissions, sessions, commands)"},
	{"hotkeys", "Show all keyboard shortcuts"},
	{"fork", "Create a new fork from a previous user message"},
	{"clone", "Duplicate the current session at the current position"},
	{"tree", "Navigate session tree (switch branches)"},
	{"graph", "Visual session timeline (git graph style)"},
	{"login", "Configure provider authentication"},
	{"logout", "Remove provider authentication"},
	{"usage", "Show rate limit usage for codex-oauth"},
	{"new", "Start a new session"},
	{"compact", "Manually compact the session context"},
	{"permissions", "Show permission decisions for this session"},
	{"resume", "Resume a different session"},
	{"reload", "Reload keybindings, extensions, skills, prompts, and themes"},
	{"search", "Search tool outputs for a pattern"},
	{"quit", "Quit Rózsa"},
	{"gc", "Clean up old session files"},
	{"lsp", "Configure LSP auto-diagnostics mode"},
}

func slashSuggestions(prefix string, dynamic []skills.Skill) []Suggestion {
	commands := make([]command, 0, len(builtinCommands)+len(dynamic))
	commands = append(commands, builtinCommands...)
	for _, skill := range dynamic {
		name := "skill:" + skill.Name
		commands = append(commands, command{name: name, description: skill.Description})
	}

	matched := make([]Suggestion, 0, len(commands))
	for _, candidate := range commands {
		name := strings.ToLower(candidate.name)
		description := strings.ToLower(candidate.description)
		if prefix != "" && !strings.Contains(name, prefix) && !strings.Contains(description, prefix) {
			continue
		}
		matched = append(matched, Suggestion{
			Value:       "/" + candidate.name + " ",
			Label:       "/" + candidate.name,
			Description: candidate.description,
		})
	}
	sort.SliceStable(matched, func(i, j int) bool {
		left, right := suggestionScore(matched[i], prefix), suggestionScore(matched[j], prefix)
		if left != right {
			return left < right
		}
		return matched[i].Value < matched[j].Value
	})
	return matched
}

func suggestionScore(item Suggestion, prefix string) int {
	if prefix == "" {
		return 0
	}
	name := strings.ToLower(strings.TrimPrefix(item.Label, "/"))
	if strings.HasPrefix(name, prefix) {
		return 0
	}
	if strings.Contains(name, prefix) {
		return 1
	}
	if strings.Contains(strings.ToLower(item.Description), prefix) {
		return 2
	}
	return 3
}

func slashPrefix(head string) (string, bool) {
	lastStart := 0
	for index, character := range head {
		if unicode.IsSpace(character) {
			lastStart = index + len(string(character))
		}
	}
	if lastStart > len(head) {
		return "", false
	}
	token := head[lastStart:]
	if !strings.HasPrefix(token, "/") {
		return "", false
	}
	return strings.ToLower(token[1:]), true
}

func filePrefix(head string) (string, bool) {
	if start := strings.LastIndex(head, `@"`); start >= 0 && tokenStart(head, start) && !strings.Contains(head[start+2:], `"`) {
		return head[start:], true
	}
	start := strings.LastIndex(head, "@")
	if start < 0 || !tokenStart(head, start) {
		return "", false
	}
	return head[start:], true
}

func tokenStart(text string, start int) bool {
	if start == 0 {
		return true
	}
	previous, _ := utf8.DecodeLastRuneInString(text[:start])
	return unicode.IsSpace(previous)
}

func (service completionService) fileSuggestions(prefix, cwd string) ([]Suggestion, error) {
	raw := strings.TrimPrefix(strings.TrimPrefix(prefix, `@"`), "@")
	raw = strings.TrimSuffix(raw, `"`)
	quoted := strings.HasPrefix(prefix, `@"`)
	expanded := expandHome(raw)
	searchDir, namePrefix, displayDir := splitCompletionPath(cwd, raw, expanded)
	entries, err := service.files.ReadDir(searchDir)
	if err != nil {
		return nil, err
	}
	items := make([]Suggestion, 0, len(entries))
	for _, entry := range entries {
		name := entry.Name()
		if !strings.HasPrefix(strings.ToLower(name), strings.ToLower(namePrefix)) {
			continue
		}
		displayPath := displayDir + name
		isDirectory := entry.IsDir()
		label := name
		if isDirectory {
			displayPath += "/"
			label += "/"
		}
		valuePath := "@" + displayPath
		if quoted || strings.Contains(displayPath, " ") {
			valuePath = `@"` + displayPath + `"`
		}
		value := valuePath + " "
		if isDirectory {
			value = valuePath
		}
		items = append(items, Suggestion{Value: value, Label: label, Description: displayPath})
	}
	sort.Slice(items, func(i, j int) bool {
		leftDirectory, rightDirectory := strings.HasSuffix(items[i].Label, "/"), strings.HasSuffix(items[j].Label, "/")
		if leftDirectory != rightDirectory {
			return leftDirectory
		}
		return items[i].Label < items[j].Label
	})
	if len(items) > 50 {
		items = items[:50]
	}
	return items, nil
}

func splitCompletionPath(cwd, raw, expanded string) (searchDir, namePrefix, displayDir string) {
	if strings.HasSuffix(raw, "/") {
		if filepath.IsAbs(expanded) {
			return filepath.FromSlash(expanded), "", raw
		}
		return filepath.Join(cwd, filepath.FromSlash(expanded)), "", raw
	}
	dirRaw, name := filepath.Split(filepath.FromSlash(raw))
	dirExpanded, _ := filepath.Split(filepath.FromSlash(expanded))
	if dirExpanded == "" {
		dirExpanded = "."
	}
	if filepath.IsAbs(dirExpanded) {
		searchDir = dirExpanded
	} else {
		searchDir = filepath.Join(cwd, dirExpanded)
	}
	displayDir = strings.ReplaceAll(dirRaw, "\\", "/")
	if displayDir != "" && !strings.HasSuffix(displayDir, "/") {
		displayDir += "/"
	}
	return searchDir, name, displayDir
}

func (service completionService) highlightRanges(text, cwd string, dynamic []skills.Skill) []HighlightRange {
	result := make([]HighlightRange, 0)
	for _, token := range slashTokens(text) {
		if validSlashCommand(token.command, dynamic) {
			result = append(result, HighlightRange{Start: utf16Length(text[:token.start]), End: utf16Length(text[:token.end])})
		}
	}
	for _, token := range fileTokens(text) {
		path := strings.TrimSuffix(strings.TrimPrefix(token.value, `@"`), `"`)
		path = strings.TrimPrefix(path, "@")
		if path == "" {
			continue
		}
		if _, err := service.files.Stat(resolvePath(path, cwd)); err == nil {
			result = append(result, HighlightRange{Start: utf16Length(text[:token.start]), End: utf16Length(text[:token.end])})
		}
	}
	sort.Slice(result, func(i, j int) bool { return result[i].Start < result[j].Start })
	return result
}

type token struct {
	start, end int
	command    string
	value      string
}

func slashTokens(text string) []token {
	result := make([]token, 0)
	for start, character := range text {
		if character != '/' || (start > 0 && !unicode.IsSpace(lastRune(text[:start]))) {
			continue
		}
		end := start + 1
		for end < len(text) {
			r, size := utf8.DecodeRuneInString(text[end:])
			if unicode.IsSpace(r) {
				break
			}
			end += size
		}
		if end > start+1 {
			result = append(result, token{start: start, end: end, command: text[start+1 : end]})
		}
	}
	return result
}

func fileTokens(text string) []token {
	result := make([]token, 0)
	for start, character := range text {
		if character != '@' || (start > 0 && !unicode.IsSpace(lastRune(text[:start]))) {
			continue
		}
		if strings.HasPrefix(text[start:], `@"`) {
			end := strings.Index(text[start+2:], `"`)
			if end >= 0 {
				end = start + 2 + end + 1
				result = append(result, token{start: start, end: end, value: text[start:end]})
			}
			continue
		}
		end := start + 1
		for end < len(text) {
			r, size := utf8.DecodeRuneInString(text[end:])
			if unicode.IsSpace(r) {
				break
			}
			end += size
		}
		if end > start+1 {
			result = append(result, token{start: start, end: end, value: text[start:end]})
		}
	}
	return result
}

func validSlashCommand(commandText string, dynamic []skills.Skill) bool {
	name := strings.ToLower(commandText)
	for _, builtin := range builtinCommands {
		if name == builtin.name {
			return true
		}
	}
	requestedSkill := strings.TrimPrefix(name, "skill:")
	for _, skill := range dynamic {
		if strings.ToLower(skill.Name) == requestedSkill {
			if strings.HasPrefix(name, "skill:") {
				return true
			}
			for _, builtin := range builtinCommands {
				if skill.Name == builtin.name {
					return false
				}
			}
			return true
		}
	}
	return false
}

func resolvePath(path, cwd string) string {
	expanded := expandHome(path)
	if filepath.IsAbs(filepath.FromSlash(expanded)) {
		return filepath.Clean(filepath.FromSlash(expanded))
	}
	return filepath.Clean(filepath.Join(cwd, filepath.FromSlash(expanded)))
}

func expandHome(path string) string {
	if path != "~" && !strings.HasPrefix(path, "~/") {
		return path
	}
	home, err := os.UserHomeDir()
	if err != nil {
		return path
	}
	if path == "~" {
		return home
	}
	return filepath.Join(home, filepath.FromSlash(strings.TrimPrefix(path, "~/")))
}

func utf16OffsetToByteIndex(text string, offset int) int {
	if offset <= 0 {
		return 0
	}
	position := 0
	for byteIndex, character := range text {
		if offset <= position {
			return byteIndex
		}
		next := position + utf16.RuneLen(character)
		if offset < next {
			return byteIndex
		}
		position = next
	}
	return len(text)
}

func utf16Length(text string) int { return len(utf16.Encode([]rune(text))) }

func lastRune(text string) rune {
	last, _ := utf8.DecodeLastRuneInString(text)
	return last
}
