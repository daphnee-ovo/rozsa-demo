//go:build windows

package process

import (
	"context"
	"fmt"
	"os/exec"
	"strconv"
	"strings"
)

func Configure(_ *exec.Cmd) {}

func KillTree(ctx context.Context, command *exec.Cmd) error {
	if command.Process == nil {
		return nil
	}
	killer := exec.CommandContext(ctx, "taskkill", "/PID", strconv.Itoa(command.Process.Pid), "/T", "/F")
	output, err := killer.CombinedOutput()
	if err == nil {
		return nil
	}
	message := strings.ToLower(string(output))
	if strings.Contains(message, "no running instance") || strings.Contains(message, "not found") {
		return nil
	}
	return fmt.Errorf("taskkill process tree %d: %w: %s", command.Process.Pid, err, strings.TrimSpace(string(output)))
}
