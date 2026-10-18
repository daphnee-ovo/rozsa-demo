package runtime

import (
	"context"
	"encoding/json"
	"strings"
	"sync"
	"testing"
	"time"

	"rozsa/backend/internal/app/questions"
	"rozsa/backend/internal/app/session"
	"rozsa/backend/internal/app/subagent"
	"rozsa/backend/internal/core/tools"
	"rozsa/backend/internal/model/types"
)

type subagentProvider struct {
	mu         sync.Mutex
	childCalls int
}

func (provider *subagentProvider) StreamSimple(_ context.Context, model types.Model, input types.Context, _ types.SimpleStreamOptions, emit func(types.StreamEvent)) error {
	child := input.SystemPrompt != nil && *input.SystemPrompt == "child"
	last := input.Messages[len(input.Messages)-1]
	if _, user := last.(types.UserMessage); user {
		var call types.ToolCallBlock
		if child {
			provider.mu.Lock()
			provider.childCalls++
			provider.mu.Unlock()
			call = types.ToolCallBlock{ID: "child-question", Name: askUserQuestionName, Arguments: json.RawMessage(questionArguments)}
		} else {
			call = types.ToolCallBlock{ID: "main-spawn", Name: "subagent", Arguments: json.RawMessage(`{"action":"spawn","system_prompt":"child","prompt":"ask independently","wait":true}`)}
		}
		emit(types.Done{Reason: types.ToolUse, Message: types.AssistantMessage{API: model.API, Provider: model.Provider, Model: model.ID, StopReason: types.ToolUse, Content: []types.ContentBlock{call}}})
		return nil
	}
	text := "main complete"
	if child {
		text = "child complete"
	}
	emit(types.Done{Reason: types.Stop, Message: types.AssistantMessage{API: model.API, Provider: model.Provider, Model: model.ID, StopReason: types.Stop, Content: []types.ContentBlock{types.TextBlock{Text: text}}}})
	return nil
}

func TestProductionSubagentExecutesAndPersistsWithSeparateQuestionIdentity(t *testing.T) {
	provider := &subagentProvider{}
	app := newTestRuntime(t, provider, nil)
	defer app.Shutdown(context.Background())
	mainID := session.IDFromPath(app.session.Path())
	finished := make(chan error, 1)
	go func() { finished <- app.Prompt(context.Background(), "delegate") }()
	request, ok := waitForQuestion(app.questions)
	if !ok {
		t.Fatal("real subagent did not execute question tool")
	}
	manager, err := app.SubagentManager(mainID)
	if err != nil {
		t.Fatal(err)
	}
	infos := manager.List()
	if len(infos) != 1 || request.SessionID != infos[0].SessionID || request.SessionID == mainID || request.TransportID != "child-question" {
		t.Fatalf("question identity = %#v, subagents %#v", request, infos)
	}
	answers := questions.Answers{"Mode": questions.Single("Fast"), "Features": questions.Multiple("Lint")}
	if err := app.questions.Respond(request.RequestID, answers); err != nil {
		t.Fatal(err)
	}
	select {
	case err := <-finished:
		if err != nil {
			t.Fatal(err)
		}
	case <-time.After(2 * time.Second):
		t.Fatal("subagent tool did not finish")
	}
	var result *session.ToolResultMessage
	for _, message := range app.session.ContextMessages() {
		if message, ok := message.(session.ToolResultMessage); ok && message.ToolName == "subagent" {
			copy := message
			result = &copy
		}
	}
	if result == nil || result.ToolCallID != "main-spawn" || result.IsError || !strings.Contains(string(result.Details), `"status":"idle"`) {
		t.Fatalf("main result lost call correlation: %#v", result)
	}
	child, err := session.OpenManager(*infos[0].SessionFile)
	if err != nil {
		t.Fatal(err)
	}
	if len(child.ContextMessages()) != 4 || child.Header().ParentSession == nil || *child.Header().ParentSession != app.session.Path() {
		t.Fatal("child transcript/header not persisted independently")
	}
	for _, message := range app.session.ContextMessages() {
		if result, ok := message.(session.ToolResultMessage); ok && result.ToolName == askUserQuestionName {
			t.Fatal("child transcript leaked into main context")
		}
	}
	events := app.SubagentEvents(mainID, 0)
	found := false
	for _, event := range events {
		if event.SubagentID == infos[0].ID && event.ToolCallID == "main-spawn" && event.SessionID == infos[0].SessionID {
			found = true
		}
	}
	if !found {
		t.Fatal("subagent events lost owner/tool association")
	}
	// Reattaching an app consumer reuses the live owner, independent of the
	// renderer's message subscription lifetime.
	again, err := app.SubagentManager(mainID)
	if err != nil || again != manager || len(again.List()) != 1 {
		t.Fatal("consumer reattachment recreated manager")
	}
}

func TestSubagentAbortAndShutdownCancelOwnQuestions(t *testing.T) {
	for _, stop := range []string{"abort", "shutdown"} {
		t.Run(stop, func(t *testing.T) {
			app := newTestRuntime(t, &subagentProvider{}, nil)
			mainID := session.IDFromPath(app.session.Path())
			manager, err := app.SubagentManager(mainID)
			if err != nil {
				t.Fatal(err)
			}
			info, err := manager.Spawn(context.Background(), subagent.SpawnConfig{SystemPrompt: "child", Scope: subagent.Inherit()}, "call")
			if err != nil {
				t.Fatal(err)
			}
			if err := manager.Send(context.Background(), info.ID, "ask", false, "call"); err != nil {
				t.Fatal(err)
			}
			request, ok := waitForQuestion(app.questions)
			if !ok {
				t.Fatal("question missing")
			}
			ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
			defer cancel()
			if stop == "abort" {
				err = manager.Abort(ctx, info.ID)
			} else {
				err = app.Shutdown(ctx)
			}
			if err != nil {
				t.Fatal(err)
			}
			if err := app.questions.Respond(request.RequestID, questions.Answers{}); err == nil {
				t.Fatal("late question response accepted")
			}
			snapshot, _ := manager.Snapshot(info.ID)
			if snapshot.Info.Status != subagent.Aborted {
				t.Fatalf("status = %s", snapshot.Info.Status)
			}
			if stop == "abort" {
				if err := app.Shutdown(ctx); err != nil {
					t.Fatal(err)
				}
			}
		})
	}
}

func TestProductionSubagentToolsDoNotRecurseOrShareOwner(t *testing.T) {
	app := newTestRuntime(t, &subagentProvider{}, nil)
	defer app.Shutdown(context.Background())
	var registered tools.Tool
	for _, candidate := range app.applicationTools(session.IDFromPath(app.session.Path())) {
		if candidate.Name() == "subagent" {
			registered = candidate
		}
	}
	if registered == nil {
		t.Fatal("runtime did not register subagent")
	}
	if _, err := registered.Execute(context.Background(), "list", json.RawMessage(`{"action":"list"}`), nil); err != nil {
		t.Fatal(err)
	}
	if _, err := app.SubagentManager("other-session"); err == nil {
		t.Fatal("unregistered owner accepted")
	}
}
