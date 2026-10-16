package anthropic

import (
	"bufio"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"strings"
	"unicode/utf8"

	"rozsa/backend/internal/model/types"
)

type rawUsage struct {
	Input      *uint64 `json:"input_tokens"`
	Output     *uint64 `json:"output_tokens"`
	CacheRead  *uint64 `json:"cache_read_input_tokens"`
	CacheWrite *uint64 `json:"cache_creation_input_tokens"`
}
type blockInfo struct {
	Type string `json:"type"`
	ID   string `json:"id"`
	Name string `json:"name"`
	Data string `json:"data"`
}
type deltaInfo struct {
	Type       string  `json:"type"`
	Text       string  `json:"text"`
	Thinking   string  `json:"thinking"`
	JSON       string  `json:"partial_json"`
	Signature  string  `json:"signature"`
	StopReason *string `json:"stop_reason"`
}
type eventData struct {
	Message *struct {
		ID    *string   `json:"id"`
		Usage *rawUsage `json:"usage"`
	} `json:"message"`
	Index *uint64    `json:"index"`
	Block *blockInfo `json:"content_block"`
	Delta *deltaInfo `json:"delta"`
	Usage *rawUsage  `json:"usage"`
}
type streamBlock struct {
	index  int
	kind   string
	json   strings.Builder
	closed bool
}

func consumeStream(ctx context.Context, input io.Reader, model types.Model, output *types.AssistantMessage, oauth bool, tools []types.ToolSchema, emit func(types.StreamEvent)) error {
	started, stopped := false, false
	blocks := map[uint64]*streamBlock{}
	handle := func(event, data string) error {
		if err := ctx.Err(); err != nil {
			return err
		}
		if event == "error" {
			return fmt.Errorf("Anthropic stream error: %s", data)
		}
		switch event {
		case "message_start", "content_block_start", "content_block_delta", "content_block_stop", "message_delta", "message_stop":
		default:
			return nil
		}
		if stopped {
			return errors.New("Anthropic event after message_stop")
		}
		if !utf8.ValidString(data) {
			return errors.New("Anthropic event contains invalid UTF-8")
		}
		var parsed eventData
		if err := json.Unmarshal([]byte(data), &parsed); err != nil {
			return fmt.Errorf("Failed to parse Anthropic %s event: %w", event, err)
		}
		if strings.TrimSpace(data) == "null" {
			return fmt.Errorf("Failed to parse Anthropic %s event: expected an object", event)
		}
		if event == "message_start" {
			if started || parsed.Message == nil {
				return errors.New("invalid Anthropic message_start")
			}
			started = true
			output.ResponseID = parsed.Message.ID
			updateUsage(model, &output.Usage, parsed.Message.Usage)
			return nil
		}
		if !started {
			return errors.New("Anthropic event before message_start")
		}
		switch event {
		case "content_block_start":
			if parsed.Index == nil || parsed.Block == nil || parsed.Block.Type == "" {
				return errors.New("invalid Anthropic content_block_start")
			}
			if _, exists := blocks[*parsed.Index]; exists {
				return errors.New("duplicate Anthropic content block index")
			}
			block := &streamBlock{index: len(output.Content), kind: parsed.Block.Type}
			blocks[*parsed.Index] = block
			switch block.kind {
			case "text":
				output.Content = append(output.Content, types.TextBlock{})
				emit(types.TextStart{ContentIndex: block.index, Partial: snapshot(*output)})
			case "thinking", "redacted_thinking":
				text, signature, redacted := "", "", block.kind == "redacted_thinking"
				if redacted {
					text, signature = "[Reasoning redacted]", parsed.Block.Data
				}
				output.Content = append(output.Content, types.ThinkingBlock{Thinking: text, Signature: &signature, Redacted: redacted})
				emit(types.ThinkingStart{ContentIndex: block.index, Partial: snapshot(*output)})
			case "tool_use":
				name := parsed.Block.Name
				if oauth {
					for _, tool := range tools {
						if strings.EqualFold(tool.Name, name) {
							name = tool.Name
							break
						}
					}
				}
				output.Content = append(output.Content, types.ToolCallBlock{ID: normalizeToolID(parsed.Block.ID), Name: name, Arguments: json.RawMessage(`{}`)})
				emit(types.ToolCallStart{ContentIndex: block.index, Partial: snapshot(*output)})
			default:
				block.index = -1 // Unknown blocks must never alias a known block.
			}
		case "content_block_delta", "content_block_stop":
			if parsed.Index == nil {
				return errors.New("Anthropic block event missing index")
			}
			block, exists := blocks[*parsed.Index]
			if !exists || block.closed {
				return errors.New("Anthropic block event without an open block")
			}
			if event == "content_block_stop" {
				block.closed = true
				if block.index < 0 {
					return nil
				}
				switch content := output.Content[block.index].(type) {
				case types.TextBlock:
					emit(types.TextEnd{ContentIndex: block.index, Content: content.Text, Partial: snapshot(*output)})
				case types.ThinkingBlock:
					emit(types.ThinkingEnd{ContentIndex: block.index, Content: content.Thinking, Partial: snapshot(*output)})
				case types.ToolCallBlock:
					arguments := block.json.String()
					if arguments != "" {
						if !json.Valid([]byte(arguments)) {
							return errors.New("invalid final Anthropic tool arguments")
						}
						content.Arguments = json.RawMessage(arguments)
					}
					output.Content[block.index] = content
					copy := snapshot(*output)
					emit(types.ToolCallEnd{ContentIndex: block.index, ToolCall: copy.Content[block.index].(types.ToolCallBlock), Partial: snapshot(*output)})
				}
				return nil
			}
			if parsed.Delta == nil || parsed.Delta.Type == "" {
				return errors.New("invalid Anthropic content_block_delta")
			}
			if block.index < 0 {
				return nil
			}
			delta := parsed.Delta
			switch delta.Type {
			case "text_delta":
				content, ok := output.Content[block.index].(types.TextBlock)
				if !ok {
					return errors.New("Anthropic text delta for non-text block")
				}
				content.Text += delta.Text
				output.Content[block.index] = content
				emit(types.TextDelta{ContentIndex: block.index, Delta: delta.Text, Partial: snapshot(*output)})
			case "thinking_delta":
				content, ok := output.Content[block.index].(types.ThinkingBlock)
				if !ok {
					return errors.New("Anthropic thinking delta for non-thinking block")
				}
				content.Thinking += delta.Thinking
				output.Content[block.index] = content
				emit(types.ThinkingDelta{ContentIndex: block.index, Delta: delta.Thinking, Partial: snapshot(*output)})
			case "signature_delta":
				content, ok := output.Content[block.index].(types.ThinkingBlock)
				if !ok {
					return errors.New("Anthropic signature delta for non-thinking block")
				}
				signature := *content.Signature + delta.Signature
				content.Signature = &signature
				output.Content[block.index] = content
			case "input_json_delta":
				content, ok := output.Content[block.index].(types.ToolCallBlock)
				if !ok {
					return errors.New("Anthropic input delta for non-tool block")
				}
				block.json.WriteString(delta.JSON)
				content.Arguments = partialJSON(block.json.String())
				output.Content[block.index] = content
				emit(types.ToolCallDelta{ContentIndex: block.index, Delta: delta.JSON, Partial: snapshot(*output)})
			}
		case "message_delta":
			if parsed.Delta == nil {
				return errors.New("invalid Anthropic message_delta")
			}
			if parsed.Delta.StopReason != nil {
				output.StopReason = mapStopReason(*parsed.Delta.StopReason)
			}
			updateUsage(model, &output.Usage, parsed.Usage)
		case "message_stop":
			for _, block := range blocks {
				if !block.closed {
					return errors.New("Anthropic message_stop with an open content block")
				}
			}
			stopped = true
		}
		return nil
	}
	if err := readSSE(ctx, input, handle); err != nil {
		return err
	}
	if !started {
		return errors.New("Anthropic stream ended before message_start")
	}
	if !stopped {
		return errors.New("Anthropic stream ended before message_stop")
	}
	if output.StopReason == types.Error {
		return errors.New("Anthropic provider returned an error stop reason")
	}
	return ctx.Err()
}

// Read complete byte lines: TCP/HTTP chunks may bisect UTF-8. There is no
// Scanner token limit. EOF dispatches every event type, including errors.
func readSSE(ctx context.Context, input io.Reader, handle func(string, string) error) error {
	reader := bufio.NewReader(input)
	event := ""
	var data []string
	flush := func() error {
		if event == "" && len(data) == 0 {
			return nil
		}
		err := handle(event, strings.Join(data, "\n"))
		event = ""
		data = nil
		return err
	}
	for {
		if err := ctx.Err(); err != nil {
			return err
		}
		line, err := reader.ReadString('\n')
		if err != nil && err != io.EOF {
			return fmt.Errorf("Anthropic stream read: %w", err)
		}
		line = strings.TrimSuffix(strings.TrimSuffix(line, "\n"), "\r")
		switch {
		case line == "":
			if flushErr := flush(); flushErr != nil {
				return flushErr
			}
		case strings.HasPrefix(line, "event:"):
			event = strings.TrimLeft(line[len("event:"):], " \t")
		case strings.HasPrefix(line, "data:"):
			data = append(data, strings.TrimLeft(line[len("data:"):], " \t"))
		}
		if err == io.EOF {
			return flush()
		}
	}
}

func partialJSON(input string) json.RawMessage {
	if json.Valid([]byte(input)) {
		return json.RawMessage(input)
	}
	if json.Valid([]byte(input + "}")) {
		return json.RawMessage(input + "}")
	}
	// Provisional only, matching Rust. block_stop requires valid final JSON.
	return json.RawMessage(`{}`)
}

func mapStopReason(reason string) types.StopReason {
	switch reason {
	case "end_turn", "pause_turn", "stop_sequence":
		return types.Stop
	case "max_tokens":
		return types.Length
	case "tool_use":
		return types.ToolUse
	default:
		return types.Error
	}
}

func updateUsage(model types.Model, usage *types.Usage, raw *rawUsage) {
	if raw == nil {
		return
	}
	if raw.Input != nil {
		usage.Input = *raw.Input
	}
	if raw.Output != nil {
		usage.Output = *raw.Output
	}
	if raw.CacheRead != nil {
		usage.CacheRead = *raw.CacheRead
	}
	if raw.CacheWrite != nil {
		usage.CacheWrite = *raw.CacheWrite
	}
	usage.TotalTokens = usage.Input + usage.Output + usage.CacheRead + usage.CacheWrite
	usage.Cost.Input = model.Cost.Input / 1_000_000 * float64(usage.Input)
	usage.Cost.Output = model.Cost.Output / 1_000_000 * float64(usage.Output)
	usage.Cost.CacheRead = model.Cost.CacheRead / 1_000_000 * float64(usage.CacheRead)
	usage.Cost.CacheWrite = model.Cost.CacheWrite / 1_000_000 * float64(usage.CacheWrite)
	usage.Cost.Total = usage.Cost.Input + usage.Cost.Output + usage.Cost.CacheRead + usage.Cost.CacheWrite
}
