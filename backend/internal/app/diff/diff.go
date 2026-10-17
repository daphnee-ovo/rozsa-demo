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
	exitCode       int
}

func (result gitResult) ExitCode() int { return result.exitCode }

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
func ReadWorkspaceDiff(workingDirectory, path string) (FileDiff, error) {
	if filepath.IsAbs(path) {
		return FileDiff{}, fmt.Errorf("Diff path must stay within the workspace")
	}
	for _, part := range strings.Split(filepath.ToSlash(path), "/") {
		if part == ".." {
			return FileDiff{}, fmt.Errorf("Diff path must stay within the workspace")
		}
	}
	repositoryCheck, err := git(workingDirectory, "rev-parse", "--is-inside-work-tree")
	if err != nil {
		return FileDiff{}, err
	}
	if repositoryCheck.ExitCode() != 0 {
		return FileDiff{}, fmt.Errorf("Workspace is not a Git repository")
	}
	trackedFileCheck, _ := git(workingDirectory, "ls-files", "--error-unmatch", "--", path)
	var data []byte
	if trackedFileCheck.ExitCode() == 0 {
		diffBytes, diffErr := checked(workingDirectory, "diff", "--no-textconv", "--no-ext-diff", "--", path)
		if diffErr != nil {
			return FileDiff{}, diffErr
		}
		data = diffBytes
	} else {
		untrackedFiles, gitErr := git(workingDirectory, "ls-files", "--others", "--exclude-standard", "--", path)
		if gitErr != nil {
			return FileDiff{}, gitErr
		}
		if untrackedFiles.ExitCode() == 0 && len(untrackedFiles.Stdout) > 0 {
			nullDevice := "/dev/null"
			diffBytes, diffErr := checked(workingDirectory, "diff", "--no-textconv", "--no-ext-diff", "--no-index", "--", nullDevice, path)
			if diffErr != nil {
				return FileDiff{}, diffErr
			}
			data = diffBytes
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
	turnEvidence := accumulator{files: map[string][2]*string{}, opaque: map[string]bool{}, complete: true}
	for _, toolResult := range results {
		details, _ := toolResult["details"].(map[string]any)
		if details == nil {
			continue
		}
		if complete, ok := details["capture_complete"].(bool); ok && !complete {
			turnEvidence.complete = false
			if limitation, ok := details["capture_limitation"].(string); ok && turnEvidence.limitation == "" {
				turnEvidence.limitation = limitation
			}
		}
		if rawDeltas, ok := details["file_deltas"].([]any); ok {
			for _, deltaEntry := range rawDeltas {
				deltaData, ok := deltaEntry.(map[string]any)
				if !ok {
					return Activity{}, fmt.Errorf("invalid file delta entry")
				}
				filePath, ok := deltaData["path"].(string)
				if !ok {
					return Activity{}, fmt.Errorf("file delta missing path")
				}
				fileAfter := stringPtr(deltaData["after"])
				fileBefore := stringPtr(deltaData["before"])
				if priorDelta, ok := turnEvidence.files[filePath]; ok {
					fileBefore = priorDelta[0]
				}
				turnEvidence.files[filePath] = [2]*string{fileBefore, fileAfter}
				delete(turnEvidence.opaque, filePath)
			}
		}
		if changedFileEntries, ok := details["changed_files"].([]any); ok {
			for _, changedFileEntry := range changedFileEntries {
				if filePath, ok := changedFileEntry.(string); ok {
					if _, hasDelta := turnEvidence.files[filePath]; !hasDelta {
						turnEvidence.opaque[filePath] = true
					}
				}
			}
		}
	}
	filePaths := make([]string, 0, len(turnEvidence.files))
	for filePath := range turnEvidence.files {
		filePaths = append(filePaths, filePath)
	}
	sort.Strings(filePaths)
	activity := Activity{CaptureComplete: turnEvidence.complete, CaptureLimitation: turnEvidence.limitation}
	for _, filePath := range filePaths {
		fileState := turnEvidence.files[filePath]
		delta := BuildFileDelta(filePath, fileState[0], fileState[1])
		if delta != nil {
			activity.FileChanges = append(activity.FileChanges, *delta)
			activity.ChangedFiles = append(activity.ChangedFiles, filePath)
		}
	}
	for filePath := range turnEvidence.opaque {
		activity.ChangedFiles = append(activity.ChangedFiles, filePath)
	}
	sort.Strings(activity.ChangedFiles)
	return activity, nil
}

func render(path string, before, after *string) (string, uint64, uint64) {
	oldFilePath, newFilePath := "/dev/null", "/dev/null"
	if before != nil {
		oldFilePath = "a/" + path
	}
	if after != nil {
		newFilePath = "b/" + path
	}
	var patch strings.Builder
	fmt.Fprintf(&patch, "--- %s\n+++ %s\n", oldFilePath, newFilePath)
	var oldLines, newLines []string
	if before != nil {
		oldLines = strings.Split(strings.TrimSuffix(*before, "\n"), "\n")
	}
	if after != nil {
		newLines = strings.Split(strings.TrimSuffix(*after, "\n"), "\n")
	}
	if before != nil && *before == "" {
		oldLines = nil
	}
	if after != nil && *after == "" {
		newLines = nil
	}
	sharedPrefixLines := 0
	for sharedPrefixLines < len(oldLines) && sharedPrefixLines < len(newLines) && oldLines[sharedPrefixLines] == newLines[sharedPrefixLines] {
		sharedPrefixLines++
	}
	sharedSuffixLines := 0
	for sharedSuffixLines < len(oldLines)-sharedPrefixLines && sharedSuffixLines < len(newLines)-sharedPrefixLines && oldLines[len(oldLines)-1-sharedSuffixLines] == newLines[len(newLines)-1-sharedSuffixLines] {
		sharedSuffixLines++
	}
	deletedLines := oldLines[sharedPrefixLines : len(oldLines)-sharedSuffixLines]
	addedLines := newLines[sharedPrefixLines : len(newLines)-sharedSuffixLines]
	fmt.Fprintf(&patch, "@@ -%d,%d +%d,%d @@\n", sharedPrefixLines+1, len(deletedLines), sharedPrefixLines+1, len(addedLines))
	for _, line := range deletedLines {
		patch.WriteByte('-')
		patch.WriteString(line)
		patch.WriteByte('\n')
	}
	for _, line := range addedLines {
		patch.WriteByte('+')
		patch.WriteString(line)
		patch.WriteByte('\n')
	}
	return patch.String(), uint64(len(addedLines)), uint64(len(deletedLines))
}
func checked(workingDirectory string, gitArguments ...string) ([]byte, error) {
	gitOutput, err := git(workingDirectory, gitArguments...)
	if err != nil {
		return nil, err
	}
	if gitOutput.ExitCode() != 0 && gitOutput.ExitCode() != 1 {
		message := strings.TrimSpace(string(gitOutput.Stderr))
		if message == "" {
			message = fmt.Sprintf("git %v failed", gitArguments)
		}
		return nil, fmt.Errorf("%s", message)
	}
	return gitOutput.Stdout, nil
}
func git(workingDirectory string, gitArguments ...string) (gitResult, error) {
	commandArguments := append([]string{"-c", "core.hooksPath=/dev/null", "-C", workingDirectory}, gitArguments...)
	gitCommand := exec.Command("git", commandArguments...)
	var stdout, stderr bytes.Buffer
	gitCommand.Stdout = &stdout
	gitCommand.Stderr = &stderr
	err := gitCommand.Run()
	if err != nil {
		if processExitError, ok := err.(*exec.ExitError); ok {
			return gitResult{stdout.Bytes(), stderr.Bytes(), processExitError.ExitCode()}, nil
		}
		return gitResult{}, fmt.Errorf("Failed to run git: %w", err)
	}
	return gitResult{stdout.Bytes(), stderr.Bytes(), 0}, nil
}
func equal(first, second *string) bool {
	return first == nil && second == nil || first != nil && second != nil && *first == *second
}
func truncate(text *string, maxBytes int) *string {
	if text == nil {
		return nil
	}
	truncatedText := *text
	if len(truncatedText) > maxBytes {
		truncatedText = truncatedText[:safeBoundary(truncatedText, maxBytes)]
	}
	return &truncatedText
}
func safeBoundary(text string, maxBytes int) int {
	boundary := min(len(text), maxBytes)
	for boundary > 0 && !bytes.Equal([]byte(text[:boundary]), []byte(string([]rune(text[:boundary])))) {
		boundary--
	}
	return boundary
}
func stringPtr(value any) *string {
	if value == nil {
		return nil
	}
	if text, ok := value.(string); ok {
		return &text
	}
	return nil
}
