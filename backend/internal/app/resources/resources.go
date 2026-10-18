// Package resources resolves layered context files and formats prompt sections.
package resources

import (
	_ "embed"
	"fmt"
	"os"
	"path/filepath"
	"strings"
)

type Source uint8

const (
	ClaudeMD Source = iota
	AgentsMD
	DevDoc
	Custom
)

type Resource struct {
	Path    string
	Content string
	Source  Source
	Label   string
}

type Diagnostic struct {
	Path    string
	Message string
}

type Result struct {
	Resources   []Resource
	Diagnostics []Diagnostic
}

// DefaultSystemPrompt is embedded in the Go binary, matching the Rust app's
// compile-time system prompt asset.
//go:embed system-prompt.md
var defaultSystemPrompt string

func DefaultSystemPrompt() string { return defaultSystemPrompt }

// Load applies config-directory override semantics, then appends one context
// file per ancestor directory from the filesystem root to cwd.
func Load(cwd string, configDirs []string) Result {
	var result Result
	seen := make(map[string]struct{})
	var configured *Resource
	for _, directory := range configDirs {
		resource, diagnostics := loadContextFile(directory)
		result.Diagnostics = append(result.Diagnostics, diagnostics...)
		if resource != nil {
			configured = resource
		}
	}
	if configured != nil {
		result.Resources = append(result.Resources, *configured)
		seen[configured.Path] = struct{}{}
	}

	var ancestors []Resource
	currentDirectory, err := filepath.Abs(cwd)
	if err != nil {
		currentDirectory = cwd
	}
	for {
		resource, diagnostics := loadContextFile(currentDirectory)
		result.Diagnostics = append(result.Diagnostics, diagnostics...)
		if resource != nil {
			if _, exists := seen[resource.Path]; !exists {
				seen[resource.Path] = struct{}{}
				ancestors = append(ancestors, *resource)
			}
		}
		parent := filepath.Dir(currentDirectory)
		if parent == currentDirectory || parent == "." {
			break
		}
		currentDirectory = parent
	}
	for left, right := 0, len(ancestors)-1; left < right; left, right = left+1, right-1 {
		ancestors[left], ancestors[right] = ancestors[right], ancestors[left]
	}
	result.Resources = append(result.Resources, ancestors...)
	return result
}

func loadContextFile(directory string) (*Resource, []Diagnostic) {
	var diagnostics []Diagnostic
	for _, filename := range []string{"AGENTS.md", "AGENTS.MD", "CLAUDE.md", "CLAUDE.MD"} {
		filePath := filepath.Join(directory, filename)
		if _, err := os.Stat(filePath); err != nil {
			continue
		}
		content, err := os.ReadFile(filePath)
		if err != nil {
			diagnostics = append(diagnostics, Diagnostic{Path: filePath, Message: "failed to read context file: " + err.Error()})
			continue
		}
		source := ClaudeMD
		if strings.HasPrefix(filename, "AGENTS") {
			source = AgentsMD
		}
		return &Resource{Path: filePath, Content: string(content), Source: source, Label: filename}, diagnostics
	}
	return nil, diagnostics
}

type SystemPrompt interface {
	Format(resources []Resource) string
}

type promptFormatter struct{ base string }

func NewSystemPrompt(base string) SystemPrompt { return promptFormatter{base: base} }

func (formatter promptFormatter) Format(resources []Resource) string {
	parts := make([]string, 0, len(resources)+1)
	parts = append(parts, formatter.base)
	for _, resource := range resources {
		label := resource.Label
		if label == "" {
			label = sourceLabel(resource.Source)
		}
		parts = append(parts, fmt.Sprintf("# %s\n\nFrom: %s\n\n%s", label, resource.Path, resource.Content))
	}
	return strings.Join(parts, "\n\n---\n\n")
}

func sourceLabel(source Source) string {
	switch source {
	case ClaudeMD:
		return "CLAUDE.md"
	case AgentsMD:
		return "AGENTS.md"
	case DevDoc:
		return ".dev-doc"
	default:
		return "custom"
	}
}
