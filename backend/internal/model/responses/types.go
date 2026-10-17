package responses

import (
	"encoding/json"
	"fmt"
	"strings"

	"rozsa/backend/internal/model/types"
)

// Request and item types model the Responses API separately from Chat
// Completions. The protocol is selected by Model.API; Provider remains an
// independent service identity.
type Request struct {
	Model             string         `json:"model"`
	Instructions      *string        `json:"instructions,omitempty"`
	Input             []InputItem    `json:"input"`
	Tools             []FunctionTool `json:"tools,omitempty"`
	ToolChoice        string         `json:"tool_choice"`
	ParallelToolCalls bool           `json:"parallel_tool_calls"`
	Reasoning         *Reasoning     `json:"reasoning,omitempty"`
	Store             bool           `json:"store"`
	Stream            bool           `json:"stream"`
	Include           []string       `json:"include"`
	ServiceTier       *string        `json:"service_tier,omitempty"`
	PromptCacheKey    *string        `json:"prompt_cache_key,omitempty"`
}

type Reasoning struct {
	Effort  *string `json:"effort,omitempty"`
	Summary *string `json:"summary,omitempty"`
}

type InputItem struct {
	Type             string        `json:"type"`
	ID               *string       `json:"id,omitempty"`
	Role             string        `json:"role,omitempty"`
	Content          []ContentItem `json:"content,omitempty"`
	Name             string        `json:"name,omitempty"`
	Arguments        string        `json:"arguments,omitempty"`
	CallID           string        `json:"call_id,omitempty"`
	Output           string        `json:"output,omitempty"`
	Summary          []SummaryPart `json:"summary,omitempty"`
	EncryptedContent *string       `json:"encrypted_content,omitempty"`
}

func (item InputItem) MarshalJSON() ([]byte, error) {
	switch item.Type {
	case "message":
		return json.Marshal(struct {
			Type    string        `json:"type"`
			ID      *string       `json:"id,omitempty"`
			Role    string        `json:"role"`
			Content []ContentItem `json:"content"`
		}{item.Type, item.ID, item.Role, item.Content})
	case "function_call":
		return json.Marshal(struct {
			Type      string  `json:"type"`
			ID        *string `json:"id,omitempty"`
			Name      string  `json:"name"`
			Arguments string  `json:"arguments"`
			CallID    string  `json:"call_id"`
		}{item.Type, item.ID, item.Name, item.Arguments, item.CallID})
	case "function_call_output":
		return json.Marshal(struct {
			Type   string  `json:"type"`
			ID     *string `json:"id,omitempty"`
			CallID string  `json:"call_id"`
			Output string  `json:"output"`
		}{item.Type, item.ID, item.CallID, item.Output})
	case "reasoning":
		return json.Marshal(struct {
			Type             string        `json:"type"`
			ID               *string       `json:"id,omitempty"`
			Summary          []SummaryPart `json:"summary"`
			EncryptedContent *string       `json:"encrypted_content,omitempty"`
		}{item.Type, item.ID, item.Summary, item.EncryptedContent})
	default:
		return nil, fmt.Errorf("unsupported Responses input item type %q", item.Type)
	}
}

type ContentItem struct {
	Type     string  `json:"type"`
	Text     string  `json:"text,omitempty"`
	ImageURL string  `json:"image_url,omitempty"`
	Detail   *string `json:"detail,omitempty"`
}

func (item ContentItem) MarshalJSON() ([]byte, error) {
	switch item.Type {
	case "input_text", "output_text":
		return json.Marshal(struct {
			Type string `json:"type"`
			Text string `json:"text"`
		}{item.Type, item.Text})
	case "input_image":
		return json.Marshal(struct {
			Type     string  `json:"type"`
			ImageURL string  `json:"image_url"`
			Detail   *string `json:"detail,omitempty"`
		}{item.Type, item.ImageURL, item.Detail})
	default:
		return nil, fmt.Errorf("unsupported Responses content item type %q", item.Type)
	}
}

type SummaryPart struct {
	Type string `json:"type"`
	Text string `json:"text"`
}

type FunctionTool struct {
	Type        string          `json:"type"`
	Name        string          `json:"name"`
	Description string          `json:"description"`
	Parameters  json.RawMessage `json:"parameters"`
	Strict      bool            `json:"strict"`
}

// buildRequest preserves Responses' item-oriented history and maps the system
// prompt to a developer message. It does not apply Chat Completions roles.
func buildRequest(model types.Model, input types.Context, options types.SimpleStreamOptions) (Request, error) {
	items, err := convertMessages(input.Messages, input.SystemPrompt)
	if err != nil {
		return Request{}, err
	}
	var tools []FunctionTool
	if len(input.Tools) != 0 {
		tools = make([]FunctionTool, 0, len(input.Tools))
		for _, tool := range input.Tools {
			parameters := append(json.RawMessage(nil), tool.Parameters...)
			if len(parameters) == 0 {
				parameters = json.RawMessage(`{}`)
			}
			if !json.Valid(parameters) {
				return Request{}, fmt.Errorf("invalid parameters for Responses function %q", tool.Name)
			}
			tools = append(tools, FunctionTool{Type: "function", Name: tool.Name, Description: tool.Description, Parameters: parameters})
		}
	}
	var reasoning *Reasoning
	if effort := options.Reasoning; effort != nil && *effort != types.ThinkingOff {
		value, supported := model.ThinkingEffortMap[*effort]
		if model.ThinkingEffortMap != nil && supported && value == nil {
			value = nil
		} else {
			mapped := string(*effort)
			if value != nil {
				mapped = *value
			}
			if *effort == types.ThinkingLow {
				mapped = "low"
			}
			summary := "auto"
			reasoning = &Reasoning{Effort: &mapped, Summary: &summary}
		}
	}
	return Request{
		Model: model.ID, Input: items, Tools: tools, ToolChoice: "auto",
		ParallelToolCalls: true, Reasoning: reasoning, Store: false, Stream: true,
		Include: []string{"reasoning.encrypted_content"}, PromptCacheKey: options.Base.SessionID,
	}, nil
}

func convertMessages(messages []types.Message, systemPrompt *string) ([]InputItem, error) {
	items := make([]InputItem, 0, len(messages)+1)
	if systemPrompt != nil && *systemPrompt != "" {
		items = append(items, InputItem{Type: "message", Role: "developer", Content: []ContentItem{{Type: "input_text", Text: *systemPrompt}}})
	}
	for _, message := range messages {
		switch message := message.(type) {
		case types.UserMessage:
			content := make([]ContentItem, 0)
			switch value := message.Content.(type) {
			case types.UserText:
				content = append(content, ContentItem{Type: "input_text", Text: string(value)})
			case types.UserBlocks:
				for _, block := range value {
					switch block := block.(type) {
					case types.TextBlock:
						content = append(content, ContentItem{Type: "input_text", Text: block.Text})
					case types.ImageBlock:
						content = append(content, ContentItem{Type: "input_image", ImageURL: "data:" + block.MimeType + ";base64," + block.Data})
					}
				}
			}
			if len(content) != 0 {
				items = append(items, InputItem{Type: "message", Role: "user", Content: content})
			}
		case types.AssistantMessage:
			for _, block := range message.Content {
				switch block := block.(type) {
				case types.TextBlock:
					items = append(items, InputItem{Type: "message", Role: "assistant", Content: []ContentItem{{Type: "output_text", Text: block.Text}}})
				case types.ThinkingBlock:
					var summary []SummaryPart
					if block.Thinking != "" {
						summary = []SummaryPart{{Type: "summary_text", Text: block.Thinking}}
					} else {
						summary = []SummaryPart{}
					}
					items = append(items, InputItem{Type: "reasoning", Summary: summary, EncryptedContent: block.Signature})
				case types.ToolCallBlock:
					if id := callID(block.ID); id != "" {
						args := string(block.Arguments)
						if len(block.Arguments) == 0 {
							args = "null"
						} else if !json.Valid(block.Arguments) {
							return nil, fmt.Errorf("invalid arguments for Responses function call %q", block.Name)
						}
						items = append(items, InputItem{Type: "function_call", Name: block.Name, Arguments: args, CallID: id})
					}
				}
			}
		case types.ToolResultMessage:
			if id := callID(message.ToolCallID); id != "" {
				output, err := contentToString(message.Content)
				if err != nil {
					return nil, err
				}
				items = append(items, InputItem{Type: "function_call_output", CallID: id, Output: output})
			}
		}
	}
	return items, nil
}

func callID(id string) string {
	value := strings.TrimSpace(strings.SplitN(id, "|", 2)[0])
	return value
}

func contentToString(blocks []types.ContentBlock) (string, error) {
	texts := make([]string, 0, len(blocks))
	for _, block := range blocks {
		if value, ok := block.(types.TextBlock); ok {
			texts = append(texts, value.Text)
		}
	}
	if len(texts) != 0 {
		return strings.Join(texts, "\n"), nil
	}
	if len(blocks) == 0 {
		return "", nil
	}
	encoded, err := json.Marshal(blocks)
	if err != nil {
		return "", fmt.Errorf("encode Responses tool result: %w", err)
	}
	return string(encoded), nil
}
