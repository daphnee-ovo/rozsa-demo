package types

import (
	"context"
	"encoding/json"
	"time"
)

// Protocol is the common model protocol contract. Shared streaming options are
// converted to the protocol-specific simple form by Stream.
type Protocol interface {
	API() API
	StreamSimple(context.Context, Model, Context, SimpleStreamOptions, func(StreamEvent)) error
}

// ExternalCredentials marks protocols that authenticate through their own
// credential chain instead of an application-managed API key.
type ExternalCredentials interface {
	UsesExternalCredentials() bool
}

// Stream adapts shared options to the protocol's simple streaming contract.
func Stream(protocol Protocol, ctx context.Context, model Model, input Context, options StreamOptions, emit func(StreamEvent)) error {
	return protocol.StreamSimple(ctx, model, input, SimpleStreamOptions{Base: options}, emit)
}

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
	MaxRetries     *uint32
	MaxRetryDelay  time.Duration
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
