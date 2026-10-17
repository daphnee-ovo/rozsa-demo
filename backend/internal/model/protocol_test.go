package model

import (
	"context"
	"testing"
	"time"

	"rozsa/backend/internal/model/types"
)

type captureProtocol struct {
	options types.SimpleStreamOptions
}

func (protocol *captureProtocol) API() types.API { return "custom-api" }

func (protocol *captureProtocol) StreamSimple(_ context.Context, _ types.Model, _ types.Context, options types.SimpleStreamOptions, _ func(types.StreamEvent)) error {
	protocol.options = options
	return nil
}

func TestStreamConvertsSharedOptionsToSimpleOptions(t *testing.T) {
	temperature := 0.25
	maxTokens := 321
	sessionID := "session-1"
	options := types.StreamOptions{
		Temperature:    &temperature,
		MaxTokens:      &maxTokens,
		APIKey:         "secret",
		CacheRetention: types.CacheShort,
		SessionID:      &sessionID,
		Headers:        map[string]string{"X-Test": "value"},
		Timeout:        4 * time.Second,
	}
	protocol := &captureProtocol{}
	if err := types.Stream(protocol, context.Background(), types.Model{}, types.Context{}, options, nil); err != nil {
		t.Fatal(err)
	}
	got := protocol.options
	if got.Base.APIKey != options.APIKey || got.Base.Timeout != options.Timeout || got.Base.CacheRetention != options.CacheRetention {
		t.Fatalf("base options = %#v, want shared options %#v", got.Base, options)
	}
	if got.Base.Temperature != options.Temperature || got.Base.MaxTokens != options.MaxTokens || got.Base.SessionID != options.SessionID {
		t.Fatalf("option pointers were not forwarded: %#v", got.Base)
	}
	if got.Base.Headers["X-Test"] != "value" || got.Reasoning != nil || got.ThinkingEffortBudgets != nil || got.ToolChoice != nil {
		t.Fatalf("simple options = %#v", got)
	}
}

func TestNewProtocolFailsForUnregisteredAPI(t *testing.T) {
	protocol, err := NewProtocol(types.API("custom-api"))
	if err == nil || protocol != nil {
		t.Fatalf("NewProtocol() = (%v, %v), want unsupported API error", protocol, err)
	}
	if err.Error() != `unsupported model API "custom-api"` {
		t.Fatalf("error = %v, want explicit unsupported API diagnostic", err)
	}
}
