// Package diff creates bounded file evidence and workspace Git diffs.
package diff

import (
	"bytes"
	"fmt"
	"os/exec"
	"path/filepath"
	"sort"
	"strings"
)

const maxPatchBytes = 120000

type FileDelta struct {
	Path      string  `json:"path"`
	Status    string  `json:"status"`
	Before    *string `json:"before"`
	After     *string `json:"after"`
	Patch     string  `json:"patch"`
	Added     uint64  `json:"added"`
	Deleted   uint64  `json:"deleted"`
	Truncated bool    `json:"truncated"`
}
type FileDiff struct {
	Path      string `json:"path"`
	Patch     string `json:"patch"`
	Truncated bool   `json:"truncated"`
}
type Activity struct {
	ChangedFiles      []string    `json:"changedFiles"`
	FileChanges       []FileDelta `json:"fileChanges"`
	CaptureComplete   bool        `json:"captureComplete"`
	CaptureLimitation string      `json:"captureLimitation,omitempty"`
}
type accumulator struct {
	files      map[string][2]*string
	opaque     map[string]bool
	complete   bool
	limitation string
}
type gitResult struct {
	Stdout, Stderr []byte
	code           int
}

func (r gitResult) ExitCode() int { return r.code }

func BuildFileDelta(path string, before, after *string) *FileDelta {
	if equal(before, after) {
		return nil
	}
	status := "modified"
	if before == nil {
		status = "added"
	} else if after == nil {
		status = "deleted"
	}
	patch, added, deleted := render(path, before, after)
	truncated := len(patch) > 256*1024
	if truncated {
		patch = patch[:safeBoundary(patch, 256*1024)]
	}
	return &FileDelta{path, status, truncate(before, 256*1024), truncate(after, 256*1024), patch, added, deleted, truncated}
}

// ReadWorkspaceDiff mirrors the Rust helper: tracked files use git diff; visible
// untracked files use diff --no-index against the platform null device.
func ReadWorkspaceDiff(cwd, path string) (FileDiff, error) {
	if filepath.IsAbs(path) {
		return FileDiff{}, fmt.Errorf("Diff path must stay within the workspace")
	}
	for _, part := range strings.Split(filepath.ToSlash(path), "/") {
		if part == ".." {
			return FileDiff{}, fmt.Errorf("Diff path must stay within the workspace")
		}
	}
	inside, err := git(cwd, "rev-parse", "--is-inside-work-tree")
	if err != nil {
		return FileDiff{}, err
	}
	if inside.ExitCode() != 0 {
		return FileDiff{}, fmt.Errorf("Workspace is not a Git repository")
	}
	tracked, _ := git(cwd, "ls-files", "--error-unmatch", "--", path)
	var data []byte
	if tracked.ExitCode() == 0 {
		out, e := checked(cwd, "diff", "--no-textconv", "--no-ext-diff", "--", path)
		if e != nil {
			return FileDiff{}, e
		}
		data = out
	} else {
		untracked, e := git(cwd, "ls-files", "--others", "--exclude-standard", "--", path)
		if e != nil {
			return FileDiff{}, e
		}
		if untracked.ExitCode() == 0 && len(untracked.Stdout) > 0 {
			null := "/dev/null"
			out, e := checked(cwd, "diff", "--no-textconv", "--no-ext-diff", "--no-index", "--", null, path)
			if e != nil {
				return FileDiff{}, e
			}
			data = out
		}
	}
	truncated := len(data) > maxPatchBytes
	if truncated {
		data = data[:safeBoundary(string(data), maxPatchBytes)]
	}
	return FileDiff{path, string(data), truncated}, nil
}

// MergeResults combines ordered tool results into a turn-level summary.
// Input must be a decoded JSON object; malformed file delta entries are errors.
func MergeResults(results []map[string]any) (Activity, error) {
	a := accumulator{files: map[string][2]*string{}, opaque: map[string]bool{}, complete: true}
	for _, result := range results {
		details, _ := result["details"].(map[string]any)
		if details == nil {
			continue
		}
		if complete, ok := details["capture_complete"].(bool); ok && !complete {
			a.complete = false
			if lim, ok := details["capture_limitation"].(string); ok && a.limitation == "" {
				a.limitation = lim
			}
		}
		if raw, ok := details["file_deltas"].([]any); ok {
			for _, item := range raw {
				m, ok := item.(map[string]any)
				if !ok {
					return Activity{}, fmt.Errorf("invalid file delta entry")
				}
				path, ok := m["path"].(string)
				if !ok {
					return Activity{}, fmt.Errorf("file delta missing path")
				}
				after := stringPtr(m["after"])
				before := stringPtr(m["before"])
				if prior, ok := a.files[path]; ok {
					before = prior[0]
				}
				a.files[path] = [2]*string{before, after}
				delete(a.opaque, path)
			}
		}
		if raw, ok := details["changed_files"].([]any); ok {
			for _, item := range raw {
				if path, ok := item.(string); ok {
					if _, has := a.files[path]; !has {
						a.opaque[path] = true
					}
				}
			}
		}
	}
	paths := make([]string, 0, len(a.files))
	for path := range a.files {
		paths = append(paths, path)
	}
	sort.Strings(paths)
	out := Activity{CaptureComplete: a.complete, CaptureLimitation: a.limitation}
	for _, path := range paths {
		pair := a.files[path]
		delta := BuildFileDelta(path, pair[0], pair[1])
		if delta != nil {
			out.FileChanges = append(out.FileChanges, *delta)
			out.ChangedFiles = append(out.ChangedFiles, path)
		}
	}
	for path := range a.opaque {
		out.ChangedFiles = append(out.ChangedFiles, path)
	}
	sort.Strings(out.ChangedFiles)
	return out, nil
}

func render(path string, before, after *string) (string, uint64, uint64) {
	left, right := "/dev/null", "/dev/null"
	if before != nil {
		left = "a/" + path
	}
	if after != nil {
		right = "b/" + path
	}
	var b strings.Builder
	fmt.Fprintf(&b, "--- %s\n+++ %s\n", left, right)
	var old, new []string
	if before != nil {
		old = strings.Split(strings.TrimSuffix(*before, "\n"), "\n")
	}
	if after != nil {
		new = strings.Split(strings.TrimSuffix(*after, "\n"), "\n")
	}
	if before != nil && *before == "" {
		old = nil
	}
	if after != nil && *after == "" {
		new = nil
	}
	prefix := 0
	for prefix < len(old) && prefix < len(new) && old[prefix] == new[prefix] {
		prefix++
	}
	suffix := 0
	for suffix < len(old)-prefix && suffix < len(new)-prefix && old[len(old)-1-suffix] == new[len(new)-1-suffix] {
		suffix++
	}
	deleted := old[prefix : len(old)-suffix]
	added := new[prefix : len(new)-suffix]
	fmt.Fprintf(&b, "@@ -%d,%d +%d,%d @@\n", prefix+1, len(deleted), prefix+1, len(added))
	for _, line := range deleted {
		b.WriteByte('-')
		b.WriteString(line)
		b.WriteByte('\n')
	}
	for _, line := range added {
		b.WriteByte('+')
		b.WriteString(line)
		b.WriteByte('\n')
	}
	return b.String(), uint64(len(added)), uint64(len(deleted))
}
func checked(cwd string, args ...string) ([]byte, error) {
	out, err := git(cwd, args...)
	if err != nil {
		return nil, err
	}
	if out.ExitCode() != 0 && out.ExitCode() != 1 {
		message := strings.TrimSpace(string(out.Stderr))
		if message == "" {
			message = fmt.Sprintf("git %v failed", args)
		}
		return nil, fmt.Errorf("%s", message)
	}
	return out.Stdout, nil
}
func git(cwd string, args ...string) (gitResult, error) {
	cmdArgs := append([]string{"-c", "core.hooksPath=/dev/null", "-C", cwd}, args...)
	cmd := exec.Command("git", cmdArgs...)
	var stdout, stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr
	err := cmd.Run()
	if err != nil {
		if exit, ok := err.(*exec.ExitError); ok {
			return gitResult{stdout.Bytes(), stderr.Bytes(), exit.ExitCode()}, nil
		}
		return gitResult{}, fmt.Errorf("Failed to run git: %w", err)
	}
	return gitResult{stdout.Bytes(), stderr.Bytes(), 0}, nil
}
func equal(a, b *string) bool { return a == nil && b == nil || a != nil && b != nil && *a == *b }
func truncate(s *string, max int) *string {
	if s == nil {
		return nil
	}
	v := *s
	if len(v) > max {
		v = v[:safeBoundary(v, max)]
	}
	return &v
}
func safeBoundary(s string, max int) int {
	n := min(len(s), max)
	for n > 0 && !bytes.Equal([]byte(s[:n]), []byte(string([]rune(s[:n])))) {
		n--
	}
	return n
}
func stringPtr(v any) *string {
	if v == nil {
		return nil
	}
	if s, ok := v.(string); ok {
		return &s
	}
	return nil
}
