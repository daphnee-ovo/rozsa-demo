//go:build windows

package process

import (
	"os/exec"
	"testing"
)

func TestWindowsAdapterLeavesProcessCreationToWindows(t *testing.T) {
	command := exec.Command("cmd.exe", "/c", "exit 0")
	Configure(command)
	if command.SysProcAttr != nil {
		t.Fatal("Windows adapter must not use Unix process-group attributes")
	}
}
