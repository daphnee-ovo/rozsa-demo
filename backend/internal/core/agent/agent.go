package agent

import (
	"context"
	"encoding/json"
	"fmt"
	"strings"
	"sync"
	"time"

	"rozsa/backend/internal/core/hooks"
	"rozsa/backend/internal/core/tools"
	"rozsa/backend/internal/model/types"
)

type Provider interface {
	StreamSimple(context.Context, types.Model, types.Context, types.SimpleStreamOptions, func(types.StreamEvent)) error
}

type Event interface{ agentEvent() }
type AgentStart struct{}
type AgentEnd struct{ Messages []hooks.Message }
type TurnStart struct{}
type TurnEnd struct {
	Message     types.AssistantMessage
	ToolResults []types.ToolResultMessage
}
type MessageStart struct{ Message hooks.Message }
type MessageUpdate struct {
	Message     hooks.Message
	StreamEvent types.StreamEvent
}
type MessageEnd struct{ Message hooks.Message }
type ToolExecutionStart struct {
	CallID, ToolName string
	Arguments        json.RawMessage
}
type ToolExecutionUpdate struct {
	CallID, ToolName string
	Partial          tools.Result
}
type ToolExecutionEnd struct {
	CallID, ToolName string
	Result           types.ToolResultMessage
}

func (AgentStart) agentEvent()          {}
func (AgentEnd) agentEvent()            {}
func (TurnStart) agentEvent()           {}
func (TurnEnd) agentEvent()             {}
func (MessageStart) agentEvent()        {}
func (MessageUpdate) agentEvent()       {}
func (MessageEnd) agentEvent()          {}
func (ToolExecutionStart) agentEvent()  {}
func (ToolExecutionUpdate) agentEvent() {}
func (ToolExecutionEnd) agentEvent()    {}

type Config struct {
	Model               types.Model
	StreamOptions       types.SimpleStreamOptions
	Provider            Provider
	ConvertToLLM        func([]hooks.Message) []types.Message
	TransformContext    func([]hooks.Message) []hooks.Message
	GetAPIKey           func(string) *string
	ShouldStopAfterTurn func(hooks.ShouldStopContext) bool
	PrepareNextTurn     hooks.PrepareNextTurn
	GetSteeringMessages func() []hooks.Message
	GetFollowUpMessages func() []hooks.Message
	MaxTurns            *uint32
	ToolExecution       tools.ExecutionMode
	PreToolUse          hooks.PreToolUse
	PostToolUse         hooks.PostToolUse
	Tools               []tools.Tool
	Now                 func() time.Time
}

type Result struct{ Events <-chan Event }

func Run(ctx context.Context, prompts []hooks.Message, input hooks.AgentContext, config Config) Result {
	events := make(chan Event, 256)
	go func() {
		defer close(events)
		runner := loop{ctx: ctx, emit: events, config: config, context: input}
		runner.run(prompts)
	}()
	return Result{Events: events}
}

func Continue(ctx context.Context, input hooks.AgentContext, config Config) Result {
	if len(input.Messages) == 0 {
		return completed(nil)
	}
	last := input.Messages[len(input.Messages)-1]
	if last.Kind == "standard" {
		if _, ok := last.Standard.(types.AssistantMessage); ok {
			return completed(nil)
		}
	}
	return Run(ctx, nil, input, config)
}

func completed(messages []hooks.Message) Result {
	events := make(chan Event, 2)
	events <- AgentStart{}
	events <- AgentEnd{Messages: messages}
	close(events)
	return Result{Events: events}
}

type loop struct {
	ctx         context.Context
	emit        chan<- Event
	config      Config
	context     hooks.AgentContext
	newMessages []hooks.Message
	turnCount   uint32
	authRetried bool
}

func (runner *loop) send(event Event) bool {
	select {
	case runner.emit <- event:
		return true
	case <-runner.ctx.Done():
		return false
	}
}

func (runner *loop) sendTerminal(event Event) {
	select {
	case runner.emit <- event:
	case <-time.After(100 * time.Millisecond):
	}
}

func (runner *loop) run(prompts []hooks.Message) {
	runner.send(AgentStart{})
	runner.send(TurnStart{})
	for _, prompt := range prompts {
		runner.appendMessage(prompt)
	}
	pending := runner.callMessages(runner.config.GetSteeringMessages)
	firstTurn := true
	for {
		hasMoreTools := true
		for hasMoreTools || len(pending) > 0 {
			if runner.ctx.Err() != nil {
				runner.end()
				return
			}
			if !firstTurn {
				runner.send(TurnStart{})
			} else {
				firstTurn = false
			}
			for _, message := range pending {
				runner.appendMessage(message)
			}
			pending = nil
			modelContext := runner.modelContext()
			message, received, cancelled := runner.streamAssistant(modelContext)
			if cancelled {
				if received {
					runner.sendTerminal(TurnEnd{Message: message})
				}
				runner.end()
				return
			}
			if !received {
				runner.sendTerminal(TurnEnd{Message: abortedMessage(runner.config.Model)})
				runner.end()
				return
			}
			assistant := standard(message)
			runner.newMessages = append(runner.newMessages, assistant)
			if message.StopReason == types.Error && !runner.authRetried && runner.config.GetAPIKey != nil && message.ErrorMessage != nil && authFailure(*message.ErrorMessage) {
				runner.authRetried = true
				if runner.config.GetAPIKey(runner.config.Model.Provider) != nil {
					runner.context.Messages = runner.context.Messages[:len(runner.context.Messages)-1]
					runner.newMessages = runner.newMessages[:len(runner.newMessages)-1]
					continue
				}
			}
			if message.StopReason == types.Error || message.StopReason == types.Aborted {
				runner.sendTerminal(TurnEnd{Message: message})
				runner.end()
				return
			}
			calls := toolCalls(message)
			if message.StopReason == types.Length && len(calls) > 0 && !validObjectOrNull(calls[len(calls)-1].Arguments) {
				calls = calls[:len(calls)-1]
			}
			var results []types.ToolResultMessage
			hasMoreTools = false
			if len(calls) > 0 {
				batch := runner.executeBatch(message, calls)
				results = batch.messages
				hasMoreTools = !batch.terminate
				for _, result := range results {
					runner.appendMessage(standard(result))
				}
			}
			runner.sendTerminal(TurnEnd{Message: message, ToolResults: results})
			stopContext := runner.stopContext(message, results)
			if runner.config.PrepareNextTurn != nil {
				if update := runner.config.PrepareNextTurn(stopContext); update != nil {
					if update.Context != nil {
						runner.context = *update.Context
					}
					if update.Model != nil {
						runner.config.Model = *update.Model
					}
					if update.ThinkingEffort != nil {
						if *update.ThinkingEffort == types.ThinkingOff {
							runner.config.StreamOptions.Reasoning = nil
						} else {
							runner.config.StreamOptions.Reasoning = update.ThinkingEffort
						}
					}
				}
			}
			if runner.config.ShouldStopAfterTurn != nil && runner.config.ShouldStopAfterTurn(stopContext) {
				runner.end()
				return
			}
			runner.turnCount++
			if runner.config.MaxTurns != nil && runner.turnCount >= *runner.config.MaxTurns {
				runner.end()
				return
			}
			pending = runner.callMessages(runner.config.GetSteeringMessages)
		}
		pending = runner.callMessages(runner.config.GetFollowUpMessages)
		if len(pending) == 0 {
			break
		}
	}
	runner.end()
}

func (runner *loop) end() {
	runner.sendTerminal(AgentEnd{Messages: append([]hooks.Message(nil), runner.newMessages...)})
}
func (runner *loop) appendMessage(message hooks.Message) {
	runner.send(MessageStart{Message: message})
	runner.send(MessageEnd{Message: message})
	runner.context.Messages = append(runner.context.Messages, message)
	runner.newMessages = append(runner.newMessages, message)
}
func (runner *loop) callMessages(get func() []hooks.Message) []hooks.Message {
	if get == nil {
		return nil
	}
	return get()
}

func (runner *loop) modelContext() types.Context {
	messages := runner.context.Messages
	if runner.config.TransformContext != nil {
		messages = runner.config.TransformContext(messages)
	}
	var converted []types.Message
	if runner.config.ConvertToLLM != nil {
		converted = runner.config.ConvertToLLM(messages)
	} else {
		for _, message := range messages {
			if message.Kind == "standard" && message.Standard != nil {
				converted = append(converted, message.Standard)
			}
		}
	}
	return types.Context{SystemPrompt: runner.context.SystemPrompt, Messages: converted, Tools: append([]types.ToolSchema(nil), runner.context.Tools...)}
}

func (runner *loop) streamAssistant(modelContext types.Context) (types.AssistantMessage, bool, bool) {
	if runner.config.Provider == nil {
		return abortedMessage(runner.config.Model), false, false
	}
	streamContext, cancel := context.WithCancel(runner.ctx)
	stream := make(chan types.StreamEvent, 32)
	done := make(chan error, 1)
	finished := make(chan struct{})
	go func() {
		defer close(finished)
		done <- runner.config.Provider.StreamSimple(streamContext, runner.config.Model, modelContext, runner.config.StreamOptions, func(event types.StreamEvent) {
			select {
			case stream <- event:
			case <-streamContext.Done():
			}
		})
		close(stream)
	}()
	// Completion includes provider cleanup. The app's shutdown deadline can
	// report a provider that has not returned after context cancellation.
	defer func() { cancel(); <-finished }()
	var partial *types.AssistantMessage
	partialAdded := false
	for {
		select {
		case <-runner.ctx.Done():
			cancel()
			if partial != nil {
				aborted := *partial
				aborted.StopReason = types.Aborted
				aborted.ErrorMessage = nil
				final := standard(aborted)
				if partialAdded {
					runner.context.Messages[len(runner.context.Messages)-1] = final
				} else {
					runner.context.Messages = append(runner.context.Messages, final)
					runner.sendTerminal(MessageStart{Message: final})
				}
				runner.sendTerminal(MessageEnd{Message: final})
				runner.newMessages = append(runner.newMessages, final)
				return aborted, true, true
			}
			return abortedMessage(runner.config.Model), false, true
		case event, ok := <-stream:
			if !ok {
				err := <-done
				if err != nil {
					message := errorMessage(runner.config.Model, err)
					finalMessage := standard(message)
					if partialAdded {
						runner.context.Messages[len(runner.context.Messages)-1] = finalMessage
					} else {
						runner.context.Messages = append(runner.context.Messages, finalMessage)
						runner.send(MessageStart{Message: finalMessage})
					}
					runner.newMessages = append(runner.newMessages, finalMessage)
					runner.send(MessageEnd{Message: finalMessage})
					return message, true, false
				}
				if partialAdded {
					runner.context.Messages = runner.context.Messages[:len(runner.context.Messages)-1]
				}
				return abortedMessage(runner.config.Model), false, false
			}
			switch value := event.(type) {
			case types.Start:
				copy := value.Partial
				partial = &copy
				partialAdded = true
				message := standard(copy)
				runner.context.Messages = append(runner.context.Messages, message)
				runner.send(MessageStart{Message: message})
			case types.Done:
				message := standard(value.Message)
				if partialAdded {
					runner.context.Messages[len(runner.context.Messages)-1] = message
				} else {
					runner.context.Messages = append(runner.context.Messages, message)
					runner.send(MessageStart{Message: message})
				}
				runner.newMessages = append(runner.newMessages, message)
				runner.send(MessageEnd{Message: message})
				return value.Message, true, false
			case types.StreamError:
				message := standard(value.Error)
				if partialAdded {
					runner.context.Messages[len(runner.context.Messages)-1] = message
				} else {
					runner.context.Messages = append(runner.context.Messages, message)
					runner.send(MessageStart{Message: message})
				}
				runner.newMessages = append(runner.newMessages, message)
				runner.send(MessageEnd{Message: message})
				return value.Error, true, false
			default:
				if snapshot, ok := partialFromEvent(event); ok {
					partial = &snapshot
					message := standard(snapshot)
					if partialAdded {
						runner.context.Messages[len(runner.context.Messages)-1] = message
					}
					runner.send(MessageUpdate{Message: message, StreamEvent: event})
				}
			}
		}
	}
}

func partialFromEvent(event types.StreamEvent) (types.AssistantMessage, bool) {
	switch value := event.(type) {
	case types.TextStart:
		return value.Partial, true
	case types.TextDelta:
		return value.Partial, true
	case types.TextEnd:
		return value.Partial, true
	case types.ThinkingStart:
		return value.Partial, true
	case types.ThinkingDelta:
		return value.Partial, true
	case types.ThinkingEnd:
		return value.Partial, true
	case types.ToolCallStart:
		return value.Partial, true
	case types.ToolCallDelta:
		return value.Partial, true
	case types.ToolCallEnd:
		return value.Partial, true
	default:
		return types.AssistantMessage{}, false
	}
}

type batchResult struct {
	messages  []types.ToolResultMessage
	terminate bool
}
type finalized struct {
	call               types.ToolCallBlock
	content            []types.ContentBlock
	details            json.RawMessage
	isError, terminate bool
}

func (runner *loop) executeBatch(assistant types.AssistantMessage, calls []types.ToolCallBlock) batchResult {
	sequential := runner.config.ToolExecution == tools.Sequential
	for _, call := range calls {
		if tool := runner.findTool(call.Name); tool != nil && tool.ExecutionMode() != nil && *tool.ExecutionMode() == tools.Sequential {
			sequential = true
		}
	}
	results := make([]finalized, len(calls))
	if sequential {
		for index, call := range calls {
			if runner.ctx.Err() != nil {
				break
			}
			runner.send(ToolExecutionStart{CallID: call.ID, ToolName: call.Name, Arguments: call.Arguments})
			results[index] = runner.executeOne(assistant, call)
			result := runner.resultMessage(results[index])
			runner.sendTerminal(ToolExecutionEnd{CallID: call.ID, ToolName: call.Name, Result: result})
		}
	} else {
		var wait sync.WaitGroup
		limit := make(chan struct{}, 10)
		for index, call := range calls {
			if runner.ctx.Err() != nil {
				results[index] = cancelled(call)
				continue
			}
			runner.send(ToolExecutionStart{CallID: call.ID, ToolName: call.Name, Arguments: call.Arguments})
			wait.Add(1)
			go func(index int, call types.ToolCallBlock) {
				defer wait.Done()
				select {
				case limit <- struct{}{}:
				case <-runner.ctx.Done():
					results[index] = cancelled(call)
					return
				}
				defer func() { <-limit }()
				results[index] = runner.executeOne(assistant, call)
			}(index, call)
		}
		wait.Wait()
		for index, call := range calls {
			if results[index].call.ID == "" {
				results[index] = cancelled(call)
			}
			runner.sendTerminal(ToolExecutionEnd{CallID: call.ID, ToolName: call.Name, Result: runner.resultMessage(results[index])})
		}
	}
	messages := make([]types.ToolResultMessage, 0, len(calls))
	allTerminate := len(calls) > 0
	for index, call := range calls {
		if results[index].call.ID == "" {
			results[index] = cancelled(call)
		}
		message := runner.resultMessage(results[index])
		messages = append(messages, message)
		allTerminate = allTerminate && results[index].terminate
	}
	return batchResult{messages: messages, terminate: allTerminate}
}

func (runner *loop) executeOne(assistant types.AssistantMessage, call types.ToolCallBlock) (out finalized) {
	out.call = call
	defer func() {
		if recovered := recover(); recovered != nil {
			out = failure(call, fmt.Sprintf("Tool execution panicked: %v", recovered))
		}
	}()
	tool := runner.findTool(call.Name)
	if tool == nil {
		return failure(call, fmt.Sprintf("Tool '%s' not found", call.Name))
	}
	if runner.config.PreToolUse != nil {
		result, err, cancelledHook := runPreHook(runner.ctx, runner.config.PreToolUse, hooks.PreToolUseContext{Assistant: assistant, CallID: call.ID, ToolName: call.Name, Arguments: call.Arguments, Context: runner.context})
		if cancelledHook || runner.ctx.Err() != nil {
			return cancelled(call)
		}
		if err != nil {
			return failure(call, fmt.Sprintf("pre-tool hook failed: %v", err))
		}
		if result != nil && result.Block {
			reason := "Tool execution was blocked"
			if result.Reason != nil {
				reason = *result.Reason
			}
			return failure(call, reason)
		}
	}
	arguments, err := tools.NormalizeArguments(call.Arguments, tool)
	if err != nil {
		return failure(call, fmt.Sprintf("invalid tool arguments: %v", err))
	}
	result, err, wasCancelled := runTool(runner.ctx, tool, call.ID, arguments, func(partial tools.Result) {
		runner.send(ToolExecutionUpdate{CallID: call.ID, ToolName: call.Name, Partial: partial})
	})
	if wasCancelled || runner.ctx.Err() != nil {
		return cancelled(call)
	}
	if err != nil {
		return failure(call, err.Error())
	}
	out.content, out.details, out.terminate = result.Content, result.Details, result.Terminate
	if runner.config.PostToolUse != nil {
		if override, panicked := callPostHook(runner.config.PostToolUse, hooks.PostToolUseContext{Assistant: assistant, CallID: call.ID, ToolName: call.Name, Arguments: call.Arguments, Result: result, Context: runner.context, IsError: false}); !panicked && override != nil {
			if override.Content != nil {
				out.content = override.Content
			}
			if override.IsError != nil {
				out.isError = *override.IsError
			}
			if override.Terminate != nil {
				out.terminate = *override.Terminate
			}
		}
	}
	return out
}

type toolOutcome struct {
	result tools.Result
	err    error
}

func runTool(ctx context.Context, tool tools.Tool, callID string, arguments json.RawMessage, update func(tools.Result)) (tools.Result, error, bool) {
	outcome := make(chan toolOutcome, 1)
	go func() {
		defer func() {
			if recovered := recover(); recovered != nil {
				outcome <- toolOutcome{err: fmt.Errorf("tool execution panicked: %v", recovered)}
			}
		}()
		result, err := tool.Execute(ctx, callID, arguments, update)
		outcome <- toolOutcome{result: result, err: err}
	}()
	select {
	case value := <-outcome:
		return value.result, value.err, false
	case <-ctx.Done():
		return tools.Result{}, ctx.Err(), true
	}
}

type hookOutcome struct {
	result *hooks.PreToolUseResult
	err    error
}

func runPreHook(ctx context.Context, hook hooks.PreToolUse, input hooks.PreToolUseContext) (*hooks.PreToolUseResult, error, bool) {
	outcome := make(chan hookOutcome, 1)
	go func() {
		defer func() {
			if recovered := recover(); recovered != nil {
				outcome <- hookOutcome{err: fmt.Errorf("pre-tool hook panicked: %v", recovered)}
			}
		}()
		result, err := hook(ctx, input)
		outcome <- hookOutcome{result: result, err: err}
	}()
	select {
	case value := <-outcome:
		return value.result, value.err, false
	case <-ctx.Done():
		return nil, ctx.Err(), true
	}
}

func callPostHook(hook hooks.PostToolUse, input hooks.PostToolUseContext) (result *hooks.PostToolUseResult, panicked bool) {
	defer func() {
		if recover() != nil {
			panicked = true
		}
	}()
	return hook(input), false
}
func (runner *loop) findTool(name string) tools.Tool {
	for _, tool := range runner.config.Tools {
		if tool.Name() == name {
			return tool
		}
	}
	return nil
}
func (runner *loop) resultMessage(value finalized) types.ToolResultMessage {
	timestamp := time.Now
	if runner.config.Now != nil {
		timestamp = runner.config.Now
	}
	details := value.details
	if len(details) == 0 {
		details = json.RawMessage("null")
	}
	return types.ToolResultMessage{ToolCallID: value.call.ID, ToolName: value.call.Name, Content: value.content, Details: details, IsError: value.isError, Timestamp: timestamp().UnixMilli()}
}
func failure(call types.ToolCallBlock, message string) finalized {
	return finalized{call: call, content: []types.ContentBlock{types.TextBlock{Text: message}}, details: json.RawMessage("null"), isError: true}
}
func cancelled(call types.ToolCallBlock) finalized {
	return failure(call, "Tool execution was cancelled")
}
func (runner *loop) stopContext(message types.AssistantMessage, results []types.ToolResultMessage) hooks.ShouldStopContext {
	return hooks.ShouldStopContext{Message: message, ToolResults: results, Context: runner.context, NewMessages: runner.newMessages}
}
func standard(message types.Message) hooks.Message {
	return hooks.Message{Kind: "standard", Standard: message}
}
func toolCalls(message types.AssistantMessage) []types.ToolCallBlock {
	var calls []types.ToolCallBlock
	for _, block := range message.Content {
		if call, ok := block.(types.ToolCallBlock); ok {
			calls = append(calls, call)
		}
	}
	return calls
}
func validObjectOrNull(raw json.RawMessage) bool {
	if len(raw) == 0 || string(raw) == "null" {
		return true
	}
	var value any
	if json.Unmarshal(raw, &value) != nil {
		return false
	}
	_, ok := value.(map[string]any)
	return ok
}
func authFailure(message string) bool {
	lower := strings.ToLower(message)
	return strings.Contains(lower, "401") || strings.Contains(lower, "unauthorized") || strings.Contains(lower, "authentication")
}
func abortedMessage(model types.Model) types.AssistantMessage {
	return types.AssistantMessage{API: model.API, Provider: model.Provider, Model: model.ID, StopReason: types.Aborted}
}
func errorMessage(model types.Model, err error) types.AssistantMessage {
	message := err.Error()
	output := abortedMessage(model)
	output.StopReason = types.Error
	output.ErrorMessage = &message
	return output
}
