package subagent

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"

	"rozsa/backend/internal/app/session"
	bash "rozsa/backend/internal/app/tools/bash"
	"rozsa/backend/internal/core/agent"
	"rozsa/backend/internal/core/hooks"
	"rozsa/backend/internal/core/tools"
	"rozsa/backend/internal/model/types"
)

type providerFunc func(context.Context, types.Model, types.Context, types.SimpleStreamOptions, func(types.StreamEvent)) error

func (provider providerFunc) StreamSimple(ctx context.Context, model types.Model, input types.Context, options types.SimpleStreamOptions, emit func(types.StreamEvent)) error {
	return provider(ctx, model, input, options, emit)
}
func finish(emit func(types.StreamEvent), text string) {
	emit(types.Done{Reason: types.Stop, Message: types.AssistantMessage{Content: []types.ContentBlock{types.TextBlock{Text: text}}, StopReason: types.Stop}})
}
func testConverter(message hooks.Message) (session.Message, bool) {
	switch value := message.Standard.(type) {
	case types.UserMessage:
		return session.UserMessage{Content: session.UserText(value.Content.(types.UserText)), Timestamp: value.Timestamp}, true
	case types.AssistantMessage:
		blocks := []session.ContentBlock{}
		for _, block := range value.Content {
			switch block := block.(type) {
			case types.TextBlock:
				blocks = append(blocks, session.TextBlock{Text: block.Text})
			case types.ToolCallBlock:
				blocks = append(blocks, session.ToolCallBlock{ID: block.ID, Name: block.Name, Arguments: block.Arguments})
			}
		}
		return session.AssistantMessage{Content: blocks, StopReason: session.StopReason(value.StopReason), Timestamp: value.Timestamp, ErrorMessage: value.ErrorMessage}, true
	case types.ToolResultMessage:
		return session.ToolResultMessage{ToolCallID: value.ToolCallID, ToolName: value.ToolName, IsError: value.IsError, Timestamp: value.Timestamp}, true
	}
	return nil, false
}
func newManager(t *testing.T, provider agent.Provider, configure func(*Config)) Manager {
	t.Helper()
	config := Config{SessionDir: t.TempDir(), MainSessionUUID: "owner", MainSessionFile: "/synthetic/main.jsonl", Cwd: t.TempDir(), Model: types.Model{ID: "model", Provider: "test", API: types.APIAnthropicMessages}, ThinkingEffort: types.ThinkingOff, Provider: provider, ToSessionMessage: testConverter}
	if configure != nil {
		configure(&config)
	}
	manager, err := New(config)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() {
		ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
		defer cancel()
		if err := manager.Shutdown(ctx); err != nil {
			t.Error(err)
		}
	})
	return manager
}
func spawn(t *testing.T, manager Manager, scope Scope) Info {
	t.Helper()
	info, err := manager.Spawn(context.Background(), SpawnConfig{SystemPrompt: "focused", Scope: scope}, "spawn-call")
	if err != nil {
		t.Fatal(err)
	}
	return info
}

func TestIndependentContextPersistenceAndEventIdentity(t *testing.T) {
	var events []Event
	var eventMu sync.Mutex
	var contexts []types.Context
	provider := providerFunc(func(_ context.Context, _ types.Model, input types.Context, _ types.SimpleStreamOptions, emit func(types.StreamEvent)) error {
		contexts = append(contexts, input)
		if input.SystemPrompt == nil || *input.SystemPrompt != "focused" {
			t.Error("lost independent system prompt")
		}
		finish(emit, "answer")
		return nil
	})
	manager := newManager(t, provider, func(config *Config) {
		config.Emit = func(event Event) { eventMu.Lock(); defer eventMu.Unlock(); events = append(events, event) }
	})
	info := spawn(t, manager, Inherit())
	other := spawn(t, manager, Inherit())
	for _, text := range []string{"first", "next"} {
		if err := manager.Send(context.Background(), info.ID, text, true, "send-call"); err != nil {
			t.Fatal(err)
		}
	}
	snapshot, _ := manager.Snapshot(info.ID)
	if snapshot.Info.Status != Idle || len(snapshot.Messages) != 4 || len(contexts) != 2 || len(contexts[0].Messages) != 1 || len(contexts[1].Messages) != 3 {
		t.Fatalf("transcript/context = %#v, %#v", snapshot, contexts)
	}
	untouched, _ := manager.Snapshot(other.ID)
	if len(untouched.Messages) != 0 {
		t.Fatal("context crossed subagents")
	}
	reopened, err := session.OpenManager(*info.SessionFile)
	if err != nil {
		t.Fatal(err)
	}
	if len(reopened.ContextMessages()) != 4 || reopened.Header().ParentSession == nil || *reopened.Header().ParentSession != "/synthetic/main.jsonl" {
		t.Fatal("JSONL transcript/header mismatch")
	}
	eventMu.Lock()
	defer eventMu.Unlock()
	for index, event := range events {
		if event.OwnerSessionID != "owner" || index > 0 && event.Sequence <= events[index-1].Sequence {
			t.Fatalf("lost event causality: %#v", event)
		}
		if event.Agent != nil && (event.SubagentID != info.ID || event.ToolCallID != "send-call" || event.SessionID != info.SessionID) {
			t.Fatalf("lost running event identity: %#v", event)
		}
	}
}

func TestRunningCapAbortAndWaitCancellation(t *testing.T) {
	started := make(chan struct{}, 11)
	provider := providerFunc(func(ctx context.Context, _ types.Model, _ types.Context, _ types.SimpleStreamOptions, _ func(types.StreamEvent)) error {
		started <- struct{}{}
		<-ctx.Done()
		return ctx.Err()
	})
	manager := newManager(t, provider, nil)
	var infos []Info
	for index := 0; index < 10; index++ {
		info := spawn(t, manager, Inherit())
		infos = append(infos, info)
		if err := manager.Send(context.Background(), info.ID, "work", false, "call"); err != nil {
			t.Fatal(err)
		}
		select {
		case <-started:
		case <-time.After(time.Second):
			t.Fatal("loop did not start")
		}
	}
	if _, err := manager.Spawn(context.Background(), SpawnConfig{}, "call"); err == nil {
		t.Fatal("running cap ignored")
	}
	if err := manager.Send(context.Background(), infos[0].ID, "again", false, "call"); err == nil {
		t.Fatal("parallel send accepted")
	}
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if !errors.Is(manager.Wait(ctx, infos[0].ID), context.Canceled) {
		t.Fatal("wait ignored cancellation")
	}
	if err := manager.Abort(context.Background(), infos[0].ID); err != nil {
		t.Fatal(err)
	}
	if err := manager.Send(context.Background(), infos[0].ID, "again", false, "call"); err == nil {
		t.Fatal("aborted agent resumed")
	}
	spawn(t, manager, Inherit())
}

func TestProviderAndPersistenceErrorsAreVisible(t *testing.T) {
	provider := providerFunc(func(_ context.Context, _ types.Model, _ types.Context, _ types.SimpleStreamOptions, _ func(types.StreamEvent)) error {
		return errors.New("stream disconnected")
	})
	manager := newManager(t, provider, nil)
	info := spawn(t, manager, Inherit())
	if err := manager.Send(context.Background(), info.ID, "hello", true, "call"); err != nil {
		t.Fatal(err)
	}
	snapshot, _ := manager.Snapshot(info.ID)
	if snapshot.Info.Status != Error || snapshot.Info.LastError == nil || !strings.Contains(*snapshot.Info.LastError, "stream disconnected") {
		t.Fatalf("error hidden: %#v", snapshot.Info)
	}
	if err := os.Rename(*info.SessionFile, *info.SessionFile+".retained"); err != nil {
		t.Fatal(err)
	}
	if err := manager.Send(context.Background(), info.ID, "missing file", true, "call"); err == nil {
		t.Fatal("failed persistence reported success")
	}
}

func TestSpawnDoesNotOverwriteExistingTranscript(t *testing.T) {
	provider := providerFunc(func(_ context.Context, _ types.Model, _ types.Context, _ types.SimpleStreamOptions, emit func(types.StreamEvent)) error {
		finish(emit, "saved")
		return nil
	})
	dir := t.TempDir()
	first := newManager(t, provider, func(config *Config) { config.SessionDir = dir })
	info := spawn(t, first, Inherit())
	if err := first.Send(context.Background(), info.ID, "keep", true, "call"); err != nil {
		t.Fatal(err)
	}
	before, _ := os.ReadFile(*info.SessionFile)
	second := newManager(t, provider, func(config *Config) { config.SessionDir = dir })
	created := spawn(t, second, Inherit())
	after, _ := os.ReadFile(*info.SessionFile)
	if string(before) != string(after) || created.ID == info.ID {
		t.Fatal("new manager overwrote existing transcript")
	}
}

type inertTool struct {
	name    string
	execute func()
}

func (tool inertTool) Name() string                                    { return tool.name }
func (inertTool) Label() string                                        { return "test" }
func (inertTool) Description() string                                  { return "test" }
func (inertTool) ParametersSchema() json.RawMessage                    { return json.RawMessage(`{"type":"object"}`) }
func (inertTool) PrepareArguments(raw json.RawMessage) json.RawMessage { return raw }
func (inertTool) ExecutionMode() *tools.ExecutionMode                  { return nil }
func (tool inertTool) Execute(context.Context, string, json.RawMessage, func(tools.Result)) (tools.Result, error) {
	if tool.execute != nil {
		tool.execute()
	}
	return tools.Result{}, nil
}

func TestScopeBeforePermissionAndDisabledRecursiveTools(t *testing.T) {
	permissionCalls, executions := 0, 0
	provider := providerFunc(func(_ context.Context, _ types.Model, input types.Context, _ types.SimpleStreamOptions, emit func(types.StreamEvent)) error {
		if len(input.Tools) != 1 || input.Tools[0].Name != "read" {
			t.Error("scope/disabled/recursive tool filtering failed")
		}
		if _, ok := input.Messages[len(input.Messages)-1].(types.UserMessage); ok {
			emit(types.Done{Reason: types.ToolUse, Message: types.AssistantMessage{StopReason: types.ToolUse, Content: []types.ContentBlock{types.ToolCallBlock{ID: "read-call", Name: "read", Arguments: json.RawMessage(`{"path":"/outside/secret"}`)}}}})
		} else {
			if result := input.Messages[len(input.Messages)-1].(types.ToolResultMessage); !result.IsError {
				t.Error("scope violation not returned to model")
			}
			finish(emit, "denied")
		}
		return nil
	})
	manager := newManager(t, provider, func(config *Config) {
		config.DisabledTools = map[string]bool{"write": true}
		config.Tools = func(string) []tools.Tool {
			return []tools.Tool{inertTool{name: "read", execute: func() { executions++ }}, inertTool{name: "write"}, inertTool{name: "subagent"}}
		}
		config.PermissionHook = func(id string) hooks.PreToolUse {
			if id == "owner" {
				t.Error("permission identity reused")
			}
			return func(context.Context, hooks.PreToolUseContext) (*hooks.PreToolUseResult, error) {
				permissionCalls++
				return nil, nil
			}
		}
	})
	info := spawn(t, manager, Scope{Paths: []string{"src"}})
	if err := manager.Send(context.Background(), info.ID, "read", true, "call"); err != nil {
		t.Fatal(err)
	}
	if permissionCalls != 0 || executions != 0 {
		t.Fatal("scope check ran after permission/execution")
	}
}

func TestScopePathsCommandsAndSkills(t *testing.T) {
	root := t.TempDir()
	allowed := filepath.Join(root, "src")
	if err := os.Mkdir(allowed, 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(t.TempDir(), filepath.Join(allowed, "link")); err != nil {
		t.Fatal(err)
	}
	for _, path := range []string{"src/file", filepath.Join(allowed, "missing", "new")} {
		raw, _ := json.Marshal(map[string]string{"path": path})
		if err := Scoped([]string{"src"}).Check("write", raw, root); err != nil {
			t.Error(err)
		}
	}
	for _, path := range []string{"src/../secret", "src-other/file", "src/link/new"} {
		raw, _ := json.Marshal(map[string]string{"path": path})
		if err := Scoped([]string{"src"}).Check("read", raw, root); err == nil {
			t.Errorf("scope allowed %s", path)
		}
	}
	if err := Readonly().Check("write", json.RawMessage(`{}`), root); err == nil {
		t.Fatal("readonly allowed write")
	}
	if err := Readonly().Check("bash", json.RawMessage(`{"command":"cat file"}`), root); err != nil {
		t.Error(err)
	}
	if err := Readonly().Check("bash", json.RawMessage(`{"command":"touch file"}`), root); err == nil {
		t.Fatal("readonly allowed touch")
	}
	if err := (Scope{Skills: []string{"search"}}).Check("skill", json.RawMessage(`{"skill":"other"}`), root); err == nil {
		t.Fatal("skill whitelist ignored")
	}
}

type processTool struct {
	inertTool
	command, cwd string
	finished     chan error
}

func (tool processTool) Execute(ctx context.Context, _ string, _ json.RawMessage, _ func(tools.Result)) (tools.Result, error) {
	_, err := bash.Execute(ctx, tool.command, tool.cwd, time.Minute)
	tool.finished <- err
	return tools.Result{}, err
}

func TestAbortWaitsForRunningBashProcessCleanup(t *testing.T) {
	if _, err := exec.LookPath("bash"); err != nil {
		t.Skip("bash is unavailable")
	}
	dir := t.TempDir()
	marker := filepath.Join(dir, "started")
	finished := make(chan error, 1)
	provider := providerFunc(func(_ context.Context, _ types.Model, input types.Context, _ types.SimpleStreamOptions, emit func(types.StreamEvent)) error {
		if _, user := input.Messages[len(input.Messages)-1].(types.UserMessage); user {
			emit(types.Done{Reason: types.ToolUse, Message: types.AssistantMessage{StopReason: types.ToolUse, Content: []types.ContentBlock{types.ToolCallBlock{ID: "process", Name: "bash", Arguments: json.RawMessage(`{"command":"sleep"}`)}}}})
		} else {
			finish(emit, "finished")
		}
		return nil
	})
	manager := newManager(t, provider, func(config *Config) {
		config.Tools = func(string) []tools.Tool {
			return []tools.Tool{processTool{inertTool: inertTool{name: "bash"}, command: fmt.Sprintf("printf ready > '%s'; sleep 30", strings.ReplaceAll(marker, "'", "'\"'\"'")), cwd: dir, finished: finished}}
		}
	})
	info := spawn(t, manager, Inherit())
	if err := manager.Send(context.Background(), info.ID, "run", false, "process-call"); err != nil {
		t.Fatal(err)
	}
	deadline := time.After(2 * time.Second)
	for {
		if _, err := os.Stat(marker); err == nil {
			break
		}
		select {
		case <-deadline:
			t.Fatal("bash did not start")
		case <-time.After(5 * time.Millisecond):
		}
	}
	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
	defer cancel()
	if err := manager.Abort(ctx, info.ID); err != nil {
		t.Fatal(err)
	}
	select {
	case err := <-finished:
		if err == nil {
			t.Fatal("bash finished without cancellation")
		}
	default:
		t.Fatal("abort returned before process cleanup")
	}
}
