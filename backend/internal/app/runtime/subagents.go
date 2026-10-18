package runtime

import (
	"context"
	"encoding/json"
	"errors"

	"rozsa/backend/internal/app/sessions"
	"rozsa/backend/internal/app/subagent"
	subagenttool "rozsa/backend/internal/app/tools/subagent"
	"rozsa/backend/internal/core/tools"
)

// SubagentManager is keyed by the originating session, independently of UI
// selection and message subscriptions. Closing a renderer does not own it.
func (runtime *Runtime) SubagentManager(sessionID string) (subagent.Manager, error) {
	runtime.mu.Lock()
	defer runtime.mu.Unlock()
	if runtime.closing {
		return nil, sessions.ErrClosed
	}
	if manager := runtime.subagents[sessionID]; manager != nil {
		return manager, nil
	}
	var source sessions.Status
	for _, status := range runtime.lifecycle.Snapshot().Sessions {
		if status.SessionID == sessionID {
			source = status
			break
		}
	}
	if source.Path == "" {
		return nil, errors.New("subagent owner session is unavailable")
	}
	// The registered writer owns its cwd and UUID; no renderer-supplied path is
	// accepted here, and subagents never inherit the main transcript.
	writer := runtime.session
	if writer == nil || writer.Path() != source.Path {
		var err error
		writer, err = runtime.service.WritableManager(source.Path, source.CatalogID)
		if err != nil {
			return nil, err
		}
	}
	manager, err := subagent.New(subagent.Config{SessionDir: runtime.sessionDir, MainSessionUUID: writer.SessionID(), MainSessionFile: writer.Path(), Cwd: writer.Cwd(), Model: runtime.model, Provider: runtime.provider,
		ToSessionMessage: toSessionMessage,
		Tools: func(id string) []tools.Tool {
			return []tools.Tool{askUserQuestionTool{sessionID: id, owner: runtime.questions}}
		},
		LifecycleHooks: sessions.Hooks{
			RejectApprovals: func(id string) error { runtime.approvals.CloseSession(id); return nil },
			CancelQuestions: func(id string) error { runtime.questions.CancelSession(id); return nil },
		},
		Emit: func(event subagent.Event) {
			runtime.mu.Lock()
			defer runtime.mu.Unlock()
			runtime.subagentEvents[sessionID] = append(runtime.subagentEvents[sessionID], event)
		},
	})
	if err != nil {
		return nil, err
	}
	if runtime.subagents == nil {
		runtime.subagents = make(map[string]subagent.Manager)
		runtime.subagentEvents = make(map[string][]subagent.Event)
	}
	runtime.subagents[sessionID] = manager
	return manager, nil
}

// SubagentEvents retains causal identity across renderer reload. Transport/UI
// projection can request a suffix without recreating an execution.
func (runtime *Runtime) SubagentEvents(sessionID string, after uint64) []subagent.Event {
	runtime.mu.Lock()
	defer runtime.mu.Unlock()
	result := []subagent.Event{}
	for _, event := range runtime.subagentEvents[sessionID] {
		if event.Sequence > after {
			result = append(result, event)
		}
	}
	return result
}

func (runtime *Runtime) shutdownSubagents(ctx context.Context) error {
	runtime.mu.Lock()
	managers := make([]subagent.Manager, 0, len(runtime.subagents))
	for _, manager := range runtime.subagents {
		managers = append(managers, manager)
	}
	runtime.mu.Unlock()
	var err error
	for _, manager := range managers {
		err = errors.Join(err, manager.Shutdown(ctx))
	}
	return err
}

type runtimeSubagentTool struct {
	tools.Tool
	runtime   *Runtime
	sessionID string
}

func (runtime *Runtime) subagentTool(sessionID string) tools.Tool {
	return runtimeSubagentTool{Tool: subagenttool.New(nil), runtime: runtime, sessionID: sessionID}
}
func (tool runtimeSubagentTool) Execute(ctx context.Context, callID string, arguments json.RawMessage, update func(tools.Result)) (tools.Result, error) {
	manager, err := tool.runtime.SubagentManager(tool.sessionID)
	if err != nil {
		return tools.Result{}, err
	}
	return subagenttool.New(manager).Execute(ctx, callID, arguments, update)
}
