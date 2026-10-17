//go:build !windows

package bash

import (
	"context"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func TestSplitSegmentsMatchesPermissionContract(t *testing.T) {
	tests := []struct {
		command string
		want    []string
	}{
		{"ls | grep foo", []string{"ls", "grep foo"}},
		{"make && make install", []string{"make", "make install"}},
		{"cmd1 || cmd2", []string{"cmd1", "cmd2"}},
		{"cmd1; cmd2; cmd3", []string{"cmd1", "cmd2", "cmd3"}},
		{`echo "hello | world" | grep hello`, []string{`echo "hello | world"`, "grep hello"}},
		{"echo 'a && b' && rm file", []string{"echo 'a && b'", "rm file"}},
	}
	for _, test := range tests {
		got := SplitSegments(test.command)
		if strings.Join(got, "\x00") != strings.Join(test.want, "\x00") {
			t.Errorf("SplitSegments(%q) = %#v, want %#v", test.command, got, test.want)
		}
	}
}

func TestExecutePreservesWorkingDirectoryOutputAndExit(t *testing.T) {
	workingDirectory := t.TempDir()
	if err := os.Mkdir(filepath.Join(workingDirectory, "subdir"), 0755); err != nil {
		t.Fatal(err)
	}
	result, err := Execute(context.Background(), `printf 'héllo\n'; printf 'warning\n' >&2; printf '%s\n' "space ' quote"; cd subdir`, workingDirectory, time.Second*5)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(result.Output, "héllo\n") || !strings.Contains(result.Output, "warning\n") || !strings.Contains(result.Output, "space ' quote\n") {
		t.Fatalf("combined output = %q", result.Output)
	}
	wantCWD := filepath.Join(workingDirectory, "subdir")
	if result.CWD != wantCWD {
		t.Fatalf("cwd = %q, want %q", result.CWD, wantCWD)
	}
	if result.ExitCode == nil || *result.ExitCode != 0 {
		t.Fatalf("exit code = %v, want 0", result.ExitCode)
	}
}

func TestExecuteResolvesExecutableThroughPATH(t *testing.T) {
	if err := os.Mkdir(filepath.Join(t.TempDir(), "bin"), 0755); err != nil {
		t.Fatal(err)
	}
	result, err := Execute(context.Background(), "command -v bash", t.TempDir(), time.Second*5)
	if err != nil || !strings.Contains(result.Output, "bash") {
		t.Fatalf("PATH resolution result=%#v err=%v", result, err)
	}
}

func TestExecuteTimeoutReportsOutputAndKillsCommand(t *testing.T) {
	result, err := Execute(context.Background(), "printf started; sleep 30", t.TempDir(), 40*time.Millisecond)
	if err == nil || !strings.Contains(err.Error(), "Command timed out after 40ms") {
		t.Fatalf("timeout error = %v", err)
	}
	if !result.TimedOut || !strings.Contains(err.Error(), "started") {
		t.Fatalf("timeout result=%#v error=%v", result, err)
	}
}

func TestExecuteCancellationKillsCommand(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	go func() {
		time.Sleep(40 * time.Millisecond)
		cancel()
	}()
	_, err := Execute(ctx, "sleep 30", t.TempDir(), time.Minute)
	if err == nil || err.Error() != "Command cancelled" {
		t.Fatalf("cancel error = %v", err)
	}
}

func TestExecuteCancellationKillsDescendantProcess(t *testing.T) {
	workingDirectory := t.TempDir()
	pidFile := filepath.Join(workingDirectory, "descendant.pid")
	ctx, cancel := context.WithCancel(context.Background())
	done := make(chan error, 1)
	go func() {
		_, err := Execute(ctx, "sleep 30 & child=$!; printf '%s' \"$child\" > '"+pidFile+"'; wait", workingDirectory, time.Minute)
		done <- err
	}()
	deadline := time.Now().Add(2 * time.Second)
	var pid string
	for time.Now().Before(deadline) {
		contents, err := os.ReadFile(pidFile)
		if err == nil && len(contents) > 0 {
			pid = string(contents)
			break
		}
		time.Sleep(10 * time.Millisecond)
	}
	if pid == "" {
		cancel()
		t.Fatal("descendant did not start")
	}
	cancel()
	if err := <-done; err == nil || err.Error() != "Command cancelled" {
		t.Fatalf("cancel error = %v", err)
	}
	deadline = time.Now().Add(2 * time.Second)
	for time.Now().Before(deadline) {
		if err := exec.Command("/bin/kill", "-0", pid).Run(); err != nil {
			return
		}
		time.Sleep(10 * time.Millisecond)
	}
	t.Fatalf("descendant process %s is still alive", pid)
}

func TestExecuteOutputLimitTruncates(t *testing.T) {
	result, err := Execute(context.Background(), "yes x", t.TempDir(), time.Second*5)
	if err != nil {
		t.Fatal(err)
	}
	if !result.Truncated || len(result.Output) > MaxOutputBytes {
		t.Fatalf("truncated=%v output bytes=%d", result.Truncated, len(result.Output))
	}
}
