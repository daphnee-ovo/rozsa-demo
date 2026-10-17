//go:build darwin || linux

package process

import (
	"context"
	"os"
	"os/exec"
	"syscall"
)

func Configure(command *exec.Cmd) {
	command.SysProcAttr = &syscall.SysProcAttr{Setpgid: true}
}

func KillTree(_ context.Context, command *exec.Cmd) error {
	if command.Process == nil {
		return nil
	}
	groupID := command.Process.Pid
	err := syscall.Kill(-groupID, syscall.SIGKILL)
	if err == syscall.ESRCH {
		return nil
	}
	if err != nil && err != os.ErrProcessDone {
		return err
	}
	return nil
}
