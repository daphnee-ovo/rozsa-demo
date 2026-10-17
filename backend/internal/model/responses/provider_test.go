package responses

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"rozsa/backend/internal/model/types"
)

func TestBuildRequestPreservesResponsesItemsAndProviderIdentity(t *testing.T) {
	system := "instructions"
	signature := "encrypted"
	model := types.Model{ID: "custom-model", Provider: "vendor-x", API: types.APIOpenAIResponses}
	input := types.Context{
		SystemPrompt: &system,
		Messages: []types.Message{
			types.UserMessage{Content: types.UserBlocks{types.TextBlock{Text: "question"}, types.ImageBlock{Data: "abc", MimeType: "image/png"}}},
			types.AssistantMessage{Content: []types.ContentBlock{types.TextBlock{Text: "answer"}, types.ThinkingBlock{Thinking: "summary", Signature: &signature}, types.ToolCallBlock{ID: "call-1|item-1", Name: "lookup", Arguments: json.RawMessage(`{"q":"x"}`)}}},
			types.ToolResultMessage{ToolCallID: "call-1|item-1", Content: []types.ContentBlock{types.TextBlock{Text: "found"}}},
		},
		Tools: []types.ToolSchema{{Name: "lookup", Description: "search", Parameters: json.RawMessage(`{"type":"object"}`)}},
	}
	session := "session-1"
	request, err := buildRequest(model, input, types.SimpleStreamOptions{Base: types.StreamOptions{SessionID: &session}})
	if err != nil {
		t.Fatal(err)
	}
	encoded, err := json.Marshal(request)
	if err != nil {
		t.Fatal(err)
	}
	var got map[string]any
	if err := json.Unmarshal(encoded, &got); err != nil {
		t.Fatal(err)
	}
	items := got["input"].([]any)
	if got["model"] != "custom-model" || got["prompt_cache_key"] != "session-1" || got["store"] != false || got["stream"] != true || got["tool_choice"] != "auto" {
		t.Fatalf("request=%s", encoded)
	}
	if items[0].(map[string]any)["role"] != "developer" || items[1].(map[string]any)["role"] != "user" {
		t.Fatalf("input=%#v", items)
	}
	if items[2].(map[string]any)["type"] != "message" || items[3].(map[string]any)["type"] != "reasoning" || items[4].(map[string]any)["call_id"] != "call-1" || items[5].(map[string]any)["type"] != "function_call_output" {
		t.Fatalf("input items=%#v", items)
	}
	if got["tools"].([]any)[0].(map[string]any)["type"] != "function" {
		t.Fatalf("tools=%#v", got["tools"])
	}
}

func TestBuildRequestKeepsEmptyProtocolFieldsAndHonorsUnsupportedEffort(t *testing.T) {
	model := types.Model{ID: "m", Provider: "custom", ThinkingEffortMap: map[types.ThinkingEffort]*string{types.ThinkingHigh: nil}}
	effort := types.ThinkingHigh
	request, err := buildRequest(model, types.Context{
		Messages: []types.Message{
			types.UserMessage{Content: types.UserText("")},
			types.ToolResultMessage{ToolCallID: "call", Content: []types.ContentBlock{}},
		},
	}, types.SimpleStreamOptions{Reasoning: &effort})
	if err != nil {
		t.Fatal(err)
	}
	encoded, err := json.Marshal(request)
	if err != nil {
		t.Fatal(err)
	}
	var got map[string]any
	if err := json.Unmarshal(encoded, &got); err != nil {
		t.Fatal(err)
	}
	items := got["input"].([]any)
	if items[0].(map[string]any)["content"].([]any)[0].(map[string]any)["text"] != "" {
		t.Fatalf("empty text omitted: %s", encoded)
	}
	if items[1].(map[string]any)["output"] != "" {
		t.Fatalf("empty tool output omitted: %s", encoded)
	}
	if _, exists := got["reasoning"]; exists {
		t.Fatalf("unsupported effort emitted reasoning: %s", encoded)
	}
}

func TestBuildRequestRejectsMalformedToolResultContent(t *testing.T) {
	_, err := buildRequest(types.Model{ID: "m", Provider: "custom"}, types.Context{
		Messages: []types.Message{
			types.AssistantMessage{Content: []types.ContentBlock{types.ToolCallBlock{ID: "call", Name: "lookup", Arguments: json.RawMessage("{")}}},
		},
	}, types.SimpleStreamOptions{})
	if err == nil || !strings.Contains(err.Error(), "invalid arguments for Responses function call") {
		t.Fatalf("err=%v, want transparent invalid arguments error", err)
	}

	_, err = buildRequest(types.Model{ID: "m", Provider: "custom"}, types.Context{
		Messages: []types.Message{
			types.ToolResultMessage{ToolCallID: "call", Content: []types.ContentBlock{types.ToolCallBlock{Arguments: json.RawMessage("{")}}},
		},
	}, types.SimpleStreamOptions{})
	if err == nil || !strings.Contains(err.Error(), "encode Responses tool result") {
		t.Fatalf("err=%v, want transparent malformed content error", err)
	}
}

func TestStreamResponsesSSEFragmentsThinkingToolAndUsage(t *testing.T) {
	var payload map[string]any
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		if request.URL.Path != "/v1/responses" || request.Method != http.MethodPost {
			t.Errorf("request=%s %s", request.Method, request.URL.Path)
		}
		if request.Header.Get("Authorization") != "Bearer secret" || request.Header.Get("X-Vendor") != "kept" || request.Header.Get("session_id") != "session-1" || request.Header.Get("x-client-request-id") != "session-1" || request.Header.Get("ChatGPT-Account-ID") != "acct-1" {
			t.Errorf("headers=%v", request.Header)
		}
		if err := json.NewDecoder(request.Body).Decode(&payload); err != nil {
			t.Error(err)
		}
		writer.Header().Set("Content-Type", "text/event-stream")
		flusher := writer.(http.Flusher)
		frames := []string{
			"event: response.created\ndata: {\"type\":\"response.created\"}\n\n",
			"event: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"hé\"}\n\n",
			"event: response.reasoning_summary_text.delta\ndata: {\"type\":\"response.reasoning_summary_text.delta\",\"delta\":\"think\",\"summary_index\":0}\n\n",
			"event: response.function_call_arguments.delta\ndata: {\"type\":\"response.function_call_arguments.delta\",\"item_id\":\"item-1\",\"call_id\":\"call-1\",\"delta\":\"{\\\"x\\\":\"}\n\n",
			"event: response.function_call_arguments.delta\ndata: {\"type\":\"response.function_call_arguments.delta\",\"item_id\":\"item-1\",\"call_id\":\"call-1\",\"delta\":\"y\\\"}\"}\n\n",
			"event: response.output_item.done\ndata: {\"type\":\"response.output_item.done\",\"item\":{\"type\":\"message\",\"content\":[{\"type\":\"output_text\",\"text\":\"héllo\"}]}}\n\n",
			"event: response.output_item.done\ndata: {\"type\":\"response.output_item.done\",\"item\":{\"type\":\"reasoning\",\"summary\":[{\"type\":\"summary_text\",\"text\":\"reasoning final\"}],\"encrypted_content\":\"enc\"}}\n\n",
			"event: response.output_item.done\ndata: {\"type\":\"response.output_item.done\",\"item\":{\"type\":\"function_call\",\"id\":\"item-1\",\"call_id\":\"call-1\",\"name\":\"lookup\",\"arguments\":\"{\\\"x\\\":\\\"y\\\"}\"}}\n\n",
			"event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp-1\",\"usage\":{\"input_tokens\":10,\"output_tokens\":4,\"total_tokens\":14,\"input_tokens_details\":{\"cached_tokens\":3},\"output_tokens_details\":{\"reasoning_tokens\":2}}}}\n\n",
			"data: [DONE]\n\n",
		}
		for _, frame := range frames {
			for start := 0; start < len(frame); {
				end := start + 7
				if end > len(frame) {
					end = len(frame)
				}
				_, _ = fmt.Fprint(writer, frame[start:end])
				flusher.Flush()
				start = end
			}
		}
	}))
	defer server.Close()
	model := types.Model{ID: "configured-name", Provider: "vendor-x", API: types.APIOpenAIResponses, BaseURL: server.URL + "/v1", Cost: types.Cost{Input: 2, Output: 4, CacheRead: 1}, Headers: map[string]string{"X-Vendor": "kept"}}
	var events []types.StreamEvent
	sessionID := "session-1"
	err := NewProvider(server.Client()).StreamSimple(context.Background(), model, types.Context{}, types.SimpleStreamOptions{Base: types.StreamOptions{APIKey: "secret", SessionID: &sessionID, Headers: map[string]string{"x-rozsa-account-id": "acct-1"}}}, func(event types.StreamEvent) { events = append(events, event) })
	if err != nil {
		t.Fatal(err)
	}
	if payload["model"] != "configured-name" || payload["store"] != false {
		t.Fatalf("payload=%#v", payload)
	}
	done, ok := events[len(events)-1].(types.Done)
	if !ok || done.Reason != types.ToolUse || done.Message.API != types.APIOpenAIResponses || done.Message.Provider != "vendor-x" {
		t.Fatalf("terminal=%#v", events[len(events)-1])
	}
	if done.Message.ResponseID == nil || *done.Message.ResponseID != "resp-1" || done.Message.Usage.Input != 10 || done.Message.Usage.CacheRead != 3 || done.Message.Usage.Output != 4 || done.Message.Usage.Cost.Total == 0 {
		t.Fatalf("message=%#v", done.Message)
	}
	if len(done.Message.Content) != 3 {
		t.Fatalf("content=%#v", done.Message.Content)
	}
	if text := done.Message.Content[0].(types.TextBlock).Text; text != "héllo" {
		t.Fatalf("text=%q", text)
	}
	thinking := done.Message.Content[1].(types.ThinkingBlock)
	if thinking.Thinking != "reasoning final" || thinking.Signature == nil || *thinking.Signature != "enc" {
		t.Fatalf("thinking=%#v", thinking)
	}
	tool := done.Message.Content[2].(types.ToolCallBlock)
	if tool.ID != "call-1" || tool.Name != "lookup" || string(tool.Arguments) != `{"x":"y"}` {
		t.Fatalf("tool=%#v", tool)
	}
	if countEvents[types.TextDelta](events) != 1 || countEvents[types.ThinkingDelta](events) != 1 || countEvents[types.ToolCallDelta](events) != 2 {
		t.Fatalf("event sequence=%#v", events)
	}
	for _, event := range events {
		if delta, ok := event.(types.ToolCallDelta); ok {
			partial := delta.Partial.Content[delta.ContentIndex].(types.ToolCallBlock)
			if string(partial.Arguments) != "null" {
				t.Fatalf("partial arguments=%s", partial.Arguments)
			}
			break
		}
	}
}

func TestStreamIncompleteAndFailedHaveDistinctTerminalResults(t *testing.T) {
	for _, test := range []struct {
		name, frame string
		wantStop    types.StopReason
		wantErr     bool
	}{
		{"incomplete", `{"type":"response.incomplete","response":{"incomplete_details":{"reason":"max_output_tokens"}}}`, types.Length, false},
		{"failed", `{"type":"response.failed","response":{"error":{"code":"server_error","message":"unavailable"}}}`, types.Error, true},
	} {
		t.Run(test.name, func(t *testing.T) {
			output := types.AssistantMessage{Content: []types.ContentBlock{}}
			err := consume(context.Background(), strings.NewReader("data: "+test.frame+"\n\n"), types.Model{}, &output, func(types.StreamEvent) {})
			if (err != nil) != test.wantErr {
				t.Fatalf("err=%v", err)
			}
			if test.wantErr {
				if !strings.Contains(err.Error(), "unavailable") {
					t.Fatalf("err=%v", err)
				}
			} else if err != nil {
				t.Fatalf("err=%v", err)
			}
			if test.name == "incomplete" {
				if output.StopReason != test.wantStop || output.ErrorMessage == nil || *output.ErrorMessage != "max_output_tokens" {
					t.Fatalf("output=%#v", output)
				}
			}
		})
	}
}

func TestConsumeRejectsMalformedUnsupportedLateAndMissingTerminal(t *testing.T) {
	for _, test := range []struct{ name, input, want string }{
		{"malformed", "data: {bad}\n\n", "invalid Responses SSE payload"},
		{"missing type", "data: {}\n\n", "missing type"},
		{"missing delta", "data: {\"type\":\"response.output_text.delta\"}\n\n", "missing delta"},
		{"unsupported event", "data: {\"type\":\"response.unknown\"}\n\n", "unsupported Responses event"},
		{"unsupported item", "data: {\"type\":\"response.output_item.done\",\"item\":{\"type\":\"web_search_call\"}}\n\n", "unsupported Responses output item"},
		{"unsupported content", "data: {\"type\":\"response.output_item.done\",\"item\":{\"type\":\"message\",\"content\":[{\"type\":\"refusal\"}]}}\n\n", "unsupported Responses message content"},
		{"malformed function call", "data: {\"type\":\"response.output_item.done\",\"item\":{\"type\":\"function_call\",\"arguments\":\"{}\"}}\n\n", "missing call ID or name"},
		{"malformed function arguments", "data: {\"type\":\"response.output_item.done\",\"item\":{\"type\":\"function_call\",\"call_id\":\"call-1\",\"name\":\"lookup\",\"arguments\":\"{bad\"}}\n\n", "invalid arguments JSON"},
		{"missing terminal", "data: {\"type\":\"response.created\"}\n\n", "without a terminal"},
		{"late event", "data: {\"type\":\"response.completed\"}\n\ndata: {\"type\":\"response.created\"}\n\n", "after terminal"},
	} {
		t.Run(test.name, func(t *testing.T) {
			err := consume(context.Background(), strings.NewReader(test.input), types.Model{}, &types.AssistantMessage{Content: []types.ContentBlock{}}, func(types.StreamEvent) {})
			if err == nil || !strings.Contains(err.Error(), test.want) {
				t.Fatalf("err=%v want=%q", err, test.want)
			}
		})
	}
	invalidUTF8 := string([]byte{'d', 'a', 't', 'a', ':', ' ', 0xff, '\n', '\n'})
	if err := consume(context.Background(), strings.NewReader(invalidUTF8), types.Model{}, &types.AssistantMessage{Content: []types.ContentBlock{}}, func(types.StreamEvent) {}); err == nil || !strings.Contains(err.Error(), "UTF-8") {
		t.Fatalf("invalid UTF-8 error=%v", err)
	}
	output := types.AssistantMessage{Content: []types.ContentBlock{}}
	if err := consume(context.Background(), strings.NewReader("data: {\"type\":\"response.incomplete\"}\n\n"), types.Model{}, &output, func(types.StreamEvent) {}); err != nil || output.StopReason != types.Length {
		t.Fatalf("incomplete without response: output=%#v err=%v", output, err)
	}
}

func TestStreamCancellationReportsAborted(t *testing.T) {
	started := make(chan struct{})
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		writer.Header().Set("Content-Type", "text/event-stream")
		writer.WriteHeader(http.StatusOK)
		writer.(http.Flusher).Flush()
		close(started)
		<-request.Context().Done()
	}))
	defer server.Close()
	ctx, cancel := context.WithCancel(context.Background())
	var terminal types.StreamEvent
	done := make(chan error, 1)
	go func() {
		done <- NewProvider(server.Client()).StreamSimple(ctx, types.Model{ID: "m", Provider: "custom", BaseURL: server.URL}, types.Context{}, types.SimpleStreamOptions{}, func(event types.StreamEvent) {
			switch event.(type) {
			case types.Done, types.StreamError:
				terminal = event
			}
		})
	}()
	<-started
	cancel()
	err := <-done
	if !errors.Is(err, context.Canceled) {
		t.Fatalf("err=%v", err)
	}
	failure, ok := terminal.(types.StreamError)
	if !ok || failure.Reason != types.Aborted {
		t.Fatalf("terminal=%#v", terminal)
	}
}

func countEvents[T any](events []types.StreamEvent) int {
	count := 0
	for _, event := range events {
		if _, ok := event.(T); ok {
			count++
		}
	}
	return count
}
