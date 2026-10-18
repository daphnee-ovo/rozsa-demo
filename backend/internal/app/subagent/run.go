package subagent

import (
	"context"
	"errors"
	"fmt"
	"time"

	"rozsa/backend/internal/app/sessions"
	"rozsa/backend/internal/core/agent"
	"rozsa/backend/internal/core/hooks"
	"rozsa/backend/internal/core/tools"
	"rozsa/backend/internal/model/types"
)

func (manager *manager) run(entry *entry, execution sessions.Execution, callID string) {
	manager.mu.Lock()
	messages := append([]hooks.Message(nil), entry.messages...)
	info := entry.info
	manager.mu.Unlock()
	var permission hooks.PreToolUse
	if manager.config.PermissionHook != nil {
		permission = manager.config.PermissionHook(info.SessionID)
	}
	maxTurns := uint32(100)
	work := newExecutionWork()
	ownedTools := make([]tools.Tool, 0, len(entry.tools))
	for _, tool := range entry.tools {
		ownedTools = append(ownedTools, ownedTool{Tool: tool, work: work})
	}
	effort := info.ThinkingEffort
	options := types.SimpleStreamOptions{Base: types.StreamOptions{Headers: entry.model.Headers, SessionID: &info.SessionID}}
	if effort != "" && effort != types.ThinkingOff {
		options.Reasoning = &effort
	}
	config := agent.Config{Model: entry.model, Provider: manager.config.Provider, Tools: ownedTools, StreamOptions: options, MaxTurns: &maxTurns, ToolExecution: tools.Parallel,
		ConvertToLLM: func(messages []hooks.Message) []types.Message {
			result := make([]types.Message, 0, len(messages))
			for _, message := range messages {
				if message.Kind == "standard" {
					result = append(result, message.Standard)
				}
			}
			return result
		},
		PreToolUse: func(ctx context.Context, request hooks.PreToolUseContext) (*hooks.PreToolUseResult, error) {
			if !work.begin(ctx) {
				return nil, context.Canceled
			}
			defer work.finish()
			if err := entry.config.Scope.Check(request.ToolName, request.Arguments, manager.config.Cwd); err != nil {
				reason := err.Error()
				return &hooks.PreToolUseResult{Block: true, Reason: &reason}, nil
			}
			if permission != nil {
				return permission(ctx, request)
			}
			return nil, nil
		}}
	input := hooks.AgentContext{SystemPrompt: &entry.config.SystemPrompt, Messages: messages, Tools: tools.Metadata(entry.tools)}
	stream := agent.Continue(execution.Context(), input, config)
	manager.emit(entry, callID, nil)
	var runErr error
	for event := range stream.Events {
		if ended, ok := event.(agent.MessageEnd); ok {
			manager.mu.Lock()
			entry.messages = append(entry.messages, ended.Message)
			entry.info.MessageCount, entry.info.LastActivityAt = len(entry.messages), time.Now().UnixMilli()
			if err := manager.persist(entry, ended.Message); err != nil {
				runErr = errors.Join(runErr, err)
			}
			if message, ok := ended.Message.Standard.(types.AssistantMessage); ok && message.StopReason == types.Error {
				if message.ErrorMessage != nil {
					runErr = errors.Join(runErr, errors.New(*message.ErrorMessage))
				} else {
					runErr = errors.Join(runErr, errors.New("provider stream failed"))
				}
			}
			manager.mu.Unlock()
			if runErr != nil {
				runErr = errors.Join(runErr, execution.Cancel())
			}
		}
		manager.emit(entry, callID, event)
	}
	work.wait()
	runErr = errors.Join(runErr, execution.Context().Err(), execution.PromptFinished())
	runErr = execution.Finish(runErr)
	manager.mu.Lock()
	if entry.info.Status != Aborted {
		entry.info.Status = Idle
		if runErr != nil {
			entry.info.Status = Error
		}
	}
	if runErr != nil && !sessions.CancellationOnly(runErr) {
		value := runErr.Error()
		entry.info.LastError = &value
	}
	entry.info.LastActivityAt = time.Now().UnixMilli()
	manager.mu.Unlock()
	manager.emit(entry, callID, nil)
	manager.mu.Lock()
	entry.execution = nil
	close(entry.done)
	manager.mu.Unlock()
}

func (manager *manager) persist(entry *entry, message hooks.Message) error {
	if manager.config.SessionDir == "" {
		return nil
	}
	converted, ok := manager.config.ToSessionMessage(message)
	if !ok {
		return fmt.Errorf("cannot persist subagent message kind %q", message.Kind)
	}
	if _, err := entry.writer.AppendMessage(converted); err != nil {
		return fmt.Errorf("persist subagent message: %w", err)
	}
	return nil
}

func (manager *manager) emit(entry *entry, callID string, event agent.Event) {
	if manager.config.Emit == nil {
		return
	}
	manager.eventMu.Lock()
	defer manager.eventMu.Unlock()
	manager.mu.Lock()
	manager.sequence++
	value := Event{OwnerSessionID: manager.config.MainSessionUUID, SubagentID: entry.info.ID, SessionID: entry.info.SessionID, ToolCallID: callID, Sequence: manager.sequence, Info: cloneInfo(entry.info), Agent: event}
	manager.mu.Unlock()
	manager.config.Emit(value)
}
