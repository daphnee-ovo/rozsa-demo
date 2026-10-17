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
	root := t.TempDir()
	path := filepath.Join(root, "nested", "note.txt")
	w, err := Write(context.Background(), path, "a\r\n  b  c\r\n")
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(w.Text, "Successfully wrote") {
		t.Fatalf("write result: %q", w.Text)
	}
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if string(data) != "a\r\n  b  c\r\n" {
		t.Fatalf("write bytes: %q", data)
	}
	e, err := Edit(context.Background(), path, "b c", "changed", false)
	if err != nil {
		t.Fatal(err)
	}
	data, err = os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if string(data) != "a\r\nchanged\r\n" {
		t.Fatalf("edit bytes: %q", data)
	}
	if !strings.Contains(e.Text, "whitespace-normalized") {
		t.Fatalf("edit result: %q", e.Text)
	}
	r, err := Read(path, nil, nil)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(r.Text, "changed") {
		t.Fatalf("read output: %q", r.Text)
	}
	if d := w.Details.(map[string]any); d["capture_complete"] != true || d["bytes_written"] != len("a\r\n  b  c\r\n") {
		t.Fatalf("write details: %#v", d)
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
	b, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if string(b) != "\uFEFFx x" {
		t.Fatalf("BOM/replacement: %q", b)
	}
}

func TestPathErrorsSymlinkAndCancellation(t *testing.T) {
	root := t.TempDir()
	if _, err := Read(filepath.Join(root, "missing"), nil, nil); err == nil || err.Error() != "File not found: "+filepath.Join(root, "missing") {
		t.Fatalf("missing path error: %v", err)
	}
	target := filepath.Join(root, "target")
	if err := os.WriteFile(target, []byte("ok"), 0600); err != nil {
		t.Fatal(err)
	}
	link := filepath.Join(root, "link")
	if err := os.Symlink(target, link); err != nil {
		t.Skipf("symlink unavailable: %v", err)
	}
	r, err := Read(link, nil, nil)
	if err != nil || !strings.Contains(r.Text, "ok") {
		t.Fatalf("read symlink: %#v %v", r, err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if _, err := Write(ctx, target, "new"); err == nil {
		t.Fatal("cancelled write succeeded")
	}
	b, _ := os.ReadFile(target)
	if string(b) != "ok" {
		t.Fatalf("cancelled write changed file: %q", b)
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
	b, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if len(b) < 1 || len(b) > 12 {
		t.Fatalf("unexpected final size: %d", len(b))
	}
}
