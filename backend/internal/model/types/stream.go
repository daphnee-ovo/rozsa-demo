package types

import (
	"encoding/json"
	"time"
)

type CacheRetention uint8

const (
	CacheDefault CacheRetention = iota
	CacheShort
	CacheLong
)

// CacheDefault matches Anthropic's Rust CacheRetention::None: it selects long
// from ROZSA_CACHE_RETENTION, otherwise short. Rust has no cache-disable option.
type StreamOptions struct {
	Temperature    *float64
	MaxTokens      *int
	APIKey         string
	CacheRetention CacheRetention
	SessionID      *string
	Headers        map[string]string
	Timeout        time.Duration
	Metadata       json.RawMessage
}
type SimpleStreamOptions struct {
	Base                  StreamOptions
	Reasoning             *ThinkingEffort
	ThinkingEffortBudgets map[ThinkingEffort]uint64
	ToolChoice            json.RawMessage
}

// StreamEvent variants preserve the Rust event order and independent snapshots.
// A synchronous emitter supplies backpressure; cancellation belongs to context.
type StreamEvent interface{ streamEvent() }
type Start struct{ Partial AssistantMessage }
type TextStart struct {
	ContentIndex int
	Partial      AssistantMessage
}
type TextDelta struct {
	ContentIndex int
	Delta        string
	Partial      AssistantMessage
}
type TextEnd struct {
	ContentIndex int
	Content      string
	Partial      AssistantMessage
}
type ThinkingStart struct {
	ContentIndex int
	Partial      AssistantMessage
}
type ThinkingDelta struct {
	ContentIndex int
	Delta        string
	Partial      AssistantMessage
}
type ThinkingEnd struct {
	ContentIndex int
	Content      string
	Partial      AssistantMessage
}
type ToolCallStart struct {
	ContentIndex int
	Partial      AssistantMessage
}
type ToolCallDelta struct {
	ContentIndex int
	Delta        string
	Partial      AssistantMessage
}
type ToolCallEnd struct {
	ContentIndex int
	ToolCall     ToolCallBlock
	Partial      AssistantMessage
}
type Done struct {
	Reason  StopReason
	Message AssistantMessage
}
type StreamError struct {
	Reason StopReason
	Error  AssistantMessage
}

func (Start) streamEvent()         {}
func (TextStart) streamEvent()     {}
func (TextDelta) streamEvent()     {}
func (TextEnd) streamEvent()       {}
func (ThinkingStart) streamEvent() {}
func (ThinkingDelta) streamEvent() {}
func (ThinkingEnd) streamEvent()   {}
func (ToolCallStart) streamEvent() {}
func (ToolCallDelta) streamEvent() {}
func (ToolCallEnd) streamEvent()   {}
func (Done) streamEvent()          {}
func (StreamError) streamEvent()   {}
