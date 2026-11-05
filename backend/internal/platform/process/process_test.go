package process_test

import (
	"context"
	"errors"
	"os"
	"os/exec"
	"path/filepath"
	"testing"
	"time"

	"rozsa/backend/internal/platform/process"
)

func TestKillTreeParentHelper(t *testing.T) {
	if os.Getenv("T214_PROCESS_TREE_ROLE") != "parent" {
		return
	}
	readyPath, releasePath := os.Getenv("T214_PROCESS_TREE_READY"), os.Getenv("T214_PROCESS_TREE_RELEASE")
	command := exec.Command(os.Args[0], "-test.run=^TestKillTreeGrandchildHelper$")
	command.Env = append(os.Environ(), "T214_PROCESS_TREE_ROLE=grandchild", "T214_PROCESS_TREE_READY="+readyPath, "T214_PROCESS_TREE_RELEASE="+releasePath, "T214_PROCESS_TREE_SURVIVED="+os.Getenv("T214_PROCESS_TREE_SURVIVED"))
	if err := command.Start(); err != nil {
		t.Fatal(err)
	}
	defer command.Wait()
	deadline := time.Now().Add(5 * time.Second)
	for {
		if _, err := os.Stat(readyPath); err == nil {
			break
		} else if !errors.Is(err, os.ErrNotExist) {
			t.Fatal(err)
		} else if time.Now().After(deadline) {
			t.Fatal("descendant did not report readiness")
		}
		time.Sleep(10 * time.Millisecond)
	}
	for {
		if _, err := os.Stat(releasePath); err == nil {
			return
		}
		time.Sleep(10 * time.Millisecond)
	}
}

func TestKillTreeGrandchildHelper(t *testing.T) {
	if os.Getenv("T214_PROCESS_TREE_ROLE") != "grandchild" {
		return
	}
	if err := os.WriteFile(os.Getenv("T214_PROCESS_TREE_READY"), []byte("ready"), 0o600); err != nil {
		t.Fatal(err)
	}
	for {
		if _, err := os.Stat(os.Getenv("T214_PROCESS_TREE_RELEASE")); err == nil {
			if err := os.WriteFile(os.Getenv("T214_PROCESS_TREE_SURVIVED"), []byte("alive"), 0o600); err != nil {
				t.Fatal(err)
			}
			return
		}
		time.Sleep(10 * time.Millisecond)
	}
}

func assertKillTreeStopsDescendant(t *testing.T) {
	t.Helper()
	directory := t.TempDir()
	readyPath := filepath.Join(directory, "grandchild-ready")
	releasePath := filepath.Join(directory, "release-grandchild")
	survivedPath := filepath.Join(directory, "grandchild-survived")
	command := exec.Command(os.Args[0], "-test.run=^TestKillTreeParentHelper$")
	command.Env = append(os.Environ(), "T214_PROCESS_TREE_ROLE=parent", "T214_PROCESS_TREE_READY="+readyPath, "T214_PROCESS_TREE_RELEASE="+releasePath, "T214_PROCESS_TREE_SURVIVED="+survivedPath)
	process.Configure(command)
	if err := command.Start(); err != nil {
		t.Fatal(err)
	}
	waited := make(chan error, 1)
	go func() { waited <- command.Wait() }()
	parentWaited := false
	t.Cleanup(func() {
		_ = os.WriteFile(releasePath, []byte("release"), 0o600)
		if parentWaited {
			return
		}
		cleanupContext, cancelCleanup := context.WithTimeout(context.Background(), 2*time.Second)
		defer cancelCleanup()
		if err := process.KillTree(cleanupContext, command); err != nil {
			t.Errorf("kill test process tree during cleanup: %v", err)
		}
		select {
		case <-waited:
		case <-time.After(3 * time.Second):
			t.Error("test process tree did not stop during cleanup")
		}
	})
	deadline := time.Now().Add(5 * time.Second)
	for {
		if _, err := os.Stat(readyPath); err == nil {
			break
		} else if !errors.Is(err, os.ErrNotExist) {
			t.Fatal(err)
		} else if time.Now().After(deadline) {
			t.Fatal("descendant did not report readiness")
		}
		time.Sleep(10 * time.Millisecond)
	}
	killContext, cancelKill := context.WithTimeout(context.Background(), 2*time.Second)
	killErr := process.KillTree(killContext, command)
	cancelKill()
	if killErr != nil {
		t.Fatal(killErr)
	}
	select {
	case err := <-waited:
		parentWaited = true
		if err == nil {
			t.Fatal("parent process exited successfully after KillTree")
		}
	case <-time.After(3 * time.Second):
		t.Fatal("parent process remained running after KillTree")
	}
	if err := os.WriteFile(releasePath, []byte("release"), 0o600); err != nil {
		t.Fatal(err)
	}
	deadline = time.Now().Add(time.Second)
	for time.Now().Before(deadline) {
		if _, err := os.Stat(survivedPath); err == nil {
			t.Fatal("descendant survived KillTree")
		} else if !errors.Is(err, os.ErrNotExist) {
			t.Fatal(err)
		}
		time.Sleep(10 * time.Millisecond)
	}
}
