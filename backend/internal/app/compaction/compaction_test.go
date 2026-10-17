package compaction

import (
	"context"
	"errors"
	"path/filepath"
	"testing"

	"rozsa/backend/internal/app/session"
)

func TestPrepareUsesReportedContextAndPreservesSafeToolBoundary(t *testing.T) {
	manager, err := session.Create(filepath.Join(t.TempDir(), "s.jsonl"), "s", "/work", nil)
	if err != nil {
		t.Fatal(err)
	}
	for i := 0; i < 4; i++ {
		if _, err := manager.AppendMessage(session.UserMessage{Content: session.UserText("01234567890123456789012345678901")}); err != nil {
			t.Fatal(err)
		}
	}
	call := session.AssistantMessage{Content: []session.ContentBlock{session.ToolCallBlock{ID: "c", Name: "tool", Arguments: []byte(`{}`)}}}
	if _, err := manager.AppendMessage(call); err != nil {
		t.Fatal(err)
	}
	if _, err := manager.AppendMessage(session.ToolResultMessage{ToolCallID: "c"}); err != nil {
		t.Fatal(err)
	}
	entries := manager.Entries()
	plan := (Engine{Trigger: Trigger{ThresholdTokens: 100, TargetTokens: 20}}).PrepareWithContext(entries, 100)
	if plan == nil {
		t.Fatal("reported context should trigger compaction")
	}
	if plan.CutPointIndex == len(entries)-1 || plan.CutPointIndex == len(entries)-2 {
		t.Fatalf("cut split tool pair at %d", plan.CutPointIndex)
	}
	if plan.EstimatedTokensBefore < 100 {
		t.Fatalf("tokensBefore=%d should include provider context", plan.EstimatedTokensBefore)
	}
}

func TestSettingsResolveRatiosAndValidateContextWindow(t *testing.T) {
	limits, err := DefaultSettings().ResolveTokenLimits(1000)
	if err != nil {
		t.Fatal(err)
	}
	if limits != (TokenLimits{ThresholdTokens: 850, TargetTokens: 300}) {
		t.Fatalf("limits=%+v", limits)
	}
	if _, err := DefaultSettings().ResolveTokenLimits(0); err == nil {
		t.Fatal("zero context window must fail")
	}
	invalid := DefaultSettings()
	invalid.TargetRatio = invalid.TriggerRatio
	if _, err := invalid.ResolveTokenLimits(1000); err == nil {
		t.Fatal("target at threshold must fail")
	}
}

func TestExecuteFailureAndCancellationDoNotAdvanceLeaf(t *testing.T) {
	for _, test := range []struct {
		name      string
		ctx       context.Context
		summarize func(context.Context, string) (string, error)
	}{
		{"summarize error", context.Background(), func(context.Context, string) (string, error) { return "", errors.New("summary failed") }},
		{"cancelled", cancelledContext(), func(context.Context, string) (string, error) { return "summary", nil }},
	} {
		t.Run(test.name, func(t *testing.T) {
			manager, err := session.Create(filepath.Join(t.TempDir(), "s.jsonl"), "s", "/work", nil)
			if err != nil {
				t.Fatal(err)
			}
			for i := 0; i < 3; i++ {
				if _, err = manager.AppendMessage(session.UserMessage{Content: session.UserText("long entry 012345678901234567890123456789")}); err != nil {
					t.Fatal(err)
				}
			}
			entries := manager.Entries()
			previous := manager.LeafID()
			plan := Plan{CutPointIndex: 1, EstimatedTokensBefore: 99}
			_, err = (Engine{}).Execute(test.ctx, manager, entries, plan, test.summarize)
			if err == nil {
				t.Fatal("expected failure")
			}
			if pointerValue(manager.LeafID()) != pointerValue(previous) {
				t.Fatal("leaf changed after failure")
			}
			if len(manager.Entries()) != len(entries) {
				t.Fatal("persisted compaction after failure")
			}
		})
	}
}

func pointerValue(value *string) string {
	if value == nil {
		return ""
	}
	return *value
}

func TestExecutePersistsBoundaryAndRestoreKeepsHistory(t *testing.T) {
	path := filepath.Join(t.TempDir(), "s.jsonl")
	manager, err := session.Create(path, "s", "/work", nil)
	if err != nil {
		t.Fatal(err)
	}
	for i := 0; i < 4; i++ {
		if _, err = manager.AppendMessage(session.UserMessage{Content: session.UserText("message")}); err != nil {
			t.Fatal(err)
		}
	}
	entries := manager.Entries()
	plan := Plan{CutPointIndex: 2, EstimatedTokensBefore: 123}
	result, err := (Engine{}).Execute(context.Background(), manager, entries, plan, func(context.Context, string) (string, error) { return "summary", nil })
	if err != nil {
		t.Fatal(err)
	}
	if result.RemovedCount != 2 {
		t.Fatalf("removed=%d", result.RemovedCount)
	}
	if len(manager.Entries()) != 5 {
		t.Fatalf("history entries=%d", len(manager.Entries()))
	}
	reopened, err := session.OpenManager(path)
	if err != nil {
		t.Fatal(err)
	}
	if len(reopened.ContextMessages()) != 2 {
		t.Fatalf("restored context messages=%d", len(reopened.ContextMessages()))
	}
}

func cancelledContext() context.Context {
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	return ctx
}
