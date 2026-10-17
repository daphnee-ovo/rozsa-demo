// Package file implements the read, write, and edit tools against real files.
package file

import (
	"context"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"unicode/utf8"

	"rozsa/backend/internal/app/diff"
)

const (
	maxLines = 2000
	maxBytes = 50 * 1024
)

type Result struct {
	Text    string `json:"text"`
	Details any    `json:"details"`
}

type Truncation struct {
	Truncated        bool    `json:"truncated"`
	TruncatedBy      *string `json:"truncated_by"`
	TotalLines       int     `json:"total_lines"`
	OutputLines      int     `json:"output_lines"`
	FirstLineExceeds bool    `json:"first_line_exceeds_limit"`
	MaxLines         int     `json:"max_lines"`
	MaxBytes         int     `json:"max_bytes"`
}

var locks sync.Map // map[original path]chan struct{}

func withLock(requestContext context.Context, path string, operation func() error) error {
	if err := requestContext.Err(); err != nil {
		return err
	}
	lockValue, _ := locks.LoadOrStore(path, func() chan struct{} {
		lockChannel := make(chan struct{}, 1)
		lockChannel <- struct{}{}
		return lockChannel
	}())
	lock := lockValue.(chan struct{})
	select {
	case <-requestContext.Done():
		return requestContext.Err()
	case <-lock:
	}
	defer func() { lock <- struct{}{} }()
	if err := requestContext.Err(); err != nil {
		return err
	}
	return operation()
}

func Read(path string, offset, limit *int) (Result, error) {
	fileInfo, err := os.Stat(path)
	if errors.Is(err, os.ErrNotExist) {
		return Result{}, fmt.Errorf("File not found: %s", path)
	}
	if err != nil {
		return Result{}, readError(path, err)
	}
	if !fileInfo.Mode().IsRegular() {
		return Result{}, fmt.Errorf("Not a file: %s", path)
	}
	data, err := os.ReadFile(path)
	if err != nil {
		return Result{}, readError(path, err)
	}
	content := string(data)
	if !utf8Valid(data) {
		return Result{}, fmt.Errorf("Binary file or invalid UTF-8: %s", path)
	}
	lineContent := strings.ReplaceAll(content, "\r\n", "\n")
	lines := strings.Split(lineContent, "\n")
	if lineContent == "" {
		lines = nil
	} else if strings.HasSuffix(lineContent, "\n") {
		lines = lines[:len(lines)-1]
	}
	startLine := 0
	if offset != nil && *offset > 1 {
		startLine = *offset - 1
	}
	if startLine >= len(lines) && len(lines) > 0 {
		return Result{}, fmt.Errorf("Offset %d is beyond end of file (%d lines total)", intOrDefault(offset, 1), len(lines))
	}
	endLine := len(lines)
	if limit != nil && *limit >= 0 && startLine+*limit < endLine {
		endLine = startLine + *limit
	}
	selected := lines[startLine:endLine]
	outputLines := make([]string, 0, len(selected))
	usedBytes := 0
	var truncationReason *string
	isFirstLineTooLarge := len(selected) > 0 && len(selected[0]) > maxBytes
	if isFirstLineTooLarge {
		return Result{}, fmt.Errorf("[Line %d is %s, exceeds %s limit. Use bash: sed -n '%dp' %s | head -c %d]", startLine+1, formatSize(len(selected[0])), formatSize(maxBytes), startLine+1, path, maxBytes)
	}
	for i, line := range selected {
		if i >= maxLines {
			truncationKind := "lines"
			truncationReason = &truncationKind
			break
		}
		lineBytes := len(line)
		if i > 0 {
			lineBytes++
		}
		if usedBytes+lineBytes > maxBytes {
			truncationKind := "bytes"
			truncationReason = &truncationKind
			break
		}
		outputLines = append(outputLines, fmt.Sprintf("%6d\t%s", startLine+i+1, line))
		usedBytes += lineBytes
	}
	truncated := len(outputLines) < len(selected)
	text := strings.Join(outputLines, "\n")
	if truncated {
		if truncationReason != nil && *truncationReason == "lines" {
			text += fmt.Sprintf("\n\n[Showing lines %d-%d of %d. Use offset=%d to continue.]", startLine+1, startLine+len(outputLines), len(lines), startLine+len(outputLines)+1)
		} else {
			text += fmt.Sprintf("\n\n[Showing lines %d-%d of %d (%s limit). Use offset=%d to continue.]", startLine+1, startLine+len(outputLines), len(lines), formatSize(maxBytes), startLine+len(outputLines)+1)
		}
	} else if limit != nil && startLine+len(outputLines) < len(lines) {
		text += fmt.Sprintf("\n\n[%d more lines in file. Use offset=%d to continue.]", len(lines)-startLine-len(outputLines), startLine+len(outputLines)+1)
	}
	return Result{Text: text, Details: Truncation{truncated, truncationReason, len(lines), len(outputLines), isFirstLineTooLarge, maxLines, maxBytes}}, nil
}

func Write(requestContext context.Context, path, content string) (Result, error) {
	var result Result
	before, captureErr := readBefore(path)
	if captureErr != nil {
		return Result{}, fmt.Errorf("Failed to capture file evidence before write: %w", captureErr)
	}
	operationErr := withLock(requestContext, path, func() error {
		parentDirectory := filepath.Dir(path)
		if parentDirectory != "." {
			if createDirectoryErr := os.MkdirAll(parentDirectory, 0755); createDirectoryErr != nil {
				if errors.Is(createDirectoryErr, os.ErrPermission) {
					return fmt.Errorf("Permission denied: cannot create parent directory for %s", path)
				}
				return fmt.Errorf("Failed to create parent directory: %w", createDirectoryErr)
			}
		}
		temporaryFilePath := temporaryPath(path)
		if writeErr := os.WriteFile(temporaryFilePath, []byte(content), 0666); writeErr != nil {
			if errors.Is(writeErr, os.ErrPermission) {
				return fmt.Errorf("Permission denied: %s", path)
			}
			return fmt.Errorf("Failed to write file: %w", writeErr)
		}
		if renameErr := os.Rename(temporaryFilePath, path); renameErr != nil {
			_ = os.Remove(temporaryFilePath)
			if errors.Is(renameErr, os.ErrPermission) {
				return fmt.Errorf("Permission denied: cannot rename temporary file to %s", path)
			}
			return fmt.Errorf("Failed to rename temporary file: %w", renameErr)
		}
		lineCount := lineCount(content)
		result.Text = fmt.Sprintf("Successfully wrote %s to %s (%d lines)", formatSize(len(content)), path, lineCount)
		after := content
		result.Details = map[string]any{"file_path": path, "changed_files": []string{path}, "success": true, "bytes_written": len(content), "line_count": lineCount, "file_deltas": deltas(path, before, &after), "capture_complete": true}
		return nil
	})
	return result, operationErr
}

func Edit(requestContext context.Context, path, oldText, newText string, replaceAll bool) (Result, error) {
	var result Result
	before, captureErr := readBefore(path)
	if captureErr != nil {
		return Result{}, fmt.Errorf("Failed to capture file evidence before edit: %w", captureErr)
	}
	operationErr := withLock(requestContext, path, func() error {
		fileInfo, statErr := os.Stat(path)
		if errors.Is(statErr, os.ErrNotExist) {
			return fmt.Errorf("File not found: %s", path)
		}
		if statErr != nil {
			return readError(path, statErr)
		}
		if !fileInfo.Mode().IsRegular() {
			return fmt.Errorf("Not a file: %s", path)
		}
		fileContents, readErr := os.ReadFile(path)
		if readErr != nil {
			return readError(path, readErr)
		}
		if !utf8Valid(fileContents) {
			return fmt.Errorf("Binary file or invalid UTF-8: %s", path)
		}
		content := string(fileContents)
		hasBOM := strings.HasPrefix(content, "\uFEFF")
		if hasBOM {
			content = strings.TrimLeft(content, "\uFEFF")
		}
		hasCRLF := strings.Contains(content, "\r\n")
		if hasCRLF {
			content = strings.ReplaceAll(content, "\r\n", "\n")
			oldText = strings.ReplaceAll(oldText, "\r\n", "\n")
		}
		strategy := "exact"
		workingContent, searchText := content, oldText
		if !strings.Contains(workingContent, searchText) {
			strategy = "whitespace-normalized"
			workingContent = normalize(content)
			searchText = normalize(oldText)
			if !strings.Contains(workingContent, searchText) {
				return fmt.Errorf("Could not find old_string in %s. The text must match exactly including all whitespace and newlines.", path)
			}
		}
		matchCount := strings.Count(workingContent, searchText)
		if !replaceAll && matchCount > 1 {
			return fmt.Errorf("Found %d occurrences of old_string in %s. The text must be unique or use replace_all=true.", matchCount, path)
		}
		updated := workingContent
		if replaceAll {
			updated = strings.ReplaceAll(workingContent, searchText, newText)
		} else {
			updated = strings.Replace(workingContent, searchText, newText, 1)
		}
		if updated == workingContent {
			return fmt.Errorf("No changes made to %s. The replacement produced identical content.", path)
		}
		if hasCRLF {
			updated = strings.ReplaceAll(updated, "\n", "\r\n")
		}
		if hasBOM {
			updated = "\uFEFF" + updated
		}
		temporaryFilePath := temporaryPath(path)
		if writeErr := os.WriteFile(temporaryFilePath, []byte(updated), 0666); writeErr != nil {
			if errors.Is(writeErr, os.ErrPermission) {
				return fmt.Errorf("Permission denied: %s", path)
			}
			return fmt.Errorf("Failed to write file: %w", writeErr)
		}
		if renameErr := os.Rename(temporaryFilePath, path); renameErr != nil {
			_ = os.Remove(temporaryFilePath)
			if errors.Is(renameErr, os.ErrPermission) {
				return fmt.Errorf("Permission denied: cannot rename temporary file to %s", path)
			}
			return fmt.Errorf("Failed to rename temporary file: %w", renameErr)
		}
		successMessage := fmt.Sprintf("Successfully replaced %d occurrence(s) in %s", matchCount, path)
		if strategy != "exact" {
			successMessage += fmt.Sprintf(" (using %s match)", strategy)
		}
		result.Text = successMessage
		after := updated
		result.Details = map[string]any{"file_path": path, "changed_files": []string{path}, "success": true, "replacements": matchCount, "file_deltas": deltas(path, before, &after), "capture_complete": true}
		return nil
	})
	return result, operationErr
}

func normalize(text string) string {
	lines := strings.Split(text, "\n")
	for i, line := range lines {
		lines[i] = strings.Join(strings.Fields(line), " ")
	}
	return strings.Join(lines, "\n")
}
func temporaryPath(filePath string) string {
	baseName := filepath.Base(filePath)
	fileExtension := filepath.Ext(filePath)
	if strings.HasPrefix(baseName, ".") && strings.Count(baseName, ".") == 1 {
		fileExtension = ""
	}
	return strings.TrimSuffix(filePath, fileExtension) + ".tmp"
}
func lineCount(content string) int {
	if content == "" {
		return 0
	}
	return len(strings.Split(strings.TrimSuffix(content, "\n"), "\n"))
}
func formatSize(byteCount int) string {
	if byteCount < 1024 {
		return fmt.Sprintf("%dB", byteCount)
	}
	if byteCount < 1024*1024 {
		return fmt.Sprintf("%.1fKB", float64(byteCount)/1024)
	}
	return fmt.Sprintf("%.1fMB", float64(byteCount)/(1024*1024))
}
func intOrDefault(providedValue *int, fallback int) int {
	if providedValue == nil {
		return fallback
	}
	return *providedValue
}
func readError(path string, readErr error) error {
	if errors.Is(readErr, os.ErrPermission) {
		return fmt.Errorf("Permission denied: %s", path)
	}
	return fmt.Errorf("Failed to read file: %w", readErr)
}
func utf8Valid(contentBytes []byte) bool { return utf8.Valid(contentBytes) }
func readBefore(path string) (*string, error) {
	fileContents, err := os.ReadFile(path)
	if errors.Is(err, os.ErrNotExist) {
		return nil, nil
	}
	if err != nil {
		return nil, err
	}
	if !utf8.Valid(fileContents) {
		return nil, fmt.Errorf("file is not valid UTF-8")
	}
	content := string(fileContents)
	return &content, nil
}
func deltas(path string, before, after *string) []diff.FileDelta {
	delta := diff.BuildFileDelta(path, before, after)
	if delta == nil {
		return []diff.FileDelta{}
	}
	return []diff.FileDelta{*delta}
}
