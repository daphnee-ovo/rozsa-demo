package diff

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

func TestBuildFileDeltaStatusesAndTurnMerge(t *testing.T) {
	old, newValue := "old\n", "new\n"
	d := BuildFileDelta("a.txt", &old, &newValue)
	if d == nil || d.Status != "modified" || d.Added != 1 || d.Deleted != 1 || !strings.Contains(d.Patch, "-old\n+new") {
		t.Fatalf("delta: %#v", d)
	}
	added := BuildFileDelta("new", nil, &newValue)
	if added == nil || added.Status != "added" {
		t.Fatalf("added delta: %#v", added)
	}
	activity, err := MergeResults([]map[string]any{
		{"details": map[string]any{"file_deltas": []any{map[string]any{"path": "a.txt", "before": old, "after": "middle\n"}}, "capture_complete": true}},
		{"details": map[string]any{"file_deltas": []any{map[string]any{"path": "a.txt", "before": "middle\n", "after": newValue}}, "capture_complete": false, "capture_limitation": "later read failed"}},
	})
	if err != nil {
		t.Fatal(err)
	}
	if activity.CaptureComplete || activity.CaptureLimitation != "later read failed" || len(activity.FileChanges) != 1 || *activity.FileChanges[0].Before != old || *activity.FileChanges[0].After != newValue {
		t.Fatalf("merged activity: %#v", activity)
	}
}

func TestReadWorkspaceDiffTrackedAndUntrackedAndPathValidation(t *testing.T) {
	root := t.TempDir()
	runGitTest(t, root, "init", "-q")
	runGitTest(t, root, "-c", "user.name=Test", "-c", "user.email=test@example.invalid", "commit", "--allow-empty", "-m", "initial")
	tracked := filepath.Join(root, "tracked.txt")
	if err := os.WriteFile(tracked, []byte("base\n"), 0600); err != nil {
		t.Fatal(err)
	}
	runGitTest(t, root, "add", "tracked.txt")
	runGitTest(t, root, "-c", "user.name=Test", "-c", "user.email=test@example.invalid", "commit", "-m", "tracked")
	if err := os.WriteFile(tracked, []byte("changed\n"), 0600); err != nil {
		t.Fatal(err)
	}
	d, err := ReadWorkspaceDiff(root, "tracked.txt")
	if err != nil || !strings.Contains(d.Patch, "-base") || !strings.Contains(d.Patch, "+changed") {
		t.Fatalf("tracked diff %#v err %v", d, err)
	}
	if err := os.WriteFile(filepath.Join(root, "new.txt"), []byte("new\n"), 0600); err != nil {
		t.Fatal(err)
	}
	d, err = ReadWorkspaceDiff(root, "new.txt")
	if err != nil || !strings.Contains(d.Patch, "+new") {
		t.Fatalf("untracked diff %#v err %v", d, err)
	}
	if _, err = ReadWorkspaceDiff(root, "../outside"); err == nil || !strings.Contains(err.Error(), "stay within") {
		t.Fatalf("path traversal error: %v", err)
	}
	if _, err = ReadWorkspaceDiff(t.TempDir(), "file"); err == nil || !strings.Contains(err.Error(), "not a Git repository") {
		t.Fatalf("non-repo error: %v", err)
	}
}

func runGitTest(t *testing.T, dir string, args ...string) {
	t.Helper()
	cmd := exec.Command("git", append([]string{"-C", dir}, args...)...)
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("git %v: %v: %s", args, err, out)
	}
}
