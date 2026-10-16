package types

import "encoding/json"

// Provider messages are model-owned; persistence adapters must not make model
// providers depend on the app/session package.
type Message interface{ modelMessage() }
type UserMessage struct {
	Content     UserContent
	DisplayText *string
	Timestamp   int64
}
type AssistantMessage struct {
	Content                   []ContentBlock
	API                       API
	Provider, Model           string
	ResponseModel, ResponseID *string
	Usage                     Usage
	StopReason                StopReason
	ErrorMessage              *string
	Timestamp                 int64
}
type ToolResultMessage struct {
	ToolCallID, ToolName string
	Content              []ContentBlock
	Details              json.RawMessage
	IsError              bool
	Timestamp            int64
}

func (UserMessage) modelMessage()       {}
func (AssistantMessage) modelMessage()  {}
func (ToolResultMessage) modelMessage() {}

type UserContent interface{ userContent() }
type UserText string
type UserBlocks []ContentBlock

func (UserText) userContent()   {}
func (UserBlocks) userContent() {}

type ContentBlock interface{ contentBlock() }
type TextBlock struct {
	Text      string
	Signature *string
}
type ThinkingBlock struct {
	Thinking  string
	Signature *string
	Redacted  bool
}
type ImageBlock struct{ Data, MimeType string }
type ToolCallBlock struct {
	ID, Name  string
	Arguments json.RawMessage
}

func (TextBlock) contentBlock()     {}
func (ThinkingBlock) contentBlock() {}
func (ImageBlock) contentBlock()    {}
func (ToolCallBlock) contentBlock() {}

type StopReason uint8

const (
	Stop StopReason = iota
	Length
	ToolUse
	Error
	Aborted
)

type Usage struct {
	Input, Output, CacheRead, CacheWrite, TotalTokens uint64
	Cost                                              UsageCost
}
type UsageCost struct{ Input, Output, CacheRead, CacheWrite, Total float64 }
type ToolSchema struct {
	Name, Description string
	Parameters        json.RawMessage
}
type Context struct {
	SystemPrompt *string
	Messages     []Message
	Tools        []ToolSchema
}
