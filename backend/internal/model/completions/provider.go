// Package completions implements OpenAI-compatible Chat Completions over HTTP/SSE.
package completions

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net"
	"net/http"
	"net/url"
	"strconv"
	"strings"
	"time"

	"rozsa/backend/internal/model/types"
)

type Provider struct{ Client *http.Client }

func NewProvider(client *http.Client) *Provider {
	if client == nil {
		client = &http.Client{Timeout: 300 * time.Second}
	}
	return &Provider{Client: client}
}
func (*Provider) API() types.API { return types.APIOpenAICompletions }

func (provider *Provider) StreamSimple(ctx context.Context, model types.Model, input types.Context, options types.SimpleStreamOptions, emit func(types.StreamEvent)) (err error) {
	if emit == nil {
		return errors.New("OpenAI Completions stream requires an event emitter")
	}
	output := types.AssistantMessage{API: types.APIOpenAICompletions, Provider: model.Provider, Model: model.ID, Timestamp: time.Now().UnixMilli(), Content: []types.ContentBlock{}}
	defer func() {
		if err == nil {
			err = ctx.Err()
		}
		if err != nil {
			reason := types.Error
			if errors.Is(err, context.Canceled) {
				reason = types.Aborted
			}
			failure := types.AssistantMessage{API: output.API, Provider: output.Provider, Model: output.Model, Timestamp: output.Timestamp, Content: []types.ContentBlock{}, StopReason: reason}
			message := err.Error()
			failure.ErrorMessage = &message
			emit(types.StreamError{Reason: reason, Error: failure})
			return
		}
		emit(types.Done{Reason: output.StopReason, Message: output})
	}()
	if provider == nil || provider.Client == nil {
		return errors.New("OpenAI Completions provider requires HTTP client")
	}
	if err = ctx.Err(); err != nil {
		return err
	}
	if options.Base.Timeout > 0 {
		var cancel context.CancelFunc
		ctx, cancel = context.WithTimeout(ctx, options.Base.Timeout)
		defer cancel()
	}
	endpoint, err := resolveEndpoint(model.BaseURL)
	if err != nil {
		return err
	}
	body, compat, err := buildPayload(model, input, options)
	if err != nil {
		return err
	}
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, endpoint, bytes.NewReader(body))
	if err != nil {
		return fmt.Errorf("OpenAI Completions request: %w", err)
	}
	req.Header.Set("Accept", "text/event-stream")
	req.Header.Set("Content-Type", "application/json")
	for k, v := range model.Headers {
		req.Header.Set(k, v)
	}
	for k, v := range options.Base.Headers {
		req.Header.Set(k, v)
	}
	if options.Base.APIKey != "" && req.Header.Get("Authorization") == "" {
		if model.Provider == "cloudflare-ai-gateway" || strings.Contains(strings.ToLower(model.BaseURL), "gateway.ai.cloudflare.com") {
			req.Header.Set("cf-aig-authorization", "Bearer "+options.Base.APIKey)
		} else {
			req.Header.Set("Authorization", "Bearer "+options.Base.APIKey)
		}
	}
	if model.Provider == "cloudflare-ai-gateway" || strings.Contains(strings.ToLower(model.BaseURL), "gateway.ai.cloudflare.com") {
		if req.Header.Get("cf-aig-authorization") == "" && options.Base.APIKey != "" {
			req.Header.Set("cf-aig-authorization", "Bearer "+options.Base.APIKey)
		}
		req.Header.Del("Authorization")
	}
	if model.Provider == "github-copilot" {
		initiator := "agent"
		if len(input.Messages) == 0 {
			initiator = "user"
		} else if _, ok := input.Messages[len(input.Messages)-1].(types.UserMessage); ok {
			initiator = "user"
		}
		if req.Header.Get("X-Initiator") == "" {
			req.Header.Set("X-Initiator", initiator)
		}
		if req.Header.Get("Openai-Intent") == "" {
			req.Header.Set("Openai-Intent", "conversation-edits")
		}
		if hasImage(input.Messages) && req.Header.Get("Copilot-Vision-Request") == "" {
			req.Header.Set("Copilot-Vision-Request", "true")
		}
	}
	if options.Base.SessionID != nil && compat.affinity {
		for _, k := range []string{"session_id", "x-client-request-id", "x-session-affinity"} {
			if req.Header.Get(k) == "" {
				req.Header.Set(k, *options.Base.SessionID)
			}
		}
	}
	client := *provider.Client
	if options.Base.Timeout > 0 {
		client.Timeout = 0
	}
	var response *http.Response
	for attempt := 0; ; attempt++ {
		attemptRequest := req.Clone(ctx)
		attemptRequest.Body, err = req.GetBody()
		if err != nil {
			return fmt.Errorf("rebuild OpenAI Completions request: %w", err)
		}
		response, err = client.Do(attemptRequest)
		if err != nil {
			var networkError net.Error
			if attempt < 2 && !errors.Is(err, context.Canceled) && (errors.As(err, &networkError) && (networkError.Timeout() || networkError.Temporary())) {
				if waitErr := retryWait(ctx, attempt, ""); waitErr != nil {
					return waitErr
				}
				continue
			}
			return fmt.Errorf("HTTP request failed: %w", err)
		}
		status := response.StatusCode
		if status != http.StatusRequestTimeout && status != http.StatusConflict && status != http.StatusTooManyRequests && status < 500 || attempt >= 2 {
			break
		}
		delay := response.Header.Get("Retry-After")
		_ = response.Body.Close()
		if waitErr := retryWait(ctx, attempt, delay); waitErr != nil {
			return waitErr
		}
	}
	defer response.Body.Close()
	if response.StatusCode < 200 || response.StatusCode >= 300 {
		data, readErr := io.ReadAll(response.Body)
		if readErr != nil {
			return fmt.Errorf("provider HTTP error (%d), reading body: %w", response.StatusCode, readErr)
		}
		return fmt.Errorf("Provider HTTP error (%d): %s", response.StatusCode, string(data))
	}
	emit(types.Start{Partial: clone(output)})
	return consume(ctx, response.Body, model, &output, emit)
}

func hasImage(messages []types.Message) bool {
	for _, message := range messages {
		switch m := message.(type) {
		case types.UserMessage:
			if blocks, ok := m.Content.(types.UserBlocks); ok {
				for _, block := range blocks {
					if _, ok := block.(types.ImageBlock); ok {
						return true
					}
				}
			}
		case types.ToolResultMessage:
			for _, block := range m.Content {
				if _, ok := block.(types.ImageBlock); ok {
					return true
				}
			}
		}
	}
	return false
}

func retryWait(ctx context.Context, attempt int, header string) error {
	delay := 250 * time.Millisecond * time.Duration(1<<attempt)
	if seconds, err := strconv.ParseUint(header, 10, 32); err == nil {
		delay = time.Duration(seconds) * time.Second
	}
	timer := time.NewTimer(delay)
	defer timer.Stop()
	select {
	case <-ctx.Done():
		return ctx.Err()
	case <-timer.C:
		return nil
	}
}

type compatibility struct {
	maxTokens                                                                                                                        string
	usage, store, strict, devRole, requiresToolName, assistantAfterTool, thinkingText, reasoningAssistant, affinity, reasoningEffort bool
	longCache, zaiToolStream                                                                                                         bool
	cacheControlFormat, thinkingFormat                                                                                               string
	openRouterRouting, vercelGatewayRouting                                                                                          json.RawMessage
	format                                                                                                                           string
}

func resolveCompat(model types.Model) compatibility {
	p, u := strings.ToLower(model.Provider), strings.ToLower(model.BaseURL)
	nonstandard := strings.Contains(p, "cerebras") || strings.Contains(p, "xai") || strings.Contains(p, "together") || strings.Contains(u, "chutes.ai") || strings.Contains(p, "deepseek") || strings.Contains(p, "zai") || strings.Contains(p, "moonshot") || strings.Contains(p, "nvidia") || strings.Contains(u, "api.cloudflare.com") || strings.Contains(u, "gateway.ai.cloudflare.com")
	c := compatibility{maxTokens: "max_completion_tokens", usage: true, store: !nonstandard, strict: !strings.Contains(p, "moonshot") && !strings.Contains(p, "together"), devRole: !nonstandard, reasoningEffort: true, format: "openai"}
	c.longCache = !strings.Contains(p, "together") && !strings.Contains(u, "api.cloudflare.com") && !strings.Contains(u, "gateway.ai.cloudflare.com")
	if strings.Contains(u, "chutes.ai") || strings.Contains(p, "moonshot") || strings.Contains(p, "together") || strings.Contains(p, "nvidia") || strings.Contains(u, "gateway.ai.cloudflare.com") {
		c.maxTokens = "max_tokens"
	}
	if strings.Contains(p, "xai") || strings.Contains(p, "zai") || strings.Contains(p, "moonshot") || strings.Contains(p, "together") || strings.Contains(p, "nvidia") || strings.Contains(u, "gateway.ai.cloudflare.com") {
		c.reasoningEffort = false
	}
	if strings.Contains(p, "deepseek") {
		c.format = "deepseek"
		c.reasoningAssistant = true
	}
	if strings.Contains(p, "zai") {
		c.format = "zai"
	}
	if strings.Contains(p, "together") {
		c.format = "together"
	}
	if strings.Contains(p, "openrouter") || strings.Contains(u, "openrouter.ai") {
		c.format = "openrouter"
	}
	var values map[string]json.RawMessage
	_ = json.Unmarshal(model.Compat, &values)
	bools := map[string]*bool{"supportsStore": &c.store, "supportsDeveloperRole": &c.devRole, "supportsReasoningEffort": &c.reasoningEffort, "supportsUsageInStreaming": &c.usage, "requiresToolResultName": &c.requiresToolName, "requiresAssistantAfterToolResult": &c.assistantAfterTool, "requiresThinkingAsText": &c.thinkingText, "requiresReasoningContentOnAssistantMessages": &c.reasoningAssistant, "supportsStrictMode": &c.strict, "sendSessionAffinityHeaders": &c.affinity}
	bools["supportsLongCacheRetention"] = &c.longCache
	bools["zaiToolStream"] = &c.zaiToolStream
	for k, target := range bools {
		var v bool
		if json.Unmarshal(values[k], &v) == nil {
			*target = v
		}
	}
	if strings.Contains(strings.ToLower(string(values["maxTokensField"])), "maxtokens") {
		c.maxTokens = "max_tokens"
	}
	if len(values["thinkingFormat"]) > 0 {
		_ = json.Unmarshal(values["thinkingFormat"], &c.format)
	}
	if len(values["cacheControlFormat"]) > 0 {
		_ = json.Unmarshal(values["cacheControlFormat"], &c.cacheControlFormat)
	}
	c.openRouterRouting = values["openRouterRouting"]
	c.vercelGatewayRouting = values["vercelGatewayRouting"]
	return c
}
func resolveEndpoint(base string) (string, error) {
	base = strings.TrimRight(strings.TrimSpace(base), "/")
	if base == "" {
		base = "https://api.openai.com/v1"
	}
	if !strings.HasSuffix(base, "/chat/completions") {
		base += "/chat/completions"
	}
	u, err := url.Parse(base)
	if err != nil || u.Host == "" || (u.Scheme != "http" && u.Scheme != "https") {
		return "", errors.New("invalid OpenAI Completions endpoint: expected an http(s) URL")
	}
	return u.String(), nil
}

func buildPayload(model types.Model, input types.Context, options types.SimpleStreamOptions) ([]byte, compatibility, error) {
	c := resolveCompat(model)
	msgs := make([]any, 0, len(input.Messages)+1)
	if input.SystemPrompt != nil && *input.SystemPrompt != "" {
		role := "system"
		if model.Reasoning && c.devRole {
			role = "developer"
		}
		msgs = append(msgs, map[string]any{"role": role, "content": *input.SystemPrompt})
	}
	lastTool := false
	for _, raw := range input.Messages {
		if c.assistantAfterTool && lastTool {
			if _, ok := raw.(types.UserMessage); ok {
				msgs = append(msgs, map[string]any{"role": "assistant", "content": "I have processed the tool results."})
			}
		}
		switch m := raw.(type) {
		case types.UserMessage:
			contentValue := any("")
			switch content := m.Content.(type) {
			case types.UserText:
				contentValue = string(content)
			case types.UserBlocks:
				parts := []any{}
				for _, b := range content {
					switch v := b.(type) {
					case types.TextBlock:
						parts = append(parts, map[string]any{"type": "text", "text": v.Text})
					case types.ImageBlock:
						if supportsImage(model) {
							parts = append(parts, map[string]any{"type": "image_url", "image_url": map[string]string{"url": "data:" + v.MimeType + ";base64," + v.Data}})
						}
					}
				}
				if len(parts) > 0 {
					contentValue = parts
				} else {
					continue
				}
			}
			msgs = append(msgs, map[string]any{"role": "user", "content": contentValue})
			lastTool = false
		case types.AssistantMessage:
			if msg := assistantMessage(m, c); msg != nil {
				msgs = append(msgs, msg)
			}
			lastTool = false
		case types.ToolResultMessage:
			text := ""
			for _, b := range m.Content {
				if t, ok := b.(types.TextBlock); ok {
					if text != "" {
						text += "\n"
					}
					text += t.Text
				}
			}
			if text == "" {
				text = "(see attached image)"
			}
			msg := map[string]any{"role": "tool", "content": text, "tool_call_id": m.ToolCallID}
			if c.requiresToolName {
				msg["name"] = m.ToolName
			}
			msgs = append(msgs, msg)
			lastTool = true
		}
	}
	payload := map[string]any{"model": model.ID, "messages": msgs, "stream": true}
	if c.usage {
		payload["stream_options"] = map[string]any{"include_usage": true}
	}
	if c.store {
		payload["store"] = false
	}
	if len(input.Tools) > 0 {
		tools := make([]any, 0, len(input.Tools))
		for _, tool := range input.Tools {
			fn := map[string]any{"name": tool.Name, "description": tool.Description, "parameters": json.RawMessage(tool.Parameters)}
			if c.strict {
				fn["strict"] = false
			}
			tools = append(tools, map[string]any{"type": "function", "function": fn})
		}
		payload["tools"] = tools
		if c.zaiToolStream {
			payload["tool_stream"] = true
		}
	} else if hasToolHistory(input.Messages) {
		payload["tools"] = []any{}
	}
	if options.Base.MaxTokens != nil {
		payload[c.maxTokens] = *options.Base.MaxTokens
	} else if model.MaxTokens > 0 {
		payload[c.maxTokens] = model.MaxTokens
	}
	if options.Base.Temperature != nil {
		payload["temperature"] = *options.Base.Temperature
	}
	if len(options.ToolChoice) > 0 {
		payload["tool_choice"] = json.RawMessage(options.ToolChoice)
	}
	var effort *types.ThinkingEffort
	if model.Reasoning && options.Reasoning != nil && *options.Reasoning != types.ThinkingOff {
		effort = options.Reasoning
	}
	if c.reasoningEffort && effort != nil {
		payload["reasoning_effort"] = string(*effort)
	}
	if c.format == "deepseek" && model.Reasoning {
		kind := "disabled"
		if effort != nil {
			kind = "enabled"
		}
		payload["thinking"] = map[string]string{"type": kind}
	}
	if c.format == "zai" && model.Reasoning {
		payload["enable_thinking"] = effort != nil
	}
	if c.format == "together" && model.Reasoning {
		payload["reasoning"] = map[string]any{"enabled": effort != nil}
	}
	if c.format == "qwen" && model.Reasoning {
		payload["enable_thinking"] = effort != nil
	}
	if c.format == "qwen-chat-template" && model.Reasoning {
		payload["chat_template_kwargs"] = map[string]any{"enable_thinking": effort != nil, "preserve_thinking": true}
	}
	if options.Base.CacheRetention != types.CacheDefault {
		if strings.Contains(strings.ToLower(model.BaseURL), "api.openai.com") && options.Base.SessionID != nil {
			key := *options.Base.SessionID
			if len(key) > 64 {
				key = key[:64]
			}
			payload["prompt_cache_key"] = key
		}
		if options.Base.CacheRetention == types.CacheLong && c.longCache {
			payload["prompt_cache_retention"] = "24h"
		}
		if c.cacheControlFormat == "anthropic" {
			cache := map[string]any{"type": "ephemeral"}
			if options.Base.CacheRetention == types.CacheLong && c.longCache {
				cache["ttl"] = "1h"
			}
			applyCacheControl(payload, cache)
		}
	}
	if strings.Contains(strings.ToLower(model.BaseURL), "openrouter.ai") && len(c.openRouterRouting) > 0 {
		payload["provider"] = json.RawMessage(c.openRouterRouting)
	}
	if strings.Contains(strings.ToLower(model.BaseURL), "ai-gateway.vercel.sh") && len(c.vercelGatewayRouting) > 0 {
		var route map[string]json.RawMessage
		if json.Unmarshal(c.vercelGatewayRouting, &route) == nil {
			gateway := map[string]json.RawMessage{}
			for _, k := range []string{"only", "order"} {
				if v := route[k]; len(v) > 0 {
					gateway[k] = v
				}
			}
			if len(gateway) > 0 {
				payload["providerOptions"] = map[string]any{"gateway": gateway}
			}
		}
	}
	data, err := json.Marshal(payload)
	return data, c, err
}

func hasToolHistory(messages []types.Message) bool {
	for _, m := range messages {
		switch v := m.(type) {
		case types.ToolResultMessage:
			return true
		case types.AssistantMessage:
			for _, b := range v.Content {
				if _, ok := b.(types.ToolCallBlock); ok {
					return true
				}
			}
		}
	}
	return false
}
func supportsImage(model types.Model) bool {
	for _, kind := range model.Input {
		if kind == "image" {
			return true
		}
	}
	return false
}
func applyCacheControl(payload map[string]any, cache map[string]any) {
	messages, _ := payload["messages"].([]any)
	for _, raw := range messages {
		m, _ := raw.(map[string]any)
		role, _ := m["role"].(string)
		if role == "system" || role == "developer" {
			if applyMessageCache(m, cache) {
				break
			}
		}
	}
	if tools, ok := payload["tools"].([]any); ok && len(tools) > 0 {
		if tool, ok := tools[len(tools)-1].(map[string]any); ok {
			tool["cache_control"] = cache
		}
	}
	for i := len(messages) - 1; i >= 0; i-- {
		m, _ := messages[i].(map[string]any)
		role, _ := m["role"].(string)
		if (role == "user" || role == "assistant") && applyMessageCache(m, cache) {
			break
		}
	}
}
func applyMessageCache(message map[string]any, cache map[string]any) bool {
	content, ok := message["content"].(string)
	if ok && content != "" {
		message["content"] = []any{map[string]any{"type": "text", "text": content, "cache_control": cache}}
		return true
	}
	return false
}
func assistantMessage(m types.AssistantMessage, c compatibility) map[string]any {
	text, thinking := "", ""
	calls := []any{}
	for _, b := range m.Content {
		switch v := b.(type) {
		case types.TextBlock:
			text += v.Text
		case types.ThinkingBlock:
			thinking += v.Thinking
		case types.ToolCallBlock:
			args := v.Arguments
			if !json.Valid(args) {
				args = []byte("{}")
			}
			calls = append(calls, map[string]any{"id": v.ID, "type": "function", "function": map[string]any{"name": v.Name, "arguments": string(args)}})
		}
	}
	if text == "" && thinking == "" && len(calls) == 0 {
		return nil
	}
	var content any
	if text != "" {
		content = text
	}
	msg := map[string]any{"role": "assistant", "content": content}
	if thinking != "" {
		if c.thinkingText {
			if text != "" {
				msg["content"] = thinking + "\n\n" + text
			} else {
				msg["content"] = thinking
			}
		} else {
			msg["reasoning_content"] = thinking
		}
	}
	if len(calls) > 0 {
		msg["tool_calls"] = calls
	}
	if c.reasoningAssistant && m.API != "" && msg["reasoning_content"] == nil {
		msg["reasoning_content"] = ""
	}
	return msg
}
func clone(m types.AssistantMessage) types.AssistantMessage {
	m.Content = append([]types.ContentBlock(nil), m.Content...)
	if m.ResponseID != nil {
		v := *m.ResponseID
		m.ResponseID = &v
	}
	if m.ResponseModel != nil {
		v := *m.ResponseModel
		m.ResponseModel = &v
	}
	return m
}
