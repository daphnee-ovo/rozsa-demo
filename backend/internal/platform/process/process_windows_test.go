//go:build windows

package process_test

import (
	"testing"
)

func TestKillTreeStopsRunningWindowsProcess(t *testing.T) {
	assertKillTreeStopsDescendant(t)
}
