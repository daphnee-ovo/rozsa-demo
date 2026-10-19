package runtime

import (
	"context"
	"encoding/json"
	"errors"
	"io"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"rozsa/backend/internal/app/permissions"
	"rozsa/backend/internal/app/session"
	"rozsa/backend/internal/app/settings"
	"rozsa/backend/internal/model/types"
)

type toolLoopProvider struct {
	name      string
	arguments func(types.Context) json.RawMessage
	calls     chan types.Context
}

func newToolLoopProvider(name string, arguments func(types.Context) json.RawMessage) *toolLoopProvider {
	return &toolLoopProvider{name: name, arguments: arguments, calls: make(chan types.Context, 16)}
}

func (provider *toolLoopProvider) StreamSimple(ctx context.Context, model types.Model, input types.Context, _ types.SimpleStreamOptions, emit func(types.StreamEvent)) error {
	if err := ctx.Err(); err != nil {
		return err
	}
	provider.calls <- input
	var message types.AssistantMessage
	message.API, message.Provider, message.Model = model.API, model.Provider, model.ID
	if len(input.Messages) > 0 {
		if _, isToolResult := input.Messages[len(input.Messages)-1].(types.ToolResultMessage); isToolResult {
			message.Content = []types.ContentBlock{types.TextBlock{Text: "tool flow complete"}}
			message.StopReason = types.Stop
			emit(types.Done{Reason: types.Stop, Message: message})
			return nil
		}
	}
	arguments := json.RawMessage(`{}`)
	if provider.arguments != nil {
		arguments = provider.arguments(input)
	}
	message.Content = []types.ContentBlock{types.ToolCallBlock{ID: "same-tool-call", Name: provider.name, Arguments: arguments}}
	message.StopReason = types.ToolUse
	emit(types.Done{Reason: types.ToolUse, Message: message})
	return nil
}

func newProductionRuntime(t *testing.T, provider *toolLoopProvider, workspace string, resolvedSettings func() map[string]any, store permissions.ProjectRuleStore) *Runtime {
	t.Helper()
	app, err := New(Config{
		SessionDir: t.TempDir(), Model: types.Model{ID: "fixture-model", Provider: "fixture", API: types.APIAnthropicMessages, ContextWindow: 128000},
		Provider: provider, Output: io.Discard, ResolvedSettings: resolvedSettings, ProjectRuleStore: store,
	})
	if err != nil {
		t.Fatal(err)
	}
	if err := app.session.SetCwd(workspace); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() {
		ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
		defer cancel()
		if err := app.Shutdown(ctx); err != nil {
			t.Errorf("shutdown runtime: %v", err)
		}
	})
	return app
}

func yoloSettings() func() map[string]any {
	return func() map[string]any {
		return map[string]any{"permission": map[string]any{"mode": "yolo"}}
	}
}

func waitForApproval(t *testing.T, app *Runtime, sessionID string) permissions.Request {
	t.Helper()
	deadline := time.After(3 * time.Second)
	for {
		requests := app.PendingApprovals(sessionID)
		for _, request := range requests {
			if request.Status == permissions.Pending || request.Status == permissions.Failed {
				return request
			}
		}
		select {
		case <-deadline:
			t.Fatalf("approval was not submitted for session %s", sessionID)
		case <-time.After(5 * time.Millisecond):
		}
	}
}

func waitForTerminal(t *testing.T, app *Runtime, operationID string) MessageEvent {
	t.Helper()
	subscription, err := app.SubscribeMessageEvents(context.Background(), operationID)
	if err != nil {
		t.Fatal(err)
	}
	defer subscription.Close()
	ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
	defer cancel()
	for {
		event, err := subscription.Next(ctx)
		if err != nil {
			t.Fatalf("wait for operation terminal: %v", err)
		}
		switch event.Kind {
		case MessageCompleted, MessageFailed, MessageCanceled:
			return event
		}
	}
}

func persistedToolResult(t *testing.T, messages []session.Message) session.ToolResultMessage {
	t.Helper()
	for _, message := range messages {
		if result, ok := message.(session.ToolResultMessage); ok {
			return result
		}
	}
	t.Fatal("session JSONL has no tool result")
	return session.ToolResultMessage{}
}

func persistedText(result session.ToolResultMessage) string {
	var text strings.Builder
	for _, block := range result.Content {
		if block, ok := block.(session.TextBlock); ok {
			text.WriteString(block.Text)
		}
	}
	return text.String()
}

func assertProductionTools(t *testing.T, schemas []types.ToolSchema) {
	t.Helper()
	found := map[string]bool{}
	for _, schema := range schemas {
		found[schema.Name] = true
	}
	for _, name := range []string{"read", "write", "bash"} {
		if !found[name] {
			t.Errorf("provider tool metadata omitted %q: %+v", name, schemas)
		}
	}
}

func TestRuntimeRegistersProductionToolsAndPersistsResults(t *testing.T) {
	for _, name := range []string{"read", "write", "bash"} {
		t.Run(name, func(t *testing.T) {
			workspace := t.TempDir()
			provider := newToolLoopProvider(name, func(types.Context) json.RawMessage {
				switch name {
				case "read":
					path := filepath.Join(workspace, "input.txt")
					if err := os.WriteFile(path, []byte("read fixture\n"), 0o600); err != nil {
						t.Fatal(err)
					}
					data, _ := json.Marshal(map[string]any{"file_path": path})
					return data
				case "write":
					data, _ := json.Marshal(map[string]any{"file_path": filepath.Join(workspace, "output.txt"), "content": "write fixture"})
					return data
				default:
					nextDirectory := filepath.Join(workspace, "next")
					if err := os.Mkdir(nextDirectory, 0o700); err != nil {
						t.Fatal(err)
					}
					command := "printf 'bash fixture' > " + filepath.Join(workspace, "bash.txt") + "; cd " + nextDirectory
					data, _ := json.Marshal(map[string]any{"command": command})
					return data
				}
			})
			app := newProductionRuntime(t, provider, workspace, yoloSettings(), nil)
			if err := app.Prompt(t.Context(), "run production tool"); err != nil {
				t.Fatal(err)
			}
			firstCall := <-provider.calls
			assertProductionTools(t, firstCall.Tools)
			result := persistedToolResult(t, app.session.ContextMessages())
			if len(result.Details) == 0 || string(result.Details) == "null" {
				t.Fatalf("tool result details were not persisted: %s", result.Details)
			}
			reopened, err := session.Open(app.session.Path())
			if err != nil {
				t.Fatal(err)
			}
			persisted := persistedToolResult(t, reopened.ContextMessages())
			if persisted.ToolName != name || persisted.ToolCallID != "same-tool-call" {
				t.Fatalf("persisted tool identity = %s/%s", persisted.ToolName, persisted.ToolCallID)
			}
			switch name {
			case "read":
				if !strings.Contains(persistedText(persisted), "read fixture") {
					t.Fatalf("Read result = %q", persistedText(persisted))
				}
			case "write":
				contents, err := os.ReadFile(filepath.Join(workspace, "output.txt"))
				if err != nil || string(contents) != "write fixture" {
					t.Fatalf("Write side effect = %q, %v", contents, err)
				}
			case "bash":
				contents, err := os.ReadFile(filepath.Join(workspace, "bash.txt"))
				if err != nil || string(contents) != "bash fixture" {
					t.Fatalf("Bash side effect = %q, %v", contents, err)
				}
				wantCWD := filepath.Join(workspace, "next")
				if app.session.Cwd() != wantCWD {
					t.Fatalf("Bash cwd = %q, want %q", app.session.Cwd(), wantCWD)
				}
				if reopened.Header().Cwd != wantCWD {
					t.Fatalf("persisted Bash cwd = %q, want %q", reopened.Header().Cwd, wantCWD)
				}
			}
		})
	}
}

func TestApprovalWaitsBeforeWriteAndAppliesSessionTrust(t *testing.T) {
	workspace := t.TempDir()
	path := filepath.Join(workspace, "approved.txt")
	provider := newToolLoopProvider("write", func(types.Context) json.RawMessage {
		data, _ := json.Marshal(map[string]any{"file_path": path, "content": "approved"})
		return data
	})
	app := newProductionRuntime(t, provider, workspace, nil, nil)
	operationID, err := app.StartMessage(t.Context(), "write once")
	if err != nil {
		t.Fatal(err)
	}
	request := waitForApproval(t, app, app.session.SessionID())
	if _, err := os.Stat(path); !errors.Is(err, os.ErrNotExist) {
		t.Fatalf("Write ran before approval: stat err = %v", err)
	}
	if len(request.Info.TrustLevels) == 0 {
		t.Fatalf("approval did not expose trust scopes: %+v", request.Info)
	}
	if err := app.ApprovalOwner().RespondApproval(t.Context(), request.Identity, permissions.Response{Kind: permissions.AllowSession, TrustKey: "write:/outside/scope"}); err == nil {
		t.Fatal("invalid trust scope was accepted")
	}
	if current, exists := app.approvals.Get(request.Identity); !exists || current.Status != permissions.Pending {
		t.Fatalf("invalid trust scope changed the pending request: %+v %v", current, exists)
	}
	response := permissions.Response{Kind: permissions.AllowSession, TrustKey: request.Info.TrustLevels[0].Key}
	if err := app.ApprovalOwner().RespondApproval(t.Context(), request.Identity, response); err != nil {
		t.Fatal(err)
	}
	if terminal := waitForTerminal(t, app, operationID); terminal.Kind != MessageCompleted {
		t.Fatalf("approved turn = %+v", terminal)
	}
	contents, err := os.ReadFile(path)
	if err != nil || string(contents) != "approved" {
		t.Fatalf("approved Write = %q, %v", contents, err)
	}
	if _, exists := app.approvals.Get(request.Identity); exists {
		t.Fatal("successful approval remained pending")
	}
	arguments := map[string]any{"file_path": path}
	if verdict := app.permissionRuntime.policies[request.Identity.SessionID].Evaluate(request.Identity.SessionID, "write", arguments); verdict.Kind != permissions.Allow {
		t.Fatalf("session trust was not applied: verdict=%+v approval=%+v", verdict, request.Info)
	}
	secondContext, cancel := context.WithTimeout(t.Context(), 2*time.Second)
	defer cancel()
	if err := app.Prompt(secondContext, "reuse session trust"); err != nil {
		t.Fatal(err)
	}
	if requests := app.PendingApprovals(app.session.SessionID()); len(requests) != 0 {
		t.Fatalf("session trust did not cover the same request: %+v", requests)
	}
}

func TestProjectTrustPersistsThroughLayeredSettings(t *testing.T) {
	workspace := t.TempDir()
	path := filepath.Join(workspace, "project-trusted.txt")
	settingsRoot := t.TempDir()
	globalPath := filepath.Join(settingsRoot, "global.json")
	projectPath := filepath.Join(settingsRoot, "project", "settings.json")
	if err := os.MkdirAll(filepath.Dir(projectPath), 0o700); err != nil {
		t.Fatal(err)
	}
	settingsManager, err := settings.Load(globalPath, &projectPath, nil)
	if err != nil {
		t.Fatal(err)
	}
	store, err := permissions.NewProjectRuleStore(settingsManager)
	if err != nil {
		t.Fatal(err)
	}
	provider := newToolLoopProvider("write", func(types.Context) json.RawMessage {
		data, _ := json.Marshal(map[string]any{"file_path": path, "content": "trusted"})
		return data
	})
	app := newProductionRuntime(t, provider, workspace, settingsManager.Resolved, store)
	operationID, err := app.StartMessage(t.Context(), "write with project trust")
	if err != nil {
		t.Fatal(err)
	}
	request := waitForApproval(t, app, app.session.SessionID())
	trustKey := request.Info.TrustLevels[0].Key
	if err := app.ApprovalOwner().RespondApproval(t.Context(), request.Identity, permissions.Response{Kind: permissions.AllowProject, TrustKey: trustKey}); err != nil {
		t.Fatal(err)
	}
	if terminal := waitForTerminal(t, app, operationID); terminal.Kind != MessageCompleted {
		message := ""
		if terminal.Error != nil {
			message = *terminal.Error
		}
		t.Fatalf("project-trusted turn = %+v: %s", terminal, message)
	}
	rule, err := permissions.ProjectRuleForTrustKey(trustKey)
	if err != nil {
		t.Fatal(err)
	}
	permissionSettings := settingsManager.Resolved()["permission"].(map[string]any)
	allowRules, ok := settingStrings(permissionSettings["allow"])
	if !ok || !containsString(allowRules, rule) {
		t.Fatalf("project approval was not persisted in settings: %v", permissionSettings["allow"])
	}
	arguments := map[string]any{"file_path": path}
	if verdict := app.permissionRuntime.policies[request.Identity.SessionID].Evaluate(request.Identity.SessionID, "write", arguments); verdict.Kind != permissions.Allow {
		t.Fatalf("project trust was not applied: verdict=%+v approval=%+v", verdict, request.Info)
	}
	secondContext, cancel := context.WithTimeout(t.Context(), 2*time.Second)
	defer cancel()
	if err := app.Prompt(secondContext, "reuse project trust"); err != nil {
		t.Fatal(err)
	}
	if requests := app.PendingApprovals(app.session.SessionID()); len(requests) != 0 {
		t.Fatalf("project trust did not cover the same request: %+v", requests)
	}
}

func TestDeniedHintCancellationAndFailedResponseRemainSafe(t *testing.T) {
	t.Run("deny does not write", func(t *testing.T) {
		workspace := t.TempDir()
		path := filepath.Join(workspace, "denied.txt")
		provider := newToolLoopProvider("write", func(types.Context) json.RawMessage {
			data, _ := json.Marshal(map[string]any{"file_path": path, "content": "must not exist"})
			return data
		})
		app := newProductionRuntime(t, provider, workspace, nil, nil)
		operationID, err := app.StartMessage(t.Context(), "request denial")
		if err != nil {
			t.Fatal(err)
		}
		request := waitForApproval(t, app, app.session.SessionID())
		if err := app.ApprovalOwner().RespondApproval(t.Context(), request.Identity, permissions.Response{Kind: permissions.Deny}); err != nil {
			t.Fatal(err)
		}
		if terminal := waitForTerminal(t, app, operationID); terminal.Kind != MessageCompleted {
			t.Fatalf("denied turn = %+v", terminal)
		}
		if _, err := os.Stat(path); !errors.Is(err, os.ErrNotExist) {
			t.Fatalf("denied Write changed the filesystem: %v", err)
		}
		result := persistedToolResult(t, app.session.ContextMessages())
		if !result.IsError || !strings.Contains(persistedText(result), "Permission denied by user") {
			t.Fatalf("denial was not returned to the model: %+v", result)
		}
	})

	t.Run("denied hint does not write", func(t *testing.T) {
		workspace := t.TempDir()
		path := filepath.Join(workspace, "denied.txt")
		provider := newToolLoopProvider("write", func(types.Context) json.RawMessage {
			data, _ := json.Marshal(map[string]any{"file_path": path, "content": "must not exist"})
			return data
		})
		app := newProductionRuntime(t, provider, workspace, nil, nil)
		operationID, err := app.StartMessage(t.Context(), "request denial")
		if err != nil {
			t.Fatal(err)
		}
		request := waitForApproval(t, app, app.session.SessionID())
		response := permissions.Response{Kind: permissions.DenyWithHint, Hint: "inspect the target first"}
		if err := app.ApprovalOwner().RespondApproval(t.Context(), request.Identity, response); err != nil {
			t.Fatal(err)
		}
		if terminal := waitForTerminal(t, app, operationID); terminal.Kind != MessageCompleted {
			t.Fatalf("denied turn = %+v", terminal)
		}
		if _, err := os.Stat(path); !errors.Is(err, os.ErrNotExist) {
			t.Fatalf("denied Write changed the filesystem: %v", err)
		}
		result := persistedToolResult(t, app.session.ContextMessages())
		if !result.IsError || !strings.Contains(persistedText(result), "Hint: inspect the target first") {
			t.Fatalf("denial was not returned to the model: %+v", result)
		}
	})

	t.Run("cancel closes the waiter", func(t *testing.T) {
		workspace := t.TempDir()
		path := filepath.Join(workspace, "cancelled.txt")
		provider := newToolLoopProvider("write", func(types.Context) json.RawMessage {
			data, _ := json.Marshal(map[string]any{"file_path": path, "content": "must not exist"})
			return data
		})
		app := newProductionRuntime(t, provider, workspace, nil, nil)
		operationID, err := app.StartMessage(t.Context(), "cancel pending approval")
		if err != nil {
			t.Fatal(err)
		}
		request := waitForApproval(t, app, app.session.SessionID())
		if err := app.CancelMessage(t.Context(), operationID); err != nil {
			t.Fatal(err)
		}
		if terminal := waitForTerminal(t, app, operationID); terminal.Kind != MessageCanceled {
			t.Fatalf("cancelled turn = %+v", terminal)
		}
		if _, err := os.Stat(path); !errors.Is(err, os.ErrNotExist) {
			t.Fatalf("cancelled Write changed the filesystem: %v", err)
		}
		if err := app.ApprovalOwner().RespondApproval(t.Context(), request.Identity, permissions.Response{Kind: permissions.AllowOnce}); err == nil {
			t.Fatal("late response to cancelled approval was accepted")
		}
	})

	t.Run("failed project persistence remains retryable", func(t *testing.T) {
		workspace := t.TempDir()
		path := filepath.Join(workspace, "retry.txt")
		storeError := errors.New("settings disk unavailable")
		provider := newToolLoopProvider("write", func(types.Context) json.RawMessage {
			data, _ := json.Marshal(map[string]any{"file_path": path, "content": "retry succeeded"})
			return data
		})
		app := newProductionRuntime(t, provider, workspace, nil, failingProjectRuleStore{err: storeError})
		operationID, err := app.StartMessage(t.Context(), "retry after settings failure")
		if err != nil {
			t.Fatal(err)
		}
		request := waitForApproval(t, app, app.session.SessionID())
		trustKey := request.Info.TrustLevels[0].Key
		if err := app.ApprovalOwner().RespondApproval(t.Context(), request.Identity, permissions.Response{Kind: permissions.AllowProject, TrustKey: trustKey}); !errors.Is(err, storeError) {
			t.Fatalf("project persistence error = %v", err)
		}
		retryRequest := waitForApproval(t, app, app.session.SessionID())
		if retryRequest.Status != permissions.Failed {
			t.Fatalf("failed response status = %d", retryRequest.Status)
		}
		if _, err := os.Stat(path); !errors.Is(err, os.ErrNotExist) {
			t.Fatalf("failed response allowed Write: %v", err)
		}
		if err := app.ApprovalOwner().RespondApproval(t.Context(), request.Identity, permissions.Response{Kind: permissions.AllowOnce}); err != nil {
			t.Fatal(err)
		}
		if terminal := waitForTerminal(t, app, operationID); terminal.Kind != MessageCompleted {
			t.Fatalf("retried turn = %+v", terminal)
		}
		contents, err := os.ReadFile(path)
		if err != nil || string(contents) != "retry succeeded" {
			t.Fatalf("retried Write = %q, %v", contents, err)
		}
	})
}

type failingProjectRuleStore struct{ err error }

func (store failingProjectRuleStore) AddProjectPermissionAllow(string) error { return store.err }

func containsString(values []string, wanted string) bool {
	for _, value := range values {
		if value == wanted {
			return true
		}
	}
	return false
}
