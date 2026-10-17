package completions

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"strings"

	"rozsa/backend/internal/model/types"
)

type usageChunk struct {
	Prompt      *uint64 `json:"prompt_tokens"`
	Completion  *uint64 `json:"completion_tokens"`
	PromptCache *uint64 `json:"prompt_cache_hit_tokens"`
	Details     *struct {
		Cached *uint64 `json:"cached_tokens"`
		Write  *uint64 `json:"cache_write_tokens"`
	} `json:"prompt_tokens_details"`
}
type chatChunk struct {
	ID      string `json:"id"`
	Model   string `json:"model"`
	Choices []struct {
		Delta struct {
			Content          string `json:"content"`
			ReasoningContent string `json:"reasoning_content"`
			Reasoning        string `json:"reasoning"`
			ReasoningText    string `json:"reasoning_text"`
			Tools            []struct {
				Index    *int   `json:"index"`
				ID       string `json:"id"`
				Function struct {
					Name      string `json:"name"`
					Arguments string `json:"arguments"`
				} `json:"function"`
			} `json:"tool_calls"`
		} `json:"delta"`
		Finish *string     `json:"finish_reason"`
		Usage  *usageChunk `json:"usage"`
	} `json:"choices"`
	Usage *usageChunk `json:"usage"`
}

func consume(ctx context.Context, body io.Reader, model types.Model, output *types.AssistantMessage, emit func(types.StreamEvent)) error {
	reader := bufio.NewReader(body)
	buffer := make([]byte, 0, 4096)
	event := make([]string, 0, 2)
	toolIndexes := map[int]int{}
	toolArgs := map[int]string{}
	textIndex, thinkingIndex := -1, -1
	sawFinish := false
	parseEvent := func() error {
		if len(event) == 0 {
			return nil
		}
		data := strings.Join(event, "\n")
		event = event[:0]
		if data == "[DONE]" {
			return nil
		}
		var chunk chatChunk
		if err := json.Unmarshal([]byte(data), &chunk); err != nil {
			return fmt.Errorf("invalid OpenAI chat chunk: %w", err)
		}
		if chunk.ID != "" && output.ResponseID == nil {
			v := chunk.ID
			output.ResponseID = &v
		}
		if chunk.Model != "" && chunk.Model != model.ID && output.ResponseModel == nil {
			v := chunk.Model
			output.ResponseModel = &v
		}
		usage := chunk.Usage
		if len(chunk.Choices) > 0 && usage == nil {
			usage = chunk.Choices[0].Usage
		}
		if usage != nil {
			applyUsage(model, output, usage)
		}
		if len(chunk.Choices) == 0 {
			return nil
		}
		choice := chunk.Choices[0]
		if choice.Finish != nil {
			sawFinish = true
			switch *choice.Finish {
			case "stop", "end":
				output.StopReason = types.Stop
			case "length":
				output.StopReason = types.Length
			case "tool_calls", "function_call":
				output.StopReason = types.ToolUse
			default:
				output.StopReason = types.Error
				message := "Provider finish_reason: " + *choice.Finish
				output.ErrorMessage = &message
			}
		}
		d := choice.Delta
		if d.Content != "" {
			if textIndex < 0 {
				textIndex = len(output.Content)
				output.Content = append(output.Content, types.TextBlock{})
				emit(types.TextStart{ContentIndex: textIndex, Partial: clone(*output)})
			}
			b := output.Content[textIndex].(types.TextBlock)
			b.Text += d.Content
			output.Content[textIndex] = b
			emit(types.TextDelta{ContentIndex: textIndex, Delta: d.Content, Partial: clone(*output)})
		}
		reasoning := d.ReasoningContent
		if reasoning == "" {
			reasoning = d.Reasoning
		}
		if reasoning == "" {
			reasoning = d.ReasoningText
		}
		if reasoning != "" {
			if thinkingIndex < 0 {
				thinkingIndex = len(output.Content)
				sig := "reasoning_content"
				output.Content = append(output.Content, types.ThinkingBlock{Signature: &sig})
				emit(types.ThinkingStart{ContentIndex: thinkingIndex, Partial: clone(*output)})
			}
			b := output.Content[thinkingIndex].(types.ThinkingBlock)
			b.Thinking += reasoning
			output.Content[thinkingIndex] = b
			emit(types.ThinkingDelta{ContentIndex: thinkingIndex, Delta: reasoning, Partial: clone(*output)})
		}
		for _, td := range d.Tools {
			idx := 0
			if td.Index != nil {
				idx = *td.Index
			}
			ci, ok := toolIndexes[idx]
			if !ok {
				ci = len(output.Content)
				toolIndexes[idx] = ci
				output.Content = append(output.Content, types.ToolCallBlock{Arguments: json.RawMessage(`{}`)})
				emit(types.ToolCallStart{ContentIndex: ci, Partial: clone(*output)})
			}
			b := output.Content[ci].(types.ToolCallBlock)
			if td.ID != "" {
				b.ID = td.ID
			}
			if td.Function.Name != "" {
				b.Name = td.Function.Name
			}
			if td.Function.Arguments != "" {
				toolArgs[idx] += td.Function.Arguments
				raw := json.RawMessage(toolArgs[idx])
				if !json.Valid(raw) {
					raw, _ = json.Marshal(toolArgs[idx])
				}
				b.Arguments = raw
				output.Content[ci] = b
				emit(types.ToolCallDelta{ContentIndex: ci, Delta: td.Function.Arguments, Partial: clone(*output)})
			}
			output.Content[ci] = b
		}
		return nil
	}
	flushLine := func(line []byte) error {
		line = bytes.TrimSuffix(line, []byte("\r"))
		if len(line) == 0 {
			return parseEvent()
		}
		if bytes.HasPrefix(line, []byte("data:")) {
			event = append(event, strings.TrimLeft(string(line[5:]), " "))
		}
		return nil
	}
	for {
		line, err := reader.ReadBytes('\n')
		buffer = append(buffer, line...)
		if err == nil {
			if e := flushLine(buffer[:len(buffer)-1]); e != nil {
				return e
			}
			buffer = buffer[:0]
			continue
		}
		if err != io.EOF {
			return err
		}
		if len(buffer) > 0 {
			if e := flushLine(buffer); e != nil {
				return e
			}
		}
		if e := parseEvent(); e != nil {
			return e
		}
		break
	}
	if output.StopReason == types.Error {
		if output.ErrorMessage != nil {
			return errors.New(*output.ErrorMessage)
		}
		return errors.New("provider returned an error finish reason")
	}
	if textIndex >= 0 {
		b := output.Content[textIndex].(types.TextBlock)
		emit(types.TextEnd{ContentIndex: textIndex, Content: b.Text, Partial: clone(*output)})
	}
	if thinkingIndex >= 0 {
		b := output.Content[thinkingIndex].(types.ThinkingBlock)
		emit(types.ThinkingEnd{ContentIndex: thinkingIndex, Content: b.Thinking, Partial: clone(*output)})
	}
	for _, idx := range toolIndexes {
		b := output.Content[idx].(types.ToolCallBlock)
		emit(types.ToolCallEnd{ContentIndex: idx, ToolCall: b, Partial: clone(*output)})
	}
	if !sawFinish {
		return errors.New("stream ended without finish_reason")
	}
	return nil
}

func applyUsage(model types.Model, output *types.AssistantMessage, raw *usageChunk) {
	prompt, completion, read, write := uint64(0), uint64(0), uint64(0), uint64(0)
	if raw.Prompt != nil {
		prompt = *raw.Prompt
	}
	if raw.Completion != nil {
		completion = *raw.Completion
	}
	if raw.Details != nil {
		if raw.Details.Cached != nil {
			read = *raw.Details.Cached
		}
		if raw.Details.Write != nil {
			write = *raw.Details.Write
		}
	}
	if read == 0 && raw.PromptCache != nil {
		read = *raw.PromptCache
	}
	input := prompt
	if read+write < input {
		input -= read + write
	} else {
		input = 0
	}
	u := types.Usage{Input: input, Output: completion, CacheRead: read, CacheWrite: write, TotalTokens: input + completion + read + write}
	u.Cost.Input = model.Cost.Input / 1e6 * float64(u.Input)
	u.Cost.Output = model.Cost.Output / 1e6 * float64(u.Output)
	u.Cost.CacheRead = model.Cost.CacheRead / 1e6 * float64(u.CacheRead)
	u.Cost.CacheWrite = model.Cost.CacheWrite / 1e6 * float64(u.CacheWrite)
	u.Cost.Total = u.Cost.Input + u.Cost.Output + u.Cost.CacheRead + u.Cost.CacheWrite
	output.Usage = u
}
