//go:build !darwin && !linux && !windows

package process

import (
	"context"
	"os/exec"
)

func Configure(_ *exec.Cmd) {}

func KillTree(_ context.Context, command *exec.Cmd) error {
	if command.Process == nil {
		return nil
	}
	return command.Process.Kill()
}
