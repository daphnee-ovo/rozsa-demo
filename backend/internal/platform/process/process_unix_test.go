//go:build darwin || linux

package process_test

import (
	"testing"
)

func TestKillTreeStopsRunningUnixProcess(t *testing.T) {
	assertKillTreeStopsDescendant(t)
}
