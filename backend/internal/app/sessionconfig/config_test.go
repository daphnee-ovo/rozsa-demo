package sessionconfig

import (
	"path/filepath"
	"testing"

	"rozsa/backend/internal/app/session"
	modeltypes "rozsa/backend/internal/model/types"
)

func TestOverlayPersistsInOrderAndRestoresPerSession(t *testing.T) {
	path := filepath.Join(t.TempDir(), "a.jsonl")
	first, err := session.Create(path, "a", "/work", nil)
	if err != nil {
		t.Fatal(err)
	}
	base := Config{Model: modeltypes.Model{ID: "base", Provider: "p", Reasoning: true}, Thinking: modeltypes.ThinkingHigh}
	medium, max := "medium", "max"
	next := modeltypes.Model{ID: "next", Provider: "q", Reasoning: true, ThinkingEffortMap: map[modeltypes.ThinkingEffort]*string{modeltypes.ThinkingHigh: nil, modeltypes.ThinkingMedium: &medium, modeltypes.ThinkingMax: &max}}
	updated, err := SetModel(first, base, next)
	if err != nil {
		t.Fatal(err)
	}
	if updated.Thinking != modeltypes.ThinkingMedium {
		t.Fatalf("normalized effort=%s", updated.Thinking)
	}
	updated, err = SetThinking(first, updated, modeltypes.ThinkingMax)
	if err != nil {
		t.Fatal(err)
	}
	if updated.Thinking != modeltypes.ThinkingMax {
		t.Fatalf("effort=%s", updated.Thinking)
	}
	reopened, err := session.OpenManager(path)
	if err != nil {
		t.Fatal(err)
	}
	restored, err := Restore(reopened, base, func(provider, id string) (modeltypes.Model, error) { return next, nil })
	if err != nil {
		t.Fatal(err)
	}
	if restored.Model.ID != "next" || restored.Thinking != modeltypes.ThinkingMax {
		t.Fatalf("restored=%+v", restored)
	}
	other, err := session.Create(filepath.Join(t.TempDir(), "b.jsonl"), "b", "/work", nil)
	if err != nil {
		t.Fatal(err)
	}
	independent, err := Restore(other, base, func(string, string) (modeltypes.Model, error) {
		t.Fatal("unexpected resolver call")
		return modeltypes.Model{}, nil
	})
	if err != nil {
		t.Fatal(err)
	}
	if independent.Model.ID != "base" || independent.Thinking != modeltypes.ThinkingHigh {
		t.Fatalf("other session inherited overlay: %+v", independent)
	}
}

func TestUnsupportedEffortClampsDownAndPreservesOffFallback(t *testing.T) {
	low := "low"
	model := modeltypes.Model{Reasoning: true, ThinkingEffortMap: map[modeltypes.ThinkingEffort]*string{modeltypes.ThinkingHigh: nil, modeltypes.ThinkingMedium: nil, modeltypes.ThinkingLow: &low}}
	if got := NormalizeThinking(model, modeltypes.ThinkingMax); got != modeltypes.ThinkingLow {
		t.Fatalf("fallback=%s", got)
	}
	model.ThinkingEffortMap[modeltypes.ThinkingLow] = nil
	if got := NormalizeThinking(model, modeltypes.ThinkingMax); got != modeltypes.ThinkingOff {
		t.Fatalf("unsupported efforts should fall back to off, got=%s", got)
	}
}
