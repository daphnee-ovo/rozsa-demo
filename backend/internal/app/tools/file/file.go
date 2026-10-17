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

func withLock(ctx context.Context, path string, fn func() error) error {
	if err := ctx.Err(); err != nil {
		return err
	}
	value, _ := locks.LoadOrStore(path, func() chan struct{} { c := make(chan struct{}, 1); c <- struct{}{}; return c }())
	lock := value.(chan struct{})
	select {
	case <-ctx.Done():
		return ctx.Err()
	case <-lock:
	}
	defer func() { lock <- struct{}{} }()
	if err := ctx.Err(); err != nil {
		return err
	}
	return fn()
}

func Read(path string, offset, limit *int) (Result, error) {
	info, err := os.Stat(path)
	if errors.Is(err, os.ErrNotExist) {
		return Result{}, fmt.Errorf("File not found: %s", path)
	}
	if err != nil {
		return Result{}, readError(path, err)
	}
	if !info.Mode().IsRegular() {
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
	start := 0
	if offset != nil && *offset > 1 {
		start = *offset - 1
	}
	if start >= len(lines) && len(lines) > 0 {
		return Result{}, fmt.Errorf("Offset %d is beyond end of file (%d lines total)", value(offset, 1), len(lines))
	}
	end := len(lines)
	if limit != nil && *limit >= 0 && start+*limit < end {
		end = start + *limit
	}
	selected := lines[start:end]
	out := make([]string, 0, len(selected))
	used := 0
	var by *string
	firstTooLarge := len(selected) > 0 && len(selected[0]) > maxBytes
	if firstTooLarge {
		return Result{}, fmt.Errorf("[Line %d is %s, exceeds %s limit. Use bash: sed -n '%dp' %s | head -c %d]", start+1, formatSize(len(selected[0])), formatSize(maxBytes), start+1, path, maxBytes)
	}
	for i, line := range selected {
		if i >= maxLines {
			value := "lines"
			by = &value
			break
		}
		n := len(line)
		if i > 0 {
			n++
		}
		if used+n > maxBytes {
			value := "bytes"
			by = &value
			break
		}
		out = append(out, fmt.Sprintf("%6d\t%s", start+i+1, line))
		used += n
	}
	truncated := len(out) < len(selected)
	text := strings.Join(out, "\n")
	if truncated {
		if by != nil && *by == "lines" {
			text += fmt.Sprintf("\n\n[Showing lines %d-%d of %d. Use offset=%d to continue.]", start+1, start+len(out), len(lines), start+len(out)+1)
		} else {
			text += fmt.Sprintf("\n\n[Showing lines %d-%d of %d (%s limit). Use offset=%d to continue.]", start+1, start+len(out), len(lines), formatSize(maxBytes), start+len(out)+1)
		}
	} else if limit != nil && start+len(out) < len(lines) {
		text += fmt.Sprintf("\n\n[%d more lines in file. Use offset=%d to continue.]", len(lines)-start-len(out), start+len(out)+1)
	}
	return Result{Text: text, Details: Truncation{truncated, by, len(lines), len(out), firstTooLarge, maxLines, maxBytes}}, nil
}

func Write(ctx context.Context, path, content string) (Result, error) {
	var result Result
	before, captureErr := readBefore(path)
	if captureErr != nil {
		return Result{}, fmt.Errorf("Failed to capture file evidence before write: %w", captureErr)
	}
	err := withLock(ctx, path, func() error {
		parent := filepath.Dir(path)
		if parent != "." {
			if e := os.MkdirAll(parent, 0755); e != nil {
				if errors.Is(e, os.ErrPermission) {
					return fmt.Errorf("Permission denied: cannot create parent directory for %s", path)
				}
				return fmt.Errorf("Failed to create parent directory: %w", e)
			}
		}
		tmp := temporaryPath(path)
		if e := os.WriteFile(tmp, []byte(content), 0666); e != nil {
			if errors.Is(e, os.ErrPermission) {
				return fmt.Errorf("Permission denied: %s", path)
			}
			return fmt.Errorf("Failed to write file: %w", e)
		}
		if e := os.Rename(tmp, path); e != nil {
			_ = os.Remove(tmp)
			if errors.Is(e, os.ErrPermission) {
				return fmt.Errorf("Permission denied: cannot rename temporary file to %s", path)
			}
			return fmt.Errorf("Failed to rename temporary file: %w", e)
		}
		lineCount := lineCount(content)
		result.Text = fmt.Sprintf("Successfully wrote %s to %s (%d lines)", formatSize(len(content)), path, lineCount)
		after := content
		result.Details = map[string]any{"file_path": path, "changed_files": []string{path}, "success": true, "bytes_written": len(content), "line_count": lineCount, "file_deltas": deltas(path, before, &after), "capture_complete": true}
		return nil
	})
	return result, err
}

func Edit(ctx context.Context, path, oldText, newText string, replaceAll bool) (Result, error) {
	var result Result
	before, captureErr := readBefore(path)
	if captureErr != nil {
		return Result{}, fmt.Errorf("Failed to capture file evidence before edit: %w", captureErr)
	}
	err := withLock(ctx, path, func() error {
		info, e := os.Stat(path)
		if errors.Is(e, os.ErrNotExist) {
			return fmt.Errorf("File not found: %s", path)
		}
		if e != nil {
			return readError(path, e)
		}
		if !info.Mode().IsRegular() {
			return fmt.Errorf("Not a file: %s", path)
		}
		bytes, e := os.ReadFile(path)
		if e != nil {
			return readError(path, e)
		}
		if !utf8Valid(bytes) {
			return fmt.Errorf("Binary file or invalid UTF-8: %s", path)
		}
		content := string(bytes)
		bom := strings.HasPrefix(content, "\uFEFF")
		if bom {
			content = strings.TrimLeft(content, "\uFEFF")
		}
		crlf := strings.Contains(content, "\r\n")
		if crlf {
			content = strings.ReplaceAll(content, "\r\n", "\n")
			oldText = strings.ReplaceAll(oldText, "\r\n", "\n")
		}
		strategy := "exact"
		working, needle := content, oldText
		if !strings.Contains(working, needle) {
			strategy = "whitespace-normalized"
			working = normalize(content)
			needle = normalize(oldText)
			if !strings.Contains(working, needle) {
				return fmt.Errorf("Could not find old_string in %s. The text must match exactly including all whitespace and newlines.", path)
			}
		}
		count := strings.Count(working, needle)
		if !replaceAll && count > 1 {
			return fmt.Errorf("Found %d occurrences of old_string in %s. The text must be unique or use replace_all=true.", count, path)
		}
		updated := working
		if replaceAll {
			updated = strings.ReplaceAll(working, needle, newText)
		} else {
			updated = strings.Replace(working, needle, newText, 1)
		}
		if updated == working {
			return fmt.Errorf("No changes made to %s. The replacement produced identical content.", path)
		}
		if crlf {
			updated = strings.ReplaceAll(updated, "\n", "\r\n")
		}
		if bom {
			updated = "\uFEFF" + updated
		}
		tmp := temporaryPath(path)
		if e = os.WriteFile(tmp, []byte(updated), 0666); e != nil {
			if errors.Is(e, os.ErrPermission) {
				return fmt.Errorf("Permission denied: %s", path)
			}
			return fmt.Errorf("Failed to write file: %w", e)
		}
		if e = os.Rename(tmp, path); e != nil {
			_ = os.Remove(tmp)
			if errors.Is(e, os.ErrPermission) {
				return fmt.Errorf("Permission denied: cannot rename temporary file to %s", path)
			}
			return fmt.Errorf("Failed to rename temporary file: %w", e)
		}
		msg := fmt.Sprintf("Successfully replaced %d occurrence(s) in %s", count, path)
		if strategy != "exact" {
			msg += fmt.Sprintf(" (using %s match)", strategy)
		}
		result.Text = msg
		after := updated
		result.Details = map[string]any{"file_path": path, "changed_files": []string{path}, "success": true, "replacements": count, "file_deltas": deltas(path, before, &after), "capture_complete": true}
		return nil
	})
	return result, err
}

func normalize(s string) string {
	lines := strings.Split(s, "\n")
	for i, line := range lines {
		lines[i] = strings.Join(strings.Fields(line), " ")
	}
	return strings.Join(lines, "\n")
}
func temporaryPath(path string) string {
	base := filepath.Base(path)
	ext := filepath.Ext(path)
	if strings.HasPrefix(base, ".") && strings.Count(base, ".") == 1 {
		ext = ""
	}
	return strings.TrimSuffix(path, ext) + ".tmp"
}
func lineCount(s string) int {
	if s == "" {
		return 0
	}
	return len(strings.Split(strings.TrimSuffix(s, "\n"), "\n"))
}
func formatSize(n int) string {
	if n < 1024 {
		return fmt.Sprintf("%dB", n)
	}
	if n < 1024*1024 {
		return fmt.Sprintf("%.1fKB", float64(n)/1024)
	}
	return fmt.Sprintf("%.1fMB", float64(n)/(1024*1024))
}
func value(v *int, fallback int) int {
	if v == nil {
		return fallback
	}
	return *v
}
func readError(path string, e error) error {
	if errors.Is(e, os.ErrPermission) {
		return fmt.Errorf("Permission denied: %s", path)
	}
	return fmt.Errorf("Failed to read file: %w", e)
}
func utf8Valid(b []byte) bool { return utf8.Valid(b) }
func readBefore(path string) (*string, error) {
	b, err := os.ReadFile(path)
	if errors.Is(err, os.ErrNotExist) {
		return nil, nil
	}
	if err != nil {
		return nil, err
	}
	if !utf8.Valid(b) {
		return nil, fmt.Errorf("file is not valid UTF-8")
	}
	s := string(b)
	return &s, nil
}
func deltas(path string, before, after *string) []diff.FileDelta {
	delta := diff.BuildFileDelta(path, before, after)
	if delta == nil {
		return []diff.FileDelta{}
	}
	return []diff.FileDelta{*delta}
}
