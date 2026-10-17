package permissions

import (
	"os"
	"os/user"
	"path/filepath"
	"regexp"
	"strings"
)

func canonicalPath(path, root string) string {
	if path == "" {
		path = root
	}
	if strings.HasPrefix(path, "~/") || path == "~" {
		if current, err := user.Current(); err == nil {
			if path == "~" {
				path = current.HomeDir
			} else {
				path = filepath.Join(current.HomeDir, strings.TrimPrefix(path, "~/"))
			}
		}
	}
	if !filepath.IsAbs(path) {
		base := root
		if base == "" {
			base = "."
		}
		path = filepath.Join(base, path)
	}
	abs, err := filepath.Abs(filepath.Clean(path))
	if err != nil {
		return filepath.ToSlash(filepath.Clean(path))
	}
	return filepath.ToSlash(filepath.Clean(resolveExistingPath(abs)))
}

// resolveExistingPath resolves symlinks in the longest existing prefix, then
// appends missing suffix components without guessing whether they exist.
func resolveExistingPath(path string) string {
	current := path
	missing := []string{}
	for {
		resolved, err := filepath.EvalSymlinks(current)
		if err == nil {
			for index := len(missing) - 1; index >= 0; index-- {
				resolved = filepath.Join(resolved, missing[index])
			}
			return filepath.Clean(resolved)
		}
		if _, statErr := os.Lstat(current); statErr == nil {
			return filepath.Clean(path)
		}
		parent := filepath.Dir(current)
		if parent == current {
			return filepath.Clean(path)
		}
		missing = append(missing, filepath.Base(current))
		current = parent
	}
}

func pathPatternMatches(pattern, target, root string) bool {
	if pattern == "*" {
		return true
	}
	candidate := canonicalPath(target, root)
	base := canonicalPath(root, root)
	if strings.HasPrefix(pattern, "$HOME/") {
		current, err := user.Current()
		if err != nil {
			return false
		}
		home := canonicalPath(current.HomeDir, current.HomeDir)
		relative, ok := relativePath(candidate, home)
		if !ok {
			return false
		}
		pattern = strings.TrimPrefix(pattern, "$HOME/")
		candidate = relative
	} else if filepath.IsAbs(filepath.FromSlash(pattern)) {
		pattern = canonicalGlob(pattern, root)
	} else {
		relative, ok := relativePath(candidate, base)
		if !ok {
			return false
		}
		candidate = relative
	}
	if expression, ok := strings.CutPrefix(pattern, "regex:"); ok {
		return regexMatches(expression, candidate)
	}
	return pathGlobMatches(pattern, candidate)
}

func canonicalGlob(pattern, root string) string {
	firstWildcard := strings.IndexAny(pattern, "*?")
	if firstWildcard < 0 {
		return canonicalPath(pattern, root)
	}
	separator := strings.LastIndex(pattern[:firstWildcard], string(filepath.Separator))
	if separator < 0 {
		return filepath.ToSlash(pattern)
	}
	prefix := pattern[:separator+1]
	return strings.TrimSuffix(canonicalPath(prefix, root), "/") + "/" + filepath.ToSlash(pattern[separator+1:])
}

func relativePath(path, base string) (string, bool) {
	path = strings.TrimSuffix(filepath.ToSlash(path), "/")
	base = strings.TrimSuffix(filepath.ToSlash(base), "/")
	if path == base {
		return "", true
	}
	if strings.HasPrefix(path, base+"/") {
		return strings.TrimPrefix(path, base+"/"), true
	}
	return "", false
}

func pathGlobMatches(pattern, candidate string) bool {
	var expression strings.Builder
	for index := 0; index < len(pattern); index++ {
		switch pattern[index] {
		case '*':
			if index+1 < len(pattern) && pattern[index+1] == '*' {
				index++
				if index+1 < len(pattern) && pattern[index+1] == '/' {
					index++
					expression.WriteString("(?:.*/)?")
				} else {
					expression.WriteString(".*")
				}
			} else {
				expression.WriteString("[^/]*")
			}
		case '?':
			expression.WriteString("[^/]")
		default:
			expression.WriteString(regexp.QuoteMeta(string(pattern[index])))
		}
	}
	return regexMatches(expression.String(), candidate)
}

func regexMatches(expression, candidate string) bool {
	compiled, err := regexp.Compile("^(?:" + expression + ")$")
	return err == nil && compiled.MatchString(candidate)
}

func trustScopeMatches(tool, grant, target string) bool {
	if !isPathTool(tool) {
		return commandTrustMatches(grant, target)
	}
	if grant == target {
		return true
	}
	if strings.ContainsAny(grant, "*?") {
		return pathGlobMatches(grant, target)
	}
	grant = strings.TrimSuffix(filepath.ToSlash(grant), "/")
	target = filepath.ToSlash(target)
	return strings.HasPrefix(target, grant+"/")
}
