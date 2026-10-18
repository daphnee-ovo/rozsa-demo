package subagent

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
)

// Nil lists mean unrestricted; empty, non-nil lists deny every value.
type Scope struct {
	Tools        []string `json:"tools,omitempty"`
	Paths        []string `json:"paths,omitempty"`
	BashPrefixes []string `json:"bash_prefixes,omitempty"`
	Skills       []string `json:"skills,omitempty"`
}

func Inherit() Scope { return Scope{} }
func cloneScope(scope Scope) Scope {
	clone := func(values []string) []string {
		if values == nil {
			return nil
		}
		return append([]string{}, values...)
	}
	return Scope{Tools: clone(scope.Tools), Paths: clone(scope.Paths), BashPrefixes: clone(scope.BashPrefixes), Skills: clone(scope.Skills)}
}
func Readonly() Scope {
	return Scope{Tools: []string{"read", "bash"}, BashPrefixes: []string{
		"head", "tail", "cat", "grep", "sort", "pwd", "ls", "basename", "dirname", "realpath",
		"readlink", "stat", "file", "wc", "diff", "cmp", "comm", "cut", "tr", "uniq", "strings", "od", "xxd",
	}}
}
func Scoped(paths []string) Scope {
	return Scope{Tools: []string{"read", "write", "edit"}, Paths: append([]string{}, paths...)}
}

func contains(values []string, value string) bool {
	for _, candidate := range values {
		if candidate == value {
			return true
		}
	}
	return false
}

func (scope Scope) Allows(name string) bool { return scope.Tools == nil || contains(scope.Tools, name) }

func (scope Scope) Check(name string, arguments json.RawMessage, cwd string) error {
	if !scope.Allows(name) {
		return fmt.Errorf("tool '%s' is not in the allowed set", name)
	}
	var args map[string]any
	if err := json.Unmarshal(arguments, &args); err != nil {
		return fmt.Errorf("invalid tool arguments: %w", err)
	}
	if contains([]string{"read", "write", "edit"}, name) && scope.Paths != nil {
		for _, key := range []string{"path", "file_path", "filePath", "dir", "directory"} {
			path, ok := args[key].(string)
			if !ok {
				continue
			}
			resolved, err := resolvePath(cwd, path)
			if err != nil {
				return err
			}
			allowed := false
			for _, root := range scope.Paths {
				base, err := resolvePath(cwd, root)
				if err != nil {
					return err
				}
				relative, err := filepath.Rel(base, resolved)
				if err == nil && relative != ".." && !strings.HasPrefix(relative, ".."+string(filepath.Separator)) {
					allowed = true
					break
				}
			}
			if !allowed {
				return fmt.Errorf("path '%s' is outside the allowed scope", resolved)
			}
		}
	}
	if name == "bash" && scope.BashPrefixes != nil {
		command, _ := args["command"].(string)
		allowed := false
		for _, prefix := range scope.BashPrefixes {
			if strings.HasPrefix(command, prefix) {
				allowed = true
				break
			}
		}
		if !allowed {
			return fmt.Errorf("bash command '%s' does not match any allowed prefix", command)
		}
	}
	if name == "skill" && scope.Skills != nil {
		skill, _ := args["skill"].(string)
		if !contains(scope.Skills, skill) {
			return fmt.Errorf("skill '%s' is not in the allowed set", skill)
		}
	}
	return nil
}

// Resolve existing ancestors as well as non-existing write targets. Unlike a
// lexical prefix, this rejects traversal and symlinks leading outside scope.
func resolvePath(cwd, path string) (string, error) {
	if !filepath.IsAbs(path) {
		path = filepath.Join(cwd, path)
	}
	absolute, err := filepath.Abs(path)
	if err != nil {
		return "", err
	}
	current := absolute
	var missing []string
	for {
		resolved, err := filepath.EvalSymlinks(current)
		if err == nil {
			for index := len(missing) - 1; index >= 0; index-- {
				resolved = filepath.Join(resolved, missing[index])
			}
			return resolved, nil
		}
		if !os.IsNotExist(err) {
			return "", fmt.Errorf("resolve scope path: %w", err)
		}
		if _, statErr := os.Lstat(current); statErr == nil {
			return "", fmt.Errorf("resolve scope path: %w", err)
		}
		parent := filepath.Dir(current)
		if parent == current {
			return "", err
		}
		missing = append(missing, filepath.Base(current))
		current = parent
	}
}
