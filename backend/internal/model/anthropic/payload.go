// Package anthropic implements the Anthropic Messages request and SSE contract.
package anthropic

import (
	"encoding/json"
	"fmt"
	"net/http"
	"strings"
	"unicode/utf8"

	"rozsa/backend/internal/model/types"
)

type compatibility struct {
	eager, longCache, affinity, toolCache, adaptive bool
}

func resolveCompatibility(model types.Model) (compatibility, error) {
	fireworks := model.Provider == "fireworks"
	compat := compatibility{!fireworks, !fireworks, fireworks || model.Provider == "cloudflare-ai-gateway" && strings.Contains(model.BaseURL, "anthropic"), !fireworks, false}
	var values map[string]json.RawMessage
	if len(model.Compat) != 0 {
		if err := validateJSONStrings(model.Compat); err != nil {
			return compat, err
		}
		if err := json.Unmarshal(model.Compat, &values); err != nil {
			return compat, fmt.Errorf("invalid model compatibility: %w", err)
		}
	}
	for key, target := range map[string]*bool{
		"supportsEagerToolInputStreaming": &compat.eager,
		"supportsLongCacheRetention":      &compat.longCache,
		"sendSessionAffinityHeaders":      &compat.affinity,
		"supportsCacheControlOnTools":     &compat.toolCache,
		"forceAdaptiveThinking":           &compat.adaptive,
	} {
		// Rust recognizes only boolean overrides; other values are metadata.
		if value := values[key]; string(value) == "true" || string(value) == "false" {
			*target = string(value) == "true"
		}
	}
	return compat, nil
}

func buildPayload(model types.Model, context types.Context, options types.SimpleStreamOptions, oauth bool, compat compatibility) ([]byte, error) {
	cache := map[string]any{"type": "ephemeral"}
	if options.Base.CacheRetention == types.CacheLong && compat.longCache {
		cache["ttl"] = "1h"
	}
	maxTokens := model.MaxTokens
	if options.Base.MaxTokens != nil {
		maxTokens = *options.Base.MaxTokens
	}
	messages, err := convertMessages(context.Messages, model, oauth, cache)
	if err != nil {
		return nil, err
	}
	payload := map[string]any{"model": model.ID, "messages": messages, "max_tokens": maxTokens, "stream": true}
	var system []map[string]any
	if oauth {
		system = append(system, map[string]any{"type": "text", "text": "You are Claude Code, Anthropic's official CLI for Claude.", "cache_control": cache})
	}
	if context.SystemPrompt != nil {
		system = append(system, map[string]any{"type": "text", "text": *context.SystemPrompt, "cache_control": cache})
	}
	if len(system) > 0 {
		payload["system"] = system
	}
	enabled := model.Reasoning && options.Reasoning != nil && *options.Reasoning != types.ThinkingOff
	if options.Base.Temperature != nil && !enabled {
		payload["temperature"] = *options.Base.Temperature
	}
	if len(context.Tools) > 0 {
		tools := make([]map[string]any, 0, len(context.Tools))
		for i, tool := range context.Tools {
			if err := validateJSONStrings(tool.Parameters); err != nil {
				return nil, fmt.Errorf("tool %q parameters: %w", tool.Name, err)
			}
			var schema map[string]json.RawMessage
			if err := json.Unmarshal(tool.Parameters, &schema); err != nil {
				return nil, fmt.Errorf("tool %q parameters: %w", tool.Name, err)
			}
			properties, required := schema["properties"], schema["required"]
			if properties == nil {
				properties = json.RawMessage(`{}`)
			}
			if required == nil {
				required = json.RawMessage(`[]`)
			}
			name := tool.Name
			if oauth {
				name = claudeCodeName(name)
			}
			converted := map[string]any{"name": name, "description": tool.Description, "input_schema": map[string]any{"type": "object", "properties": properties, "required": required}}
			if compat.eager {
				converted["eager_input_streaming"] = true
			}
			if i == len(context.Tools)-1 && compat.toolCache {
				converted["cache_control"] = cache
			}
			tools = append(tools, converted)
		}
		payload["tools"] = tools
	}
	if enabled {
		if compat.adaptive {
			payload["thinking"] = map[string]any{"type": "adaptive", "display": "summarized"}
			effort, include := string(*options.Reasoning), true
			if mapped, exists := model.ThinkingEffortMap[*options.Reasoning]; exists {
				if mapped == nil {
					include = false
				} else {
					effort = *mapped
				}
			}
			if include {
				payload["output_config"] = map[string]any{"effort": effort}
			}
		} else {
			budget := uint64(1024)
			choices := []types.ThinkingEffort{*options.Reasoning}
			if *options.Reasoning == types.ThinkingMax {
				choices = append(choices, types.ThinkingXHigh, types.ThinkingHigh)
			}
			if *options.Reasoning == types.ThinkingXHigh {
				choices = append(choices, types.ThinkingHigh)
			}
			for _, effort := range choices {
				if value, ok := options.ThinkingEffortBudgets[effort]; ok {
					budget = value
					break
				}
			}
			payload["thinking"] = map[string]any{"type": "enabled", "budget_tokens": budget, "display": "summarized"}
		}
	} else if model.Reasoning && options.Reasoning != nil && *options.Reasoning == types.ThinkingOff {
		payload["thinking"] = map[string]any{"type": "disabled"}
	}
	if len(options.Base.Metadata) > 0 {
		if err := validateJSONStrings(options.Base.Metadata); err != nil {
			return nil, err
		}
		var metadata map[string]json.RawMessage
		if err := json.Unmarshal(options.Base.Metadata, &metadata); err != nil {
			return nil, fmt.Errorf("invalid metadata: %w", err)
		}
		var userID string
		if value := metadata["user_id"]; len(value) > 0 && string(value) != "null" && json.Unmarshal(value, &userID) == nil {
			payload["metadata"] = map[string]any{"user_id": userID}
		}
	}
	if len(options.ToolChoice) > 0 {
		payload["tool_choice"] = options.ToolChoice
	}
	if err := validateJSONStrings(payload); err != nil {
		return nil, err
	}
	return json.Marshal(payload)
}

func convertMessages(messages []types.Message, model types.Model, oauth bool, cache map[string]any) ([]map[string]any, error) {
	images := false
	for _, modality := range model.Input {
		images = images || modality == "image"
	}
	result := make([]map[string]any, 0, len(messages))
	for i := 0; i < len(messages); i++ {
		role := "user"
		var blocks []map[string]any
		switch message := messages[i].(type) {
		case types.UserMessage:
			switch content := message.Content.(type) {
			case types.UserText:
				if strings.TrimSpace(string(content)) != "" {
					blocks = []map[string]any{{"type": "text", "text": string(content)}}
				}
			case types.UserBlocks:
				blocks = userBlocks(content, images, "(image omitted: model does not support images)")
			default:
				return nil, fmt.Errorf("unsupported user content %T", content)
			}
		case types.AssistantMessage:
			role = "assistant"
			blocks = assistantBlocks(message.Content, oauth)
		case types.ToolResultMessage:
			for {
				hasImages := false
				texts := []string{}
				for _, block := range message.Content {
					switch block := block.(type) {
					case types.ImageBlock:
						hasImages = true
					case types.TextBlock:
						texts = append(texts, block.Text)
					}
				}
				var content any = strings.Join(texts, "\n")
				if hasImages {
					content = userBlocks(message.Content, images, "(tool image omitted: model does not support images)")
				}
				blocks = append(blocks, map[string]any{"type": "tool_result", "tool_use_id": message.ToolCallID, "content": content, "is_error": message.IsError})
				if i+1 >= len(messages) {
					break
				}
				next, ok := messages[i+1].(types.ToolResultMessage)
				if !ok {
					break
				}
				message = next
				i++
			}
		default:
			return nil, fmt.Errorf("unsupported model message %T", message)
		}
		if len(blocks) > 0 {
			result = append(result, map[string]any{"role": role, "content": blocks})
		}
	}
	if len(result) > 0 {
		last := result[len(result)-1]
		if last["role"] == "user" {
			blocks := last["content"].([]map[string]any)
			blocks[len(blocks)-1]["cache_control"] = cache
		}
	}
	return result, nil
}

func assistantBlocks(content []types.ContentBlock, oauth bool) []map[string]any {
	var blocks []map[string]any
	for _, block := range content {
		switch block := block.(type) {
		case types.TextBlock:
			if strings.TrimSpace(block.Text) != "" {
				blocks = append(blocks, map[string]any{"type": "text", "text": block.Text})
			}
		case types.ThinkingBlock:
			if block.Redacted {
				if block.Signature != nil {
					blocks = append(blocks, map[string]any{"type": "redacted_thinking", "data": *block.Signature})
				}
				continue
			}
			if strings.TrimSpace(block.Thinking) == "" {
				continue
			}
			if block.Signature != nil && strings.TrimSpace(*block.Signature) != "" {
				blocks = append(blocks, map[string]any{"type": "thinking", "thinking": block.Thinking, "signature": *block.Signature})
			} else {
				blocks = append(blocks, map[string]any{"type": "text", "text": block.Thinking})
			}
		case types.ToolCallBlock:
			name := block.Name
			if oauth {
				name = claudeCodeName(name)
			}
			blocks = append(blocks, map[string]any{"type": "tool_use", "id": block.ID, "name": name, "input": block.Arguments})
		}
	}
	return blocks
}

func userBlocks(blocks []types.ContentBlock, images bool, placeholder string) []map[string]any {
	result := make([]map[string]any, 0, len(blocks))
	previousPlaceholder := false
	onlyImages := true
	for _, block := range blocks {
		switch block := block.(type) {
		case types.TextBlock:
			if strings.TrimSpace(block.Text) != "" {
				result = append(result, map[string]any{"type": "text", "text": block.Text})
				previousPlaceholder = false
				onlyImages = false
			}
		case types.ImageBlock:
			if images {
				result = append(result, map[string]any{"type": "image", "source": map[string]any{"type": "base64", "media_type": block.MimeType, "data": block.Data}})
				previousPlaceholder = false
			} else if !previousPlaceholder {
				result = append(result, map[string]any{"type": "text", "text": placeholder})
				previousPlaceholder = true
				onlyImages = false
			}
		}
	}
	if len(result) > 0 && onlyImages {
		result = append([]map[string]any{{"type": "text", "text": "(see attached image)"}}, result...)
	}
	return result
}

func buildHeaders(model types.Model, options types.SimpleStreamOptions, oauth bool, compat compatibility, context types.Context) http.Header {
	headers := make(http.Header)
	headers.Set("content-type", "application/json")
	headers.Set("accept", "application/json")
	headers.Set("anthropic-version", "2023-06-01")
	betas := []string{}
	if len(context.Tools) > 0 && !compat.eager {
		betas = append(betas, "fine-grained-tool-streaming-2025-05-14")
	}
	if !compat.adaptive {
		betas = append(betas, "interleaved-thinking-2025-05-14")
	}
	if oauth {
		betas = append(betas, "claude-code-20250219", "oauth-2025-04-20")
		headers.Set("user-agent", "claude-cli/2.1.75")
		headers.Set("x-app", "cli")
		headers.Set("anthropic-dangerous-direct-browser-access", "true")
	}
	if len(betas) > 0 {
		headers.Set("anthropic-beta", strings.Join(betas, ","))
	}
	if compat.affinity && options.Base.SessionID != nil {
		headers.Set("x-session-affinity", *options.Base.SessionID)
	}
	for key, value := range model.Headers {
		headers.Set(key, value)
	}
	for key, value := range options.Base.Headers {
		headers.Set(key, value)
	}
	return headers
}

func claudeCodeName(name string) string {
	for _, known := range []string{"Read", "Write", "Edit", "Bash", "Grep", "Glob", "AskUserQuestion", "EnterPlanMode", "ExitPlanMode", "KillShell", "NotebookEdit", "Skill", "Task", "TaskOutput", "TodoWrite", "WebFetch", "WebSearch"} {
		if strings.EqualFold(name, known) {
			return known
		}
	}
	return name
}

func normalizeToolID(id string) string {
	var result strings.Builder
	for _, char := range id {
		if char >= 'a' && char <= 'z' || char >= 'A' && char <= 'Z' || char >= '0' && char <= '9' || char == '_' || char == '-' {
			result.WriteRune(char)
			if result.Len() == 64 {
				break
			}
		}
	}
	return result.String()
}

// encoding/json replaces invalid UTF-8. Reject it before encoding so requests
// cannot silently change user text or tool JSON.
func validateJSONStrings(value any) error {
	switch value := value.(type) {
	case string:
		if !utf8.ValidString(value) {
			return fmt.Errorf("request contains invalid UTF-8")
		}
	case json.RawMessage:
		if !utf8.Valid(value) || !json.Valid(value) {
			return fmt.Errorf("request contains invalid JSON or UTF-8")
		}
	case map[string]any:
		for key, item := range value {
			if err := validateJSONStrings(key); err != nil {
				return err
			}
			if err := validateJSONStrings(item); err != nil {
				return err
			}
		}
	case []map[string]any:
		for _, item := range value {
			if err := validateJSONStrings(item); err != nil {
				return err
			}
		}
	}
	return nil
}
