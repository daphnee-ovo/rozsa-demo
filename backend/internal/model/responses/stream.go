package responses

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

type usage struct {
	Input   uint64 `json:"input_tokens"`
	Output  uint64 `json:"output_tokens"`
	Total   uint64 `json:"total_tokens"`
	Details *struct {
		Cached *uint64 `json:"cached_tokens"`
	} `json:"input_tokens_details"`
}

type responseItem struct {
	Type      string        `json:"type"`
	ID        string        `json:"id"`
	Role      string        `json:"role"`
	Content   []contentPart `json:"content"`
	CallID    string        `json:"call_id"`
	Name      string        `json:"name"`
	Arguments string        `json:"arguments"`
	Output    string        `json:"output"`
	Summary   []struct {
		Text string `json:"text"`
	} `json:"summary"`
	EncryptedContent *string `json:"encrypted_content"`
}

type contentPart struct {
	Type string `json:"type"`
	Text string `json:"text"`
}

type rawEvent struct {
	Type         string          `json:"type"`
	Response     json.RawMessage `json:"response"`
	Item         json.RawMessage `json:"item"`
	ItemID       string          `json:"item_id"`
	CallID       string          `json:"call_id"`
	Delta        *string         `json:"delta"`
	SummaryIndex *int            `json:"summary_index"`
	ContentIndex *int            `json:"content_index"`
}

type streamState struct {
	output        *types.AssistantMessage
	model         types.Model
	emit          func(types.StreamEvent)
	textIndex     int
	thinkingIndex int
	toolIndexes   map[string]int
	toolArgs      map[string]string
	terminal      bool
}

func consume(ctx context.Context, body io.Reader, model types.Model, output *types.AssistantMessage, emit func(types.StreamEvent)) error {
	state := &streamState{output: output, model: model, emit: emit, textIndex: -1, thinkingIndex: -1, toolIndexes: map[string]int{}, toolArgs: map[string]string{}}
	reader := bufio.NewReader(body)
	eventName := ""
	data := make([]string, 0, 2)
	flush := func() error {
		if len(data) == 0 {
			eventName = ""
			return nil
		}
		payload := strings.Join(data, "\n")
		data = data[:0]
		name := eventName
		eventName = ""
		if payload == "[DONE]" {
			return nil
		}
		if !utf8.ValidString(payload) {
			return errors.New("invalid UTF-8 in Responses SSE payload")
		}
		var raw rawEvent
		if err := json.Unmarshal([]byte(payload), &raw); err != nil {
			return fmt.Errorf("invalid Responses SSE payload: %w", err)
		}
		if raw.Type == "" {
			return errors.New("invalid Responses SSE payload: missing type")
		}
		if name != "" && name != raw.Type {
			return fmt.Errorf("Responses SSE event name %q does not match payload type %q", name, raw.Type)
		}
		if err := ctx.Err(); err != nil {
			return err
		}
		return state.handle(raw)
	}
	for {
		line, err := reader.ReadString('\n')
		if len(line) != 0 {
			line = strings.TrimSuffix(strings.TrimSuffix(line, "\n"), "\r")
			if !utf8.ValidString(line) {
				return errors.New("invalid UTF-8 in Responses SSE stream")
			}
			if line == "" {
				if flushErr := flush(); flushErr != nil {
					return flushErr
				}
			} else if strings.HasPrefix(line, ":") {
				// SSE comments are keep-alives.
			} else if strings.HasPrefix(line, "event:") {
				eventName = strings.TrimSpace(strings.TrimPrefix(line, "event:"))
			} else if strings.HasPrefix(line, "data:") {
				data = append(data, strings.TrimLeft(strings.TrimPrefix(line, "data:"), " "))
			}
		}
		if err == nil {
			continue
		}
		if !errors.Is(err, io.EOF) {
			return err
		}
		if flushErr := flush(); flushErr != nil {
			return flushErr
		}
		break
	}
	if err := ctx.Err(); err != nil {
		return err
	}
	if !state.terminal {
		return errors.New("Responses stream ended without a terminal response event")
	}
	return nil
}

func (state *streamState) handle(raw rawEvent) error {
	if state.terminal {
		return fmt.Errorf("Responses event %q received after terminal event", raw.Type)
	}
	switch raw.Type {
	case "response.created":
		return nil
	case "response.output_item.added", "response.output_item.done":
		if len(raw.Item) == 0 || string(raw.Item) == "null" {
			return fmt.Errorf("Responses %s event is missing item", raw.Type)
		}
		var item responseItem
		if err := json.Unmarshal(raw.Item, &item); err != nil {
			return fmt.Errorf("invalid Responses output item: %w", err)
		}
		if err := validateOutputItem(item); err != nil {
			return err
		}
		if raw.Type == "response.output_item.done" {
			if item.Type == "function_call" && (callID(item.CallID) == "" && callID(item.ID) == "" || strings.TrimSpace(item.Name) == "") {
				return errors.New("malformed Responses function_call output item: missing call ID or name")
			}
			if err := state.finishItem(item); err != nil {
				return err
			}
		}
		return nil
	case "response.output_text.delta":
		delta, err := requiredDelta(raw)
		if err != nil {
			return err
		}
		state.textDelta(delta)
	case "response.reasoning_summary_text.delta", "response.reasoning_text.delta":
		delta, err := requiredDelta(raw)
		if err != nil {
			return err
		}
		state.thinkingDelta(delta)
	case "response.function_call_arguments.delta":
		delta, err := requiredDelta(raw)
		if err != nil {
			return err
		}
		key := raw.CallID
		if key == "" {
			key = raw.ItemID
		}
		if strings.TrimSpace(key) == "" {
			return errors.New("Responses function-call delta is missing call_id and item_id")
		}
		state.toolDelta(key, delta)
	case "response.completed":
		return state.complete(raw.Response)
	case "response.incomplete":
		var response struct {
			Details *struct {
				Reason string `json:"reason"`
			} `json:"incomplete_details"`
		}
		if len(raw.Response) != 0 && string(raw.Response) != "null" {
			if err := json.Unmarshal(raw.Response, &response); err != nil {
				return fmt.Errorf("invalid Responses incomplete payload: %w", err)
			}
		}
		state.output.StopReason = types.Length
		if response.Details != nil && response.Details.Reason != "" {
			reason := response.Details.Reason
			state.output.ErrorMessage = &reason
		}
		state.terminal = true
	case "response.failed":
		message, err := failedMessage(raw.Response)
		if err != nil {
			return err
		}
		state.terminal = true
		return fmt.Errorf("Responses stream failed: %s", message)
	default:
		return fmt.Errorf("unsupported Responses event type %q", raw.Type)
	}
	return nil
}

func requiredDelta(raw rawEvent) (string, error) {
	if raw.Delta == nil {
		return "", fmt.Errorf("Responses event %q is missing delta", raw.Type)
	}
	return *raw.Delta, nil
}

func validateOutputItem(item responseItem) error {
	switch item.Type {
	case "message":
		for _, part := range item.Content {
			if part.Type != "output_text" && part.Type != "input_text" && part.Type != "input_image" {
				return fmt.Errorf("unsupported Responses message content type %q", part.Type)
			}
		}
	case "function_call", "function_call_output", "reasoning":
	default:
		return fmt.Errorf("unsupported Responses output item type %q", item.Type)
	}
	return nil
}

func failedMessage(payload json.RawMessage) (string, error) {
	if len(payload) == 0 || string(payload) == "null" {
		return "response.failed event with no response payload", nil
	}
	var response struct {
		Error *struct {
			Message string `json:"message"`
		} `json:"error"`
	}
	if err := json.Unmarshal(payload, &response); err != nil {
		return "", fmt.Errorf("invalid Responses failed payload: %w", err)
	}
	if response.Error == nil || response.Error.Message == "" {
		return "unknown error", nil
	}
	return response.Error.Message, nil
}

func (state *streamState) textDelta(delta string) {
	if state.textIndex < 0 {
		state.textIndex = len(state.output.Content)
		state.output.Content = append(state.output.Content, types.TextBlock{})
		state.emit(types.TextStart{ContentIndex: state.textIndex, Partial: snapshot(*state.output)})
	}
	block := state.output.Content[state.textIndex].(types.TextBlock)
	block.Text += delta
	state.output.Content[state.textIndex] = block
	state.emit(types.TextDelta{ContentIndex: state.textIndex, Delta: delta, Partial: snapshot(*state.output)})
}

func (state *streamState) thinkingDelta(delta string) {
	if state.thinkingIndex < 0 {
		state.thinkingIndex = len(state.output.Content)
		state.output.Content = append(state.output.Content, types.ThinkingBlock{})
		state.emit(types.ThinkingStart{ContentIndex: state.thinkingIndex, Partial: snapshot(*state.output)})
	}
	block := state.output.Content[state.thinkingIndex].(types.ThinkingBlock)
	block.Thinking += delta
	state.output.Content[state.thinkingIndex] = block
	state.emit(types.ThinkingDelta{ContentIndex: state.thinkingIndex, Delta: delta, Partial: snapshot(*state.output)})
}

func (state *streamState) toolDelta(key, delta string) {
	index, exists := state.toolIndexes[key]
	if !exists {
		index = len(state.output.Content)
		state.toolIndexes[key] = index
		state.toolArgs[key] = ""
		state.output.Content = append(state.output.Content, types.ToolCallBlock{ID: key, Arguments: json.RawMessage("null")})
		state.emit(types.ToolCallStart{ContentIndex: index, Partial: snapshot(*state.output)})
	}
	state.toolArgs[key] += delta
	state.emit(types.ToolCallDelta{ContentIndex: index, Delta: delta, Partial: snapshot(*state.output)})
}

func (state *streamState) finishItem(item responseItem) error {
	switch item.Type {
	case "message":
		for _, part := range item.Content {
			if part.Type == "output_text" && state.textIndex >= 0 {
				block := state.output.Content[state.textIndex].(types.TextBlock)
				block.Text = part.Text
				state.output.Content[state.textIndex] = block
				state.emit(types.TextEnd{ContentIndex: state.textIndex, Content: part.Text, Partial: snapshot(*state.output)})
				state.textIndex = -1
			}
		}
	case "reasoning":
		if state.thinkingIndex >= 0 {
			finalText := ""
			for _, part := range item.Summary {
				finalText += part.Text
			}
			block := state.output.Content[state.thinkingIndex].(types.ThinkingBlock)
			if finalText != "" {
				block.Thinking = finalText
			}
			block.Signature = item.EncryptedContent
			state.output.Content[state.thinkingIndex] = block
			state.emit(types.ThinkingEnd{ContentIndex: state.thinkingIndex, Content: block.Thinking, Partial: snapshot(*state.output)})
			state.thinkingIndex = -1
		}
	case "function_call":
		key := callID(item.CallID)
		if key == "" {
			key = callID(item.ID)
		}
		index, exists := state.toolIndexes[key]
		if !exists && item.ID != "" {
			index, exists = state.toolIndexes[item.ID]
			if exists {
				delete(state.toolIndexes, item.ID)
				state.toolIndexes[key] = index
				state.toolArgs[key] = state.toolArgs[item.ID]
				delete(state.toolArgs, item.ID)
			}
		}
		if !exists {
			index = len(state.output.Content)
			state.output.Content = append(state.output.Content, types.ToolCallBlock{})
		}
		arguments := json.RawMessage(item.Arguments)
		if !json.Valid(arguments) {
			return fmt.Errorf("malformed Responses function_call output item: invalid arguments JSON")
		}
		block := types.ToolCallBlock{ID: key, Name: item.Name, Arguments: arguments}
		state.output.Content[index] = block
		state.emit(types.ToolCallEnd{ContentIndex: index, ToolCall: block, Partial: snapshot(*state.output)})
	}
	return nil
}

func (state *streamState) complete(payload json.RawMessage) error {
	var response struct {
		ID    string          `json:"id"`
		Usage json.RawMessage `json:"usage"`
	}
	if len(payload) != 0 && string(payload) != "null" {
		if err := json.Unmarshal(payload, &response); err != nil {
			return fmt.Errorf("invalid Responses completed payload: %w", err)
		}
		if len(response.Usage) != 0 && string(response.Usage) != "null" {
			var raw usage
			if err := json.Unmarshal(response.Usage, &raw); err != nil {
				return fmt.Errorf("invalid Responses usage: %w", err)
			}
			applyUsage(state.model, state.output, raw)
		}
	}
	if response.ID != "" {
		state.output.ResponseID = &response.ID
	}
	state.output.StopReason = types.Stop
	for _, block := range state.output.Content {
		if _, ok := block.(types.ToolCallBlock); ok {
			state.output.StopReason = types.ToolUse
			break
		}
	}
	state.terminal = true
	return nil
}

func applyUsage(model types.Model, output *types.AssistantMessage, raw usage) {
	read := uint64(0)
	if raw.Details != nil && raw.Details.Cached != nil {
		read = *raw.Details.Cached
	}
	output.Usage = types.Usage{Input: raw.Input, Output: raw.Output, CacheRead: read, TotalTokens: raw.Total}
	output.Usage.Cost.Input = model.Cost.Input / 1e6 * float64(raw.Input)
	output.Usage.Cost.Output = model.Cost.Output / 1e6 * float64(raw.Output)
	output.Usage.Cost.CacheRead = model.Cost.CacheRead / 1e6 * float64(read)
	output.Usage.Cost.Total = output.Usage.Cost.Input + output.Usage.Cost.Output + output.Usage.Cost.CacheRead
}
