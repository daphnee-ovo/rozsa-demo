package subagent

import (
	"context"
	"encoding/json"
	"sync"

	"rozsa/backend/internal/core/tools"
)

// Core cancellation may finish before an Execute goroutine returns. The
// subagent owns that cleanup too: once sealed, late goroutines cannot start
// work, and completion waits for every already-started tool/hook to return.
type executionWork struct {
	mu      sync.Mutex
	active  int
	sealed  bool
	drained chan struct{}
}

func newExecutionWork() *executionWork { return &executionWork{drained: make(chan struct{})} }
func (work *executionWork) begin(ctx context.Context) bool {
	work.mu.Lock()
	defer work.mu.Unlock()
	if work.sealed || ctx.Err() != nil {
		return false
	}
	work.active++
	return true
}
func (work *executionWork) finish() {
	work.mu.Lock()
	defer work.mu.Unlock()
	work.active--
	if work.sealed && work.active == 0 {
		close(work.drained)
	}
}
func (work *executionWork) wait() {
	work.mu.Lock()
	work.sealed = true
	if work.active == 0 {
		close(work.drained)
	}
	work.mu.Unlock()
	<-work.drained
}

type ownedTool struct {
	tools.Tool
	work *executionWork
}

func (tool ownedTool) Execute(ctx context.Context, callID string, arguments json.RawMessage, update func(tools.Result)) (tools.Result, error) {
	if !tool.work.begin(ctx) {
		return tools.Result{}, context.Canceled
	}
	defer tool.work.finish()
	return tool.Tool.Execute(ctx, callID, arguments, update)
}
