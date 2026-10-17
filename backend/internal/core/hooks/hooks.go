package hooks

import (
	"context"
	"encoding/json"

	"rozsa/backend/internal/core/tools"
	"rozsa/backend/internal/model/types"
)

type AgentContext struct {
	SystemPrompt *string
	Messages     []Message
	Tools        []types.ToolSchema
}

type Message struct {
	Kind     string
	Standard types.Message
	Custom   *CustomMessage
}

type CustomMessage struct {
	MessageType string
	Payload     json.RawMessage
	Timestamp   int64
}

type PreToolUseContext struct {
	Assistant types.AssistantMessage
	CallID    string
	ToolName  string
	Arguments json.RawMessage
	Context   AgentContext
}

type PreToolUseResult struct {
	Block  bool
	Reason *string
}

type PostToolUseContext struct {
	Assistant types.AssistantMessage
	CallID    string
	ToolName  string
	Arguments json.RawMessage
	Result    tools.Result
	IsError   bool
	Context   AgentContext
}

type PostToolUseResult struct {
	Content   []types.ContentBlock
	IsError   *bool
	Terminate *bool
}

type ShouldStopContext struct {
	Message     types.AssistantMessage
	ToolResults []types.ToolResultMessage
	Context     AgentContext
	NewMessages []Message
}

type TurnUpdate struct {
	Context        *AgentContext
	Model          *types.Model
	ThinkingEffort *types.ThinkingEffort
}

type PreToolUse func(context.Context, PreToolUseContext) (*PreToolUseResult, error)
type PostToolUse func(PostToolUseContext) *PostToolUseResult
type PrepareNextTurn func(ShouldStopContext) *TurnUpdate
