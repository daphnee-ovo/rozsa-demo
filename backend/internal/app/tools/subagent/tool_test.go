package subagent_test

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	runtime "runtime"
	"strings"
	"testing"

	"rozsa/backend/internal/app/session"
	app "rozsa/backend/internal/app/subagent"
	subagenttool "rozsa/backend/internal/app/tools/subagent"
	"rozsa/backend/internal/core/hooks"
	"rozsa/backend/internal/core/tools"
	"rozsa/backend/internal/model/types"
)

type provider struct{}

func (provider) StreamSimple(_ context.Context, _ types.Model, _ types.Context, _ types.SimpleStreamOptions, emit func(types.StreamEvent)) error {
	emit(types.Done{Reason: types.Stop, Message: types.AssistantMessage{Content: []types.ContentBlock{types.TextBlock{Text: "actual response"}}, StopReason: types.Stop}})
	return nil
}

type scopeProvider struct {
	path      string
	toolError bool
}

func (provider *scopeProvider) StreamSimple(_ context.Context, _ types.Model, input types.Context, _ types.SimpleStreamOptions, emit func(types.StreamEvent)) error {
	if _, isUser := input.Messages[len(input.Messages)-1].(types.UserMessage); isUser {
		emit(types.Done{Reason: types.ToolUse, Message: types.AssistantMessage{StopReason: types.ToolUse, Content: []types.ContentBlock{
			types.ToolCallBlock{ID: "read-call", Name: "read", Arguments: mustJSON(map[string]string{"path": provider.path})},
		}}})
		return nil
	}
	if result, ok := input.Messages[len(input.Messages)-1].(types.ToolResultMessage); ok {
		provider.toolError = result.IsError
	}
	emit(types.Done{Reason: types.Stop, Message: types.AssistantMessage{StopReason: types.Stop, Content: []types.ContentBlock{types.TextBlock{Text: "finished"}}}})
	return nil
}

type scopeReadTool struct{ executions *int }

func (tool scopeReadTool) Name() string                                    { return "read" }
func (scopeReadTool) Label() string                                        { return "Read" }
func (scopeReadTool) Description() string                                  { return "Read a test path" }
func (scopeReadTool) ParametersSchema() json.RawMessage                    { return json.RawMessage(`{"type":"object"}`) }
func (scopeReadTool) PrepareArguments(raw json.RawMessage) json.RawMessage { return raw }
func (scopeReadTool) ExecutionMode() *tools.ExecutionMode                  { return nil }
func (tool scopeReadTool) Execute(context.Context, string, json.RawMessage, func(tools.Result)) (tools.Result, error) {
	(*tool.executions)++
	return tools.Result{Content: []types.ContentBlock{types.TextBlock{Text: "read completed"}}}, nil
}

func mustJSON(value any) json.RawMessage {
	encoded, err := json.Marshal(value)
	if err != nil {
		panic(err)
	}
	return encoded
}

func projectWorkingDirectory(t *testing.T) string {
	t.Helper()
	_, sourcePath, _, ok := runtime.Caller(0)
	if !ok {
		t.Fatal("could not locate test source")
	}
	repositoryRoot := filepath.Clean(filepath.Join(filepath.Dir(sourcePath), "../../../../.."))
	target := filepath.Join(repositoryRoot, "testdata", "project-test-dirs", "subagent-tool")
	if err := os.MkdirAll(target, 0o700); err != nil {
		t.Fatal(err)
	}
	workingDirectory, err := os.MkdirTemp(target, "run-")
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() {
		if err := os.RemoveAll(workingDirectory); err != nil {
			t.Error(err)
		}
	})
	return workingDirectory
}

func convertSessionMessage(message hooks.Message) (session.Message, bool) {
	switch value := message.Standard.(type) {
	case types.UserMessage:
		return session.UserMessage{Content: session.UserText(value.Content.(types.UserText)), Timestamp: value.Timestamp}, true
	case types.AssistantMessage:
		blocks := make([]session.ContentBlock, 0, len(value.Content))
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

func TestToolActionsRunActualManager(t *testing.T) {
	workingDirectory := projectWorkingDirectory(t)
	manager, err := app.New(app.Config{MainSessionUUID: "main", MainSessionFile: filepath.Join(workingDirectory, "main.jsonl"), Cwd: workingDirectory, Provider: provider{}, ToSessionMessage: func(hooks.Message) (session.Message, bool) { return nil, false }})
	if err != nil {
		t.Fatal(err)
	}
	defer manager.Shutdown(context.Background())
	tool := subagenttool.New(manager)
	result, err := tool.Execute(context.Background(), "create", json.RawMessage(`{"action":"spawn","system_prompt":"focused","prompt":"work","wait":true}`), nil)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(result.Content[0].(types.TextBlock).Text, "actual response") {
		t.Fatal("spawn returned a static card")
	}
	var info struct {
		ID     string `json:"id"`
		Status string `json:"status"`
	}
	if err := json.Unmarshal(result.Details, &info); err != nil {
		t.Fatal(err)
	}
	if info.ID == "" || info.Status != "idle" {
		t.Fatalf("result identity/status = %#v", info)
	}
	for _, action := range []string{"send", "wait", "list", "interrupt"} {
		raw, _ := json.Marshal(map[string]any{"action": action, "id": info.ID, "prompt": "more", "wait": true})
		if _, err := tool.Execute(context.Background(), "next", raw, nil); err != nil {
			t.Fatal(action, err)
		}
	}
	raw, _ := json.Marshal(map[string]any{"action": "send", "id": info.ID, "prompt": "resume"})
	if _, err := tool.Execute(context.Background(), "late", raw, nil); err == nil {
		t.Fatal("aborted agent resumed")
	}
	for _, raw := range []string{`{"action":"spawn"}`, `{"action":"send","id":"x"}`, `{"action":"wait"}`, `{"action":"unknown"}`, `{"action":"spawn","system_prompt":"x","scope":"invalid"}`, `{"action":"spawn","system_prompt":"x","scope":{"type":"custom","paths":[3]}}`} {
		if _, err := tool.Execute(context.Background(), "invalid", json.RawMessage(raw), nil); err == nil {
			t.Fatalf("accepted %s", raw)
		}
	}
}

func TestScopeParsingPreservesEmptyRestrictions(t *testing.T) {
	for _, test := range []struct {
		name, scope, path string
		wantDenied        bool
	}{
		{name: "empty tool set", scope: `{"type":"custom","tools":[]}`, path: "allowed/file", wantDenied: true},
		{name: "empty path set", scope: `{"type":"scoped","paths":[]}`, path: "allowed/file", wantDenied: true},
		{name: "inherit allows operation", scope: `"inherit"`, path: "allowed/file", wantDenied: false},
	} {
		t.Run(test.name, func(t *testing.T) {
			workingDirectory := projectWorkingDirectory(t)
			provider := &scopeProvider{path: test.path}
			executions := 0
			manager, err := app.New(app.Config{
				SessionDir: t.TempDir(), MainSessionUUID: "main", MainSessionFile: filepath.Join(workingDirectory, "main.jsonl"), Cwd: workingDirectory,
				Provider: provider,
				Tools: func(string, *session.Manager) []tools.Tool {
					return []tools.Tool{scopeReadTool{executions: &executions}}
				},
				ToSessionMessage: convertSessionMessage,
			})
			if err != nil {
				t.Fatal(err)
			}
			defer manager.Shutdown(context.Background())
			tool := subagenttool.New(manager)
			arguments := json.RawMessage(`{"action":"spawn","system_prompt":"test scope","prompt":"read path","wait":true,"scope":` + test.scope + `}`)
			if _, err := tool.Execute(context.Background(), "scope-call", arguments, nil); err != nil {
				t.Fatal(err)
			}
			if test.wantDenied {
				if executions != 0 || !provider.toolError {
					t.Fatalf("empty restriction did not deny operation: executions=%d toolError=%v", executions, provider.toolError)
				}
			} else if executions != 1 || provider.toolError {
				t.Fatalf("allowed control did not execute: executions=%d toolError=%v", executions, provider.toolError)
			}
		})
	}
}
