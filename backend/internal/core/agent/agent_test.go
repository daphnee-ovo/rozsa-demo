package agent

import (
	"context"
	"encoding/json"
	"errors"
	"sync/atomic"
	"testing"
	"time"

	"rozsa/backend/internal/core/hooks"
	"rozsa/backend/internal/core/tools"
	"rozsa/backend/internal/model/types"
)

type fakeProvider struct {
	events []types.StreamEvent
	wait   bool
}

func (provider fakeProvider) StreamSimple(ctx context.Context, _ types.Model, _ types.Context, _ types.SimpleStreamOptions, emit func(types.StreamEvent)) error {
	for _, event := range provider.events {
		emit(event)
	}
	if provider.wait {
		<-ctx.Done()
		return ctx.Err()
	}
	return nil
}

type fakeTool struct {
	name  string
	delay time.Duration
	calls *atomic.Int32
	wait  bool
	fail  bool
}

func (tool fakeTool) Name() string                                           { return tool.name }
func (tool fakeTool) Description() string                                    { return "fixture" }
func (tool fakeTool) Label() string                                          { return tool.name }
func (tool fakeTool) ParametersSchema() json.RawMessage                      { return json.RawMessage(`{"type":"object"}`) }
func (tool fakeTool) PrepareArguments(value json.RawMessage) json.RawMessage { return value }
func (tool fakeTool) ExecutionMode() *tools.ExecutionMode                    { return nil }
func (tool fakeTool) Execute(ctx context.Context, _ string, _ json.RawMessage, _ func(tools.Result)) (tools.Result, error) {
	if tool.calls != nil {
		tool.calls.Add(1)
	}
	if tool.wait {
		<-ctx.Done()
		return tools.Result{}, ctx.Err()
	}
	if tool.fail {
		return tools.Result{}, errors.New("fixture tool failure")
	}
	select {
	case <-time.After(tool.delay):
	case <-ctx.Done():
		return tools.Result{}, ctx.Err()
	}
	return tools.Result{Content: []types.ContentBlock{types.TextBlock{Text: tool.name}}, Details: json.RawMessage("null")}, nil
}

type scriptedProvider struct {
	messages []types.AssistantMessage
	calls    atomic.Int32
	contexts chan []types.Message
}

func (provider *scriptedProvider) StreamSimple(_ context.Context, _ types.Model, input types.Context, _ types.SimpleStreamOptions, emit func(types.StreamEvent)) error {
	provider.contexts <- append([]types.Message(nil), input.Messages...)
	index := int(provider.calls.Add(1)) - 1
	if index < len(provider.messages) {
		emit(types.Done{Message: provider.messages[index]})
	}
	return nil
}

func fixtureAssistant(content []types.ContentBlock, reason types.StopReason) types.AssistantMessage {
	return types.AssistantMessage{API: types.APIOpenAIResponses, Provider: "fixture", Model: "fixed", Content: content, StopReason: reason, Timestamp: 17}
}

func fixtureModel() types.Model {
	return types.Model{ID: "fixed", Provider: "fixture", API: types.APIOpenAIResponses}
}
func fixtureContext() hooks.AgentContext { return hooks.AgentContext{Tools: []types.ToolSchema{}} }

func collect(result Result) []Event {
	var events []Event
	for event := range result.Events {
		events = append(events, event)
	}
	return events
}

func TestParallelToolsReturnResultsInCallOrderAndHonorPreHook(t *testing.T) {
	assistant := fixtureAssistant([]types.ContentBlock{
		types.ToolCallBlock{ID: "first", Name: "slow", Arguments: json.RawMessage(`{}`)},
		types.ToolCallBlock{ID: "second", Name: "fast", Arguments: json.RawMessage(`{}`)},
	}, types.ToolUse)
	var executed atomic.Int32
	config := Config{Model: fixtureModel(), Provider: fakeProvider{events: []types.StreamEvent{types.Done{Message: assistant}}}, ToolExecution: tools.Parallel,
		Tools:               []tools.Tool{fakeTool{name: "slow", delay: 30 * time.Millisecond, calls: &executed}, fakeTool{name: "fast", delay: time.Millisecond, calls: &executed}},
		ShouldStopAfterTurn: func(hooks.ShouldStopContext) bool { return true }, Now: func() time.Time { return time.UnixMilli(99) }}
	events := collect(Run(context.Background(), nil, fixtureContext(), config))
	var ends []ToolExecutionEnd
	for _, event := range events {
		if end, ok := event.(ToolExecutionEnd); ok {
			ends = append(ends, end)
		}
	}
	if len(ends) != 2 || ends[0].CallID != "first" || ends[1].CallID != "second" {
		t.Fatalf("tool end order = %#v", ends)
	}
	if ends[0].Result.Timestamp != 99 || executed.Load() != 2 {
		t.Fatalf("timestamps/calls = %d/%d", ends[0].Result.Timestamp, executed.Load())
	}
	config.ToolExecution = tools.Sequential
	serialEvents := collect(Run(context.Background(), nil, fixtureContext(), config))
	var serialEnds []string
	for _, event := range serialEvents {
		if end, ok := event.(ToolExecutionEnd); ok {
			serialEnds = append(serialEnds, end.CallID)
		}
	}
	if len(serialEnds) != 2 || serialEnds[0] != "first" || serialEnds[1] != "second" {
		t.Fatalf("serial result order = %v", serialEnds)
	}

	executed.Store(0)
	config.PreToolUse = func(context.Context, hooks.PreToolUseContext) (*hooks.PreToolUseResult, error) {
		reason := "denied by fixture"
		return &hooks.PreToolUseResult{Block: true, Reason: &reason}, nil
	}
	events = collect(Run(context.Background(), nil, fixtureContext(), config))
	for _, event := range events {
		if end, ok := event.(ToolExecutionEnd); ok && end.CallID == "first" {
			if !end.Result.IsError {
				t.Fatal("blocked result must be error")
			}
			if executed.Load() != 0 {
				t.Fatal("blocked tool had a side effect")
			}
			return
		}
	}
	t.Fatal("missing result for denied tool")
}

func TestProviderCancellationEmitsAbortedTerminalState(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	config := Config{Model: fixtureModel(), Provider: fakeProvider{events: []types.StreamEvent{types.Start{Partial: fixtureAssistant([]types.ContentBlock{types.TextBlock{Text: "partial"}}, types.Stop)}}, wait: true}}
	result := Run(ctx, nil, fixtureContext(), config)
	var events []Event
	for len(events) < 3 {
		events = append(events, <-result.Events)
		if _, ok := events[len(events)-1].(MessageStart); ok {
			cancel()
		}
	}
	for event := range result.Events {
		events = append(events, event)
	}
	var sawAborted, sawEnd bool
	for _, event := range events {
		switch value := event.(type) {
		case TurnEnd:
			sawAborted = value.Message.StopReason == types.Aborted
		case AgentEnd:
			sawEnd = true
			if len(value.Messages) == 0 {
				t.Fatal("partial assistant message missing from terminal messages")
			}
		}
	}
	if !sawAborted || !sawEnd {
		t.Fatalf("terminal states: aborted=%v agent_end=%v", sawAborted, sawEnd)
	}
}

func TestSteeringFollowUpAndTurnTerminationPreserveCausalMessages(t *testing.T) {
	provider := &scriptedProvider{messages: []types.AssistantMessage{
		fixtureAssistant([]types.ContentBlock{types.TextBlock{Text: "first"}}, types.Stop),
		fixtureAssistant([]types.ContentBlock{types.TextBlock{Text: "second"}}, types.Stop),
		fixtureAssistant([]types.ContentBlock{types.TextBlock{Text: "third"}}, types.Stop),
	}, contexts: make(chan []types.Message, 3)}
	var steeringCalls, followUpCalls atomic.Int32
	steering := hooks.Message{Kind: "standard", Standard: types.UserMessage{Content: types.UserText("steering"), Timestamp: 21}}
	followUp := hooks.Message{Kind: "standard", Standard: types.UserMessage{Content: types.UserText("follow-up"), Timestamp: 22}}
	config := Config{Model: fixtureModel(), Provider: provider,
		GetSteeringMessages: func() []hooks.Message {
			if steeringCalls.Add(1) == 2 {
				return []hooks.Message{steering}
			}
			return nil
		},
		GetFollowUpMessages: func() []hooks.Message {
			if followUpCalls.Add(1) == 1 {
				return []hooks.Message{followUp}
			}
			return nil
		},
		ShouldStopAfterTurn: func(input hooks.ShouldStopContext) bool {
			return input.Message.Content[0].(types.TextBlock).Text == "third"
		},
		Now: func() time.Time { return time.UnixMilli(7) }}
	events := collect(Run(context.Background(), nil, fixtureContext(), config))
	if provider.calls.Load() != 3 {
		t.Fatalf("provider calls = %d, want 3", provider.calls.Load())
	}
	firstContext, secondContext, thirdContext := <-provider.contexts, <-provider.contexts, <-provider.contexts
	if len(firstContext) != 0 || len(secondContext) != 2 || len(thirdContext) != 4 {
		t.Fatalf("context lengths = %d,%d,%d", len(firstContext), len(secondContext), len(thirdContext))
	}
	if secondContext[0].(types.AssistantMessage).Content[0].(types.TextBlock).Text != "first" || thirdContext[2].(types.AssistantMessage).Content[0].(types.TextBlock).Text != "second" {
		t.Fatal("assistant turns were not retained in causal order")
	}
	if steeringCalls.Load() != 3 || followUpCalls.Load() != 1 {
		t.Fatalf("steering/follow-up checks = %d/%d", steeringCalls.Load(), followUpCalls.Load())
	}
	var turnEnds, agentEnds int
	for _, event := range events {
		switch event.(type) {
		case TurnEnd:
			turnEnds++
		case AgentEnd:
			agentEnds++
		}
	}
	if turnEnds != 3 || agentEnds != 1 {
		t.Fatalf("turn/agent terminal counts = %d/%d", turnEnds, agentEnds)
	}
}

func TestToolCancellationAndPostHookOverride(t *testing.T) {
	assistant := fixtureAssistant([]types.ContentBlock{types.ToolCallBlock{ID: "wait", Name: "wait", Arguments: json.RawMessage(`{}`)}}, types.ToolUse)
	ctx, cancel := context.WithCancel(context.Background())
	config := Config{Model: fixtureModel(), Provider: fakeProvider{events: []types.StreamEvent{types.Done{Message: assistant}}}, Tools: []tools.Tool{fakeTool{name: "wait", wait: true}}, ToolExecution: tools.Sequential}
	result := Run(ctx, nil, fixtureContext(), config)
	var events []Event
	for event := range result.Events {
		events = append(events, event)
		if _, ok := event.(ToolExecutionStart); ok {
			cancel()
		}
	}
	var cancelledResult, ended bool
	for _, event := range events {
		switch value := event.(type) {
		case ToolExecutionEnd:
			cancelledResult = value.Result.IsError
		case AgentEnd:
			ended = true
		}
	}
	if !cancelledResult || !ended {
		t.Fatalf("cancellation terminal result=%v agent_end=%v", cancelledResult, ended)
	}

	config = Config{Model: fixtureModel(), Provider: fakeProvider{events: []types.StreamEvent{types.Done{Message: assistant}}}, Tools: []tools.Tool{fakeTool{name: "wait"}}, ToolExecution: tools.Sequential,
		PostToolUse: func(hooks.PostToolUseContext) *hooks.PostToolUseResult {
			isError, terminate := true, true
			return &hooks.PostToolUseResult{Content: []types.ContentBlock{types.TextBlock{Text: "replaced"}}, IsError: &isError, Terminate: &terminate}
		},
		ShouldStopAfterTurn: func(input hooks.ShouldStopContext) bool { return len(input.ToolResults) == 1 }}
	for _, event := range collect(Run(context.Background(), nil, fixtureContext(), config)) {
		if end, ok := event.(ToolExecutionEnd); ok {
			text, isText := end.Result.Content[0].(types.TextBlock)
			if !end.Result.IsError || !isText || text.Text != "replaced" {
				t.Fatalf("post-hook result = %#v", end.Result)
			}
			return
		}
	}
	t.Fatal("post-hook result missing")
}

func TestToolFailureIsLocalAndPreHookFailureIsVisible(t *testing.T) {
	assistant := fixtureAssistant([]types.ContentBlock{types.ToolCallBlock{ID: "failed", Name: "broken", Arguments: json.RawMessage(`{}`)}}, types.ToolUse)
	var executed atomic.Int32
	config := Config{Model: fixtureModel(), Provider: fakeProvider{events: []types.StreamEvent{types.Done{Message: assistant}}}, Tools: []tools.Tool{fakeTool{name: "broken", calls: &executed, fail: true}}, ToolExecution: tools.Sequential,
		ShouldStopAfterTurn: func(hooks.ShouldStopContext) bool { return true }}
	events := collect(Run(context.Background(), nil, fixtureContext(), config))
	for _, event := range events {
		if end, ok := event.(ToolExecutionEnd); ok {
			if !end.Result.IsError || executed.Load() != 1 {
				t.Fatalf("local tool failure result/calls = %#v/%d", end.Result, executed.Load())
			}
			break
		}
	}

	executed.Store(0)
	config.PreToolUse = func(context.Context, hooks.PreToolUseContext) (*hooks.PreToolUseResult, error) {
		return nil, errors.New("permission backend unavailable")
	}
	events = collect(Run(context.Background(), nil, fixtureContext(), config))
	for _, event := range events {
		if end, ok := event.(ToolExecutionEnd); ok {
			if !end.Result.IsError || executed.Load() != 0 {
				t.Fatalf("pre-hook failure result/calls = %#v/%d", end.Result, executed.Load())
			}
			return
		}
	}
	t.Fatal("missing explicit hook failure result")
}
