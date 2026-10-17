package diff

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

func TestBuildFileDeltaStatusesAndTurnMerge(t *testing.T) {
	oldContent, newContent := "old\n", "new\n"
	fileDelta := BuildFileDelta("a.txt", &oldContent, &newContent)
	if fileDelta == nil || fileDelta.Status != "modified" || fileDelta.Added != 1 || fileDelta.Deleted != 1 || !strings.Contains(fileDelta.Patch, "-old\n+new") {
		t.Fatalf("delta: %#v", fileDelta)
	}
	addedDelta := BuildFileDelta("new", nil, &newContent)
	if addedDelta == nil || addedDelta.Status != "added" {
		t.Fatalf("added delta: %#v", addedDelta)
	}
	activity, err := MergeResults([]map[string]any{
		{"details": map[string]any{"file_deltas": []any{map[string]any{"path": "a.txt", "before": oldContent, "after": "middle\n"}}, "capture_complete": true}},
		{"details": map[string]any{"file_deltas": []any{map[string]any{"path": "a.txt", "before": "middle\n", "after": newContent}}, "capture_complete": false, "capture_limitation": "later read failed"}},
	})
	if err != nil {
		t.Fatal(err)
	}
	if activity.CaptureComplete || activity.CaptureLimitation != "later read failed" || len(activity.FileChanges) != 1 || *activity.FileChanges[0].Before != oldContent || *activity.FileChanges[0].After != newContent {
		t.Fatalf("merged activity: %#v", activity)
	}
}

func TestReadWorkspaceDiffTrackedAndUntrackedAndPathValidation(t *testing.T) {
	workspace := t.TempDir()
	runGitTest(t, workspace, "init", "-q")
	runGitTest(t, workspace, "-c", "user.name=Test", "-c", "user.email=test@example.invalid", "commit", "--allow-empty", "-m", "initial")
	trackedFile := filepath.Join(workspace, "tracked.txt")
	if err := os.WriteFile(trackedFile, []byte("base\n"), 0600); err != nil {
		t.Fatal(err)
	}
	runGitTest(t, workspace, "add", "tracked.txt")
	runGitTest(t, workspace, "-c", "user.name=Test", "-c", "user.email=test@example.invalid", "commit", "-m", "tracked")
	if err := os.WriteFile(trackedFile, []byte("changed\n"), 0600); err != nil {
		t.Fatal(err)
	}
	trackedDiff, err := ReadWorkspaceDiff(workspace, "tracked.txt")
	if err != nil || !strings.Contains(trackedDiff.Patch, "-base") || !strings.Contains(trackedDiff.Patch, "+changed") {
		t.Fatalf("tracked diff %#v err %v", trackedDiff, err)
	}
	if err := os.WriteFile(filepath.Join(workspace, "new.txt"), []byte("new\n"), 0600); err != nil {
		t.Fatal(err)
	}
	untrackedDiff, err := ReadWorkspaceDiff(workspace, "new.txt")
	if err != nil || !strings.Contains(untrackedDiff.Patch, "+new") {
		t.Fatalf("untracked diff %#v err %v", untrackedDiff, err)
	}
	if _, err = ReadWorkspaceDiff(workspace, "../outside"); err == nil || !strings.Contains(err.Error(), "stay within") {
		t.Fatalf("path traversal error: %v", err)
	}
	if _, err = ReadWorkspaceDiff(t.TempDir(), "file"); err == nil || !strings.Contains(err.Error(), "not a Git repository") {
		t.Fatalf("non-repo error: %v", err)
	}
}

func runGitTest(t *testing.T, dir string, gitArguments ...string) {
	t.Helper()
	gitCommand := exec.Command("git", append([]string{"-C", dir}, gitArguments...)...)
	commandOutput, err := gitCommand.CombinedOutput()
	if err != nil {
		t.Fatalf("git %v: %v: %s", gitArguments, err, commandOutput)
	}
}
