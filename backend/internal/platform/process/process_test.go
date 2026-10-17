package process

import (
	"context"
	"os/exec"
	"testing"
)

func TestPlatformAdapterConfiguresCommand(t *testing.T) {
	command := exec.Command("echo", "ok")
	Configure(command)
	if err := KillTree(context.Background(), command); err != nil {
		t.Fatalf("kill before start: %v", err)
	}
}
