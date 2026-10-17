package file

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
)

func TestReadWriteEditRealFileAndEvidenceMetadata(t *testing.T) {
	tempDirectory := t.TempDir()
	path := filepath.Join(tempDirectory, "nested", "note.txt")
	writeResult, err := Write(context.Background(), path, "a\r\n  b  c\r\n")
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(writeResult.Text, "Successfully wrote") {
		t.Fatalf("write result: %q", writeResult.Text)
	}
	fileBytes, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if string(fileBytes) != "a\r\n  b  c\r\n" {
		t.Fatalf("write bytes: %q", fileBytes)
	}
	editResult, err := Edit(context.Background(), path, "b c", "changed", false)
	if err != nil {
		t.Fatal(err)
	}
	fileBytes, err = os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if string(fileBytes) != "a\r\nchanged\r\n" {
		t.Fatalf("edit bytes: %q", fileBytes)
	}
	if !strings.Contains(editResult.Text, "whitespace-normalized") {
		t.Fatalf("edit result: %q", editResult.Text)
	}
	readResult, err := Read(path, nil, nil)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(readResult.Text, "changed") {
		t.Fatalf("read output: %q", readResult.Text)
	}
	if writeDetails := writeResult.Details.(map[string]any); writeDetails["capture_complete"] != true || writeDetails["bytes_written"] != len("a\r\n  b  c\r\n") {
		t.Fatalf("write details: %#v", writeDetails)
	}
}

func TestEditAmbiguousMissingAndBOM(t *testing.T) {
	path := filepath.Join(t.TempDir(), "bom.txt")
	if err := os.WriteFile(path, []byte("\uFEFFrepeat repeat"), 0600); err != nil {
		t.Fatal(err)
	}
	if _, err := Edit(context.Background(), path, "repeat", "x", false); err == nil || !strings.Contains(err.Error(), "Found 2 occurrences") {
		t.Fatalf("ambiguous error: %v", err)
	}
	if _, err := Edit(context.Background(), path, "absent", "x", false); err == nil || !strings.Contains(err.Error(), "Could not find old_string") {
		t.Fatalf("missing error: %v", err)
	}
	if _, err := Edit(context.Background(), path, "repeat", "x", true); err != nil {
		t.Fatal(err)
	}
	fileContents, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if string(fileContents) != "\uFEFFx x" {
		t.Fatalf("BOM/replacement: %q", fileContents)
	}
}

func TestPathErrorsSymlinkAndCancellation(t *testing.T) {
	tempDirectory := t.TempDir()
	if _, err := Read(filepath.Join(tempDirectory, "missing"), nil, nil); err == nil || err.Error() != "File not found: "+filepath.Join(tempDirectory, "missing") {
		t.Fatalf("missing path error: %v", err)
	}
	target := filepath.Join(tempDirectory, "target")
	if err := os.WriteFile(target, []byte("ok"), 0600); err != nil {
		t.Fatal(err)
	}
	link := filepath.Join(tempDirectory, "link")
	if err := os.Symlink(target, link); err != nil {
		t.Skipf("symlink unavailable: %v", err)
	}
	readResult, err := Read(link, nil, nil)
	if err != nil || !strings.Contains(readResult.Text, "ok") {
		t.Fatalf("read symlink: %#v %v", readResult, err)
	}
	requestContext, cancel := context.WithCancel(context.Background())
	cancel()
	if _, err := Write(requestContext, target, "new"); err == nil {
		t.Fatal("cancelled write succeeded")
	}
	fileContents, _ := os.ReadFile(target)
	if string(fileContents) != "ok" {
		t.Fatalf("cancelled write changed file: %q", fileContents)
	}
}

func TestPathLockSerializesWrites(t *testing.T) {
	path := filepath.Join(t.TempDir(), "shared")
	var wg sync.WaitGroup
	for i := 0; i < 12; i++ {
		wg.Add(1)
		go func(i int) {
			defer wg.Done()
			if _, err := Write(context.Background(), path, strings.Repeat("x", i+1)); err != nil {
				t.Errorf("write: %v", err)
			}
		}(i)
	}
	wg.Wait()
	fileContents, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if len(fileContents) < 1 || len(fileContents) > 12 {
		t.Fatalf("unexpected final size: %d", len(fileContents))
	}
}
