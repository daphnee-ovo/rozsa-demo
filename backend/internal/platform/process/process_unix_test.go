//go:build darwin || linux

package process

import (
	"os/exec"
	"testing"
)

func TestUnixAdapterStartsAnIsolatedProcessGroup(t *testing.T) {
	command := exec.Command("true")
	Configure(command)
	if command.SysProcAttr == nil || !command.SysProcAttr.Setpgid {
		t.Fatal("Unix adapter must create an isolated process group")
	}
}
