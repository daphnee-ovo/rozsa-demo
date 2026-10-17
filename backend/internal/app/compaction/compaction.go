// Package compaction plans and persists append-only session compaction.
package compaction

import (
	"context"
	"encoding/json"
	"fmt"
	"math"

	"rozsa/backend/internal/app/session"
)

type Trigger struct{ ThresholdTokens, TargetTokens uint64 }
type Plan struct {
	CutPointIndex         int
	EntriesToRemove       []string
	EstimatedTokensBefore uint64
}
type Result struct {
	Summary      string
	RemovedCount int
}
type Engine struct{ Trigger Trigger }
type Settings struct {
	Enabled                   bool
	TriggerRatio, TargetRatio float64
}
type TokenLimits struct{ ThresholdTokens, TargetTokens uint64 }

func DefaultSettings() Settings {
	return Settings{Enabled: true, TriggerRatio: 0.85, TargetRatio: 0.30}
}
func (settings Settings) ResolveTokenLimits(contextWindow uint64) (TokenLimits, error) {
	if math.IsNaN(settings.TriggerRatio) || math.IsInf(settings.TriggerRatio, 0) || settings.TriggerRatio <= 0 || settings.TriggerRatio > 1 {
		return TokenLimits{}, fmt.Errorf("compaction trigger ratio must be greater than 0 and at most 1: %v", settings.TriggerRatio)
	}
	if math.IsNaN(settings.TargetRatio) || math.IsInf(settings.TargetRatio, 0) || settings.TargetRatio <= 0 || settings.TargetRatio > 1 {
		return TokenLimits{}, fmt.Errorf("compaction target ratio must be greater than 0 and at most 1: %v", settings.TargetRatio)
	}
	if settings.TargetRatio >= settings.TriggerRatio {
		return TokenLimits{}, fmt.Errorf("compaction target ratio must be less than trigger ratio: target=%v, trigger=%v", settings.TargetRatio, settings.TriggerRatio)
	}
	if contextWindow == 0 {
		return TokenLimits{}, fmt.Errorf("model context window must be greater than zero")
	}
	thresholdFloat := math.Ceil(float64(contextWindow) * settings.TriggerRatio)
	targetFloat := math.Floor(float64(contextWindow) * settings.TargetRatio)
	threshold := uint64(thresholdFloat)
	target := uint64(targetFloat)
	if target > threshold-1 {
		target = threshold - 1
	}
	return TokenLimits{ThresholdTokens: threshold, TargetTokens: target}, nil
}

func DefaultTrigger() Trigger { return Trigger{ThresholdTokens: 100_000, TargetTokens: 20_000} }
func (engine Engine) ShouldCompact(contextTokens uint64) bool {
	return contextTokens >= engine.Trigger.ThresholdTokens
}
func (engine Engine) Prepare(entries []session.Entry) *Plan {
	var total uint64
	for _, entry := range entries {
		total += EstimateEntryTokens(entry)
	}
	return engine.PrepareWithContext(entries, total)
}

func (engine Engine) PrepareWithContext(entries []session.Entry, contextTokens uint64) *Plan {
	if len(entries) == 0 {
		return nil
	}
	var total uint64
	for _, entry := range entries {
		total += EstimateEntryTokens(entry)
	}
	if contextTokens < engine.Trigger.ThresholdTokens && total < engine.Trigger.ThresholdTokens {
		return nil
	}
	keep := engine.Trigger.TargetTokens
	if cap := engine.Trigger.ThresholdTokens - 1; keep > cap {
		keep = cap
	}
	var kept uint64
	cut := len(entries)
	for i := len(entries) - 1; i >= 0; i-- {
		kept += EstimateEntryTokens(entries[i])
		if kept >= keep {
			cut = i
			break
		}
	}
	if cut == 0 || cut >= len(entries) {
		return nil
	}
	cut = safeBoundary(entries, cut)
	if cut == 0 {
		return nil
	}
	removed := make([]string, 0, cut)
	for _, entry := range entries[:cut] {
		removed = append(removed, entry.ID)
	}
	if contextTokens > total {
		total = contextTokens
	}
	return &Plan{CutPointIndex: cut, EntriesToRemove: removed, EstimatedTokensBefore: total}
}

// Execute summarizes first. Cancellation, summarizer failure, and append
// failure leave the manager's active leaf unchanged; successful append is the
// commit point for the durable compaction boundary.
func (engine Engine) Execute(ctx context.Context, manager *session.Manager, entries []session.Entry, plan Plan, summarize func(context.Context, string) (string, error)) (Result, error) {
	if plan.CutPointIndex <= 0 || plan.CutPointIndex >= len(entries) {
		return Result{}, fmt.Errorf("invalid compaction cut point %d", plan.CutPointIndex)
	}
	if err := ctx.Err(); err != nil {
		return Result{}, err
	}
	var content []byte
	for _, entry := range entries[:plan.CutPointIndex] {
		if message, ok := entry.Payload.(session.MessageEntry); ok {
			serialized, err := json.Marshal(message.Message)
			if err != nil {
				return Result{}, fmt.Errorf("serialize message for compaction: %w", err)
			}
			if containsImage(message.Message) {
				serialized = stripImageData(serialized)
			}
			content = append(content, serialized...)
			content = append(content, '\n')
		}
	}
	summary, err := summarize(ctx, string(content))
	if err != nil {
		return Result{}, err
	}
	if err := ctx.Err(); err != nil {
		return Result{}, err
	}
	if summary == "" {
		return Result{}, fmt.Errorf("compaction summarization returned empty result")
	}
	if _, err := manager.AppendCompaction(summary, entries[plan.CutPointIndex].ID, plan.EstimatedTokensBefore, nil, nil); err != nil {
		return Result{}, err
	}
	removed := 0
	for _, entry := range entries[:plan.CutPointIndex] {
		if _, ok := entry.Payload.(session.MessageEntry); ok {
			removed++
		}
	}
	return Result{Summary: summary, RemovedCount: removed}, nil
}

func EstimateEntryTokens(entry session.Entry) uint64 {
	switch payload := entry.Payload.(type) {
	case session.MessageEntry:
		if assistant, ok := payload.Message.(session.AssistantMessage); ok && assistant.Usage.Output > 0 {
			return assistant.Usage.Output
		}
		data, err := json.Marshal(payload.Message)
		if err != nil {
			return 0
		}
		return EstimateTokens(string(data))
	case session.Compaction:
		return EstimateTokens(payload.Summary)
	case session.Custom:
		if len(payload.Data) > 0 {
			return EstimateTokens(string(payload.Data))
		}
	}
	return 0
}
func EstimateTokens(text string) uint64 { return (uint64(len(text)) + 3) / 4 }

func safeBoundary(entries []session.Entry, raw int) int {
	cut := raw
	for cut > 0 {
		message, ok := entries[cut].Payload.(session.MessageEntry)
		if !ok {
			break
		}
		switch next := message.Message.(type) {
		case session.ToolResultMessage:
			cut--
			continue
		case session.AssistantMessage:
			if hasToolCall(next) && cut+1 < len(entries) {
				if nextEntry, ok := entries[cut+1].Payload.(session.MessageEntry); ok {
					if _, ok := nextEntry.Message.(session.ToolResultMessage); ok {
						cut--
						continue
					}
				}
			}
		}
		break
	}
	return cut
}
func hasToolCall(message session.AssistantMessage) bool {
	for _, block := range message.Content {
		if _, ok := block.(session.ToolCallBlock); ok {
			return true
		}
	}
	return false
}
func containsImage(message session.Message) bool {
	var blocks []session.ContentBlock
	switch value := message.(type) {
	case session.AssistantMessage:
		blocks = value.Content
	case session.ToolResultMessage:
		blocks = value.Content
	}
	for _, block := range blocks {
		if _, ok := block.(session.ImageBlock); ok {
			return true
		}
	}
	return false
}
func stripImageData(data []byte) []byte {
	var value any
	if json.Unmarshal(data, &value) != nil {
		return data
	}
	var visit func(any)
	visit = func(node any) {
		switch current := node.(type) {
		case map[string]any:
			if current["type"] == "image" {
				if _, ok := current["data"]; ok {
					current["data"] = "[image data omitted]"
				}
			}
			for _, child := range current {
				visit(child)
			}
		case []any:
			for _, child := range current {
				visit(child)
			}
		}
	}
	visit(value)
	result, err := json.Marshal(value)
	if err != nil {
		return data
	}
	return result
}
