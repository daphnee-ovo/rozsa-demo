package subagent

import (
	"context"
	"encoding/json"
	"strings"
	"testing"

	"rozsa/backend/internal/app/session"
	app "rozsa/backend/internal/app/subagent"
	"rozsa/backend/internal/core/hooks"
	"rozsa/backend/internal/model/types"
)

type provider struct{}

func (provider) StreamSimple(_ context.Context, _ types.Model, _ types.Context, _ types.SimpleStreamOptions, emit func(types.StreamEvent)) error {
	emit(types.Done{Reason: types.Stop, Message: types.AssistantMessage{Content: []types.ContentBlock{types.TextBlock{Text: "actual response"}}, StopReason: types.Stop}})
	return nil
}

func TestToolActionsRunActualManager(t *testing.T) {
	manager, err := app.New(app.Config{MainSessionUUID: "main", Provider: provider{}, ToSessionMessage: func(hooks.Message) (session.Message, bool) { return nil, false }})
	if err != nil {
		t.Fatal(err)
	}
	defer manager.Shutdown(context.Background())
	tool := New(manager)
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
	for _, raw := range []string{`{"action":"spawn"}`, `{"action":"send","id":"x"}`, `{"action":"wait"}`, `{"action":"unknown"}`, `{"action":"spawn","system_prompt":"x","scope":"invalid"}`} {
		if _, err := tool.Execute(context.Background(), "invalid", json.RawMessage(raw), nil); err == nil {
			t.Fatalf("accepted %s", raw)
		}
	}
}

func TestScopeParsingPreservesEmptyRestrictions(t *testing.T) {
	for _, raw := range []string{`{"type":"custom","tools":[]}`, `{"type":"scoped","paths":[]}`} {
		scope, err := parseScope(json.RawMessage(raw))
		if err != nil {
			t.Fatal(err)
		}
		if err := scope.Check("read", json.RawMessage(`{"path":"anywhere"}`), "/workspace"); err == nil {
			t.Fatal("empty restriction became unrestricted")
		}
	}
	if _, err := parseScope(json.RawMessage(`{"type":"custom","paths":[3]}`)); err == nil {
		t.Fatal("malformed paths silently weakened scope")
	}
}
