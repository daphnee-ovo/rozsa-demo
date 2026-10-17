// Package skills discovers, parses, and resolves layered skill definitions.
package skills

import (
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"strings"
	"unicode"
	"unicode/utf8"
)

type Scope uint8

const (
	User Scope = iota
	Agents
	Project
)

type Source struct {
	Directory string
	Scope     Scope
}

type Skill struct {
	Name        string
	Description string
	FilePath    string
	BaseDir     string
	Scope       Scope
}

type Diagnostic struct {
	Path    string
	Message string
}

type LoadResult struct {
	Skills      []Skill
	Diagnostics []Diagnostic
}

// Load reads skills in source order. Invalid skill files become diagnostics;
// unreadable source directories are skipped to match the Rust loader.
func Load(sources []Source, enabled map[string]bool) LoadResult {
	var result LoadResult
	for _, source := range sources {
		entries, err := readDirectory(source.Directory)
		if err != nil {
			continue
		}
		scan(source.Directory, source.Scope, entries, &result)
	}
	return Resolve(result, enabled)
}

func scan(directory string, scope Scope, entries []fs.DirEntry, result *LoadResult) {
	for _, entry := range entries {
		path := filepath.Join(directory, entry.Name())
		info, err := os.Stat(path)
		if err != nil || !info.IsDir() {
			continue
		}
		filePath := filepath.Join(path, "SKILL.md")
		info, err = os.Stat(filePath)
		if err == nil && !info.IsDir() {
			skill, diagnostic := parseFile(filePath, path, scope)
			if diagnostic != nil {
				result.Diagnostics = append(result.Diagnostics, *diagnostic)
			} else {
				result.Skills = append(result.Skills, skill)
			}
			continue
		}
		if err != nil && !os.IsNotExist(err) {
			result.Diagnostics = append(result.Diagnostics, Diagnostic{Path: filePath, Message: "failed to inspect: " + err.Error()})
			continue
		}
		nestedEntries, err := readDirectory(path)
		if err != nil {
			continue
		}
		scan(path, scope, nestedEntries, result)
	}
}

func readDirectory(directory string) ([]fs.DirEntry, error) {
	openedDirectory, err := os.Open(directory)
	if err != nil {
		return nil, err
	}
	defer openedDirectory.Close()
	return openedDirectory.ReadDir(-1)
}

func parseFile(filePath, skillDir string, scope Scope) (Skill, *Diagnostic) {
	content, err := os.ReadFile(filePath)
	if err != nil {
		return Skill{}, &Diagnostic{Path: filePath, Message: "failed to read: " + err.Error()}
	}
	frontmatter, ok := extractFrontmatter(string(content))
	if !ok {
		return Skill{}, &Diagnostic{Path: filePath, Message: "missing YAML frontmatter (no --- delimiters)"}
	}
	description, exists := frontmatterField(frontmatter, "description")
	if !exists {
		return Skill{}, &Diagnostic{Path: filePath, Message: "missing required field: description"}
	}
	if strings.TrimSpace(description) == "" {
		return Skill{}, &Diagnostic{Path: filePath, Message: "description is empty"}
	}
	name, exists := frontmatterField(frontmatter, "name")
	if !exists {
		name = filepath.Base(skillDir)
	}
	return Skill{Name: name, Description: description, FilePath: filePath, BaseDir: skillDir, Scope: scope}, nil
}

func extractFrontmatter(content string) (string, bool) {
	trimmed := strings.TrimLeft(content, " \t\r\n")
	if !strings.HasPrefix(trimmed, "---") {
		return "", false
	}
	afterFirst := strings.TrimPrefix(trimmed[3:], "\n")
	if strings.HasPrefix(afterFirst, "\r\n") {
		afterFirst = afterFirst[2:]
	}
	end := strings.Index(afterFirst, "\n---")
	if end < 0 {
		return "", false
	}
	return afterFirst[:end], true
}

func frontmatterField(frontmatter, key string) (string, bool) {
	lines := strings.Split(frontmatter, "\n")
	prefix := key + ":"
	for index, line := range lines {
		value, found := strings.CutPrefix(strings.TrimSpace(line), prefix)
		if !found {
			continue
		}
		value = strings.TrimSpace(value)
		if value == "|" || value == ">" {
			var parts []string
			for _, continuation := range lines[index+1:] {
				if !strings.HasPrefix(continuation, " ") && !strings.HasPrefix(continuation, "\t") {
					break
				}
				parts = append(parts, strings.TrimSpace(continuation))
			}
			if len(parts) != 0 {
				return strings.Join(parts, " "), true
			}
			continue
		}
		if len(value) >= 2 && ((value[0] == '"' && value[len(value)-1] == '"') || (value[0] == '\'' && value[len(value)-1] == '\'')) {
			return value[1 : len(value)-1], true
		}
		if value != "" {
			return value, true
		}
	}
	return "", false
}

func Resolve(loaded LoadResult, enabled map[string]bool) LoadResult {
	resolved := make([]Skill, 0, len(loaded.Skills))
	indexes := make(map[string]int, len(loaded.Skills))
	for _, skill := range loaded.Skills {
		if isEnabled, exists := enabled[skill.Name]; exists && !isEnabled {
			continue
		}
		if index, exists := indexes[skill.Name]; exists {
			if skill.Scope > resolved[index].Scope {
				resolved[index] = skill
			}
			continue
		}
		indexes[skill.Name] = len(resolved)
		resolved = append(resolved, skill)
	}
	loaded.Skills = resolved
	return loaded
}

func (skill Skill) VariablePath() string {
	variable := map[Scope]string{User: "$USER_SKILLS", Agents: "$AGENTS_SKILLS", Project: "$PROJECT_SKILLS"}[skill.Scope]
	return fmt.Sprintf("%s/%s/%s", variable, filepath.Base(skill.BaseDir), filepath.Base(skill.FilePath))
}

func (result LoadResult) SlashCommandNames() []string {
	names := make([]string, 0, len(result.Skills))
	for _, skill := range result.Skills {
		names = append(names, skill.Name)
	}
	return names
}

func (result LoadResult) PromptFragment() string {
	if len(result.Skills) == 0 {
		return ""
	}
	lines := []string{"## Skills", "Available skills:"}
	for _, skill := range result.Skills {
		lines = append(lines, fmt.Sprintf("- %s: %s (file: %s)", skill.Name, skill.Description, skill.VariablePath()))
	}
	return strings.Join(lines, "\n")
}

// ExpandCommands replaces known /skill:name tokens with their markdown body.
// Unknown skills and unreadable skill files remain unchanged. The display value
// is the original input and is present only when at least one token expands.
func ExpandCommands(text string, skills []Skill) (expanded, display string, changed bool) {
	byName := make(map[string]Skill, len(skills))
	for _, skill := range skills {
		byName[skill.Name] = skill
	}
	var output strings.Builder
	cursor := 0
	searchFrom := 0
	for searchFrom < len(text) {
		relativeStart := strings.IndexByte(text[searchFrom:], '/')
		if relativeStart < 0 {
			break
		}
		start := searchFrom + relativeStart
		searchFrom = start + 1
		if start > 0 && !isSpaceBefore(text, start) {
			continue
		}
		nameStart := start + len("/skill:")
		if nameStart > len(text) || !strings.HasPrefix(text[start:], "/skill:") {
			continue
		}
		end := nameStart
		for end < len(text) {
			r, size := runeAt(text, end)
			if isSpace(r) {
				break
			}
			end += size
		}
		if end == nameStart {
			continue
		}
		skill, exists := byName[text[nameStart:end]]
		if !exists {
			start = end
			continue
		}
		content, err := os.ReadFile(skill.FilePath)
		if err != nil {
			start = end
			continue
		}
		output.WriteString(text[cursor:start])
		output.WriteString("<skill>\n<name>")
		output.WriteString(text[nameStart:end])
		output.WriteString("</name>\n<content>\n")
		output.WriteString(strings.TrimSpace(StripFrontmatter(string(content))))
		output.WriteString("\n</content>\n<base_dir>")
		output.WriteString(skill.BaseDir)
		output.WriteString("</base_dir>\n</skill>")
		cursor = end
		changed = true
		start = end
	}
	if !changed {
		return text, "", false
	}
	output.WriteString(text[cursor:])
	return output.String(), text, true
}

func isSpaceBefore(text string, index int) bool {
	character, _ := utf8.DecodeLastRuneInString(text[:index])
	return unicode.IsSpace(character)
}

func runeAt(text string, index int) (rune, int) {
	return utf8.DecodeRuneInString(text[index:])
}

func isSpace(character rune) bool {
	return unicode.IsSpace(character)
}

func StripFrontmatter(content string) string {
	trimmed := strings.TrimLeft(content, " \t\r\n")
	if !strings.HasPrefix(trimmed, "---") {
		return content
	}
	afterFirst := strings.TrimPrefix(trimmed[3:], "\n")
	if strings.HasPrefix(afterFirst, "\r\n") {
		afterFirst = afterFirst[2:]
	}
	end := strings.Index(afterFirst, "\n---")
	if end < 0 {
		return content
	}
	body := afterFirst[end+4:]
	return strings.TrimPrefix(body, "\n")
}
