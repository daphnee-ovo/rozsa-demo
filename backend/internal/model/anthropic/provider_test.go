package anthropic

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"os"
	"reflect"
	"strings"
	"sync/atomic"
	"testing"
	"time"

	"rozsa/backend/internal/model/types"
)

func pointer[T any](value T) *T { return &value }
func fixtureModel() types.Model {
	return types.Model{ID: "claude-fixture", Provider: "anthropic", API: types.APIAnthropicMessages, MaxTokens: 1024, Reasoning: true, Input: []string{"text", "image"}, Cost: types.Cost{Input: 3, Output: 15, CacheRead: 0.3, CacheWrite: 3.75}}
}
func fixtureContext() types.Context {
	image := types.ImageBlock{Data: "aW1n", MimeType: "image/png"}
	return types.Context{SystemPrompt: pointer("system"), Messages: []types.Message{
		types.UserMessage{Content: types.UserText(" \n")},
		types.UserMessage{Content: types.UserBlocks{image}},
		types.AssistantMessage{Content: []types.ContentBlock{
			types.TextBlock{Text: "answer"}, types.ThinkingBlock{Thinking: "signed", Signature: pointer("signature")}, types.ThinkingBlock{Thinking: "unsigned"}, types.ThinkingBlock{Redacted: true, Signature: pointer("opaque")}, types.ThinkingBlock{Redacted: true}, image,
			types.ToolCallBlock{ID: "call:unchanged", Name: "read", Arguments: json.RawMessage(`{"path":"a"}`)},
		}},
		types.ToolResultMessage{ToolCallID: "call:unchanged", Content: []types.ContentBlock{types.TextBlock{Text: "a"}, types.TextBlock{Text: "b"}}},
		types.ToolResultMessage{ToolCallID: "other", Content: []types.ContentBlock{image}, IsError: true},
	}, Tools: []types.ToolSchema{{Name: "read", Description: "read a file", Parameters: json.RawMessage(`{"type":"object","properties":{"path":{"type":"string"}},"required":["path"],"additionalProperties":false}`)}}}
}
func fixtureOptions() types.SimpleStreamOptions {
	return types.SimpleStreamOptions{Base: types.StreamOptions{APIKey: "sk-ant-oat-fake", Temperature: pointer(0.5), MaxTokens: pointer(2048), CacheRetention: types.CacheLong, Metadata: json.RawMessage(`{"user_id":"fixture-user","ignored":true}`)}, Reasoning: pointer(types.ThinkingMax), ThinkingEffortBudgets: map[types.ThinkingEffort]uint64{types.ThinkingHigh: 4096}, ToolChoice: json.RawMessage(`{"type":"auto"}`)}
}
func readFixture(t *testing.T, name string) string {
	t.Helper()
	data, err := os.ReadFile("testdata/" + name)
	if err != nil {
		t.Fatal(err)
	}
	return string(data)
}
func jsonValue(t *testing.T, input []byte) any {
	t.Helper()
	var result any
	if err := json.Unmarshal(input, &result); err != nil {
		t.Fatal(err)
	}
	return result
}
func noEnvironment(string) (string, bool) { return "", false }
func newTestProvider(client *http.Client) *Provider {
	provider := NewProvider(client)
	provider.LookupEnv = noEnvironment
	return provider
}

// request.json is hand-derived from the current Rust anthropic/payload.rs,
// including its unnormalized outgoing tool IDs and schema projection.
func TestPayloadReference(t *testing.T) {
	model, input, options := fixtureModel(), fixtureContext(), fixtureOptions()
	compat, err := resolveCompatibility(model)
	if err != nil {
		t.Fatal(err)
	}
	payload, err := buildPayload(model, input, options, true, compat)
	if err != nil {
		t.Fatal(err)
	}
	want := jsonValue(t, []byte(readFixture(t, "request.json")))
	if got := jsonValue(t, payload); !reflect.DeepEqual(got, want) {
		t.Fatalf("payload mismatch\ngot %s\nwant %v", payload, want)
	}
}

func TestThinkingCacheAndCompatibility(t *testing.T) {
	for _, test := range []struct {
		name             string
		effort           *types.ThinkingEffort
		adaptive         bool
		mapping          map[types.ThinkingEffort]*string
		thinking, output string
		temperature      bool
	}{
		{"unset", nil, false, nil, "null", "null", true},
		{"off", pointer(types.ThinkingOff), false, nil, `{"type":"disabled"}`, "null", true},
		{"low", pointer(types.ThinkingLow), false, nil, `{"type":"enabled","budget_tokens":1024,"display":"summarized"}`, "null", false},
		{"adaptive mapped", pointer(types.ThinkingMax), true, map[types.ThinkingEffort]*string{types.ThinkingMax: pointer("high")}, `{"type":"adaptive","display":"summarized"}`, `{"effort":"high"}`, false},
		{"adaptive null", pointer(types.ThinkingMax), true, map[types.ThinkingEffort]*string{types.ThinkingMax: nil}, `{"type":"adaptive","display":"summarized"}`, "null", false},
		{"adaptive absent", pointer(types.ThinkingMax), true, map[types.ThinkingEffort]*string{}, `{"type":"adaptive","display":"summarized"}`, `{"effort":"max"}`, false},
	} {
		t.Run(test.name, func(t *testing.T) {
			model := fixtureModel()
			model.ThinkingEffortMap = test.mapping
			model.Compat = json.RawMessage(fmt.Sprintf(`{"forceAdaptiveThinking":%t}`, test.adaptive))
			options := types.SimpleStreamOptions{Base: types.StreamOptions{Temperature: pointer(0.3)}, Reasoning: test.effort}
			compat, err := resolveCompatibility(model)
			if err != nil {
				t.Fatal(err)
			}
			data, err := buildPayload(model, types.Context{}, options, false, compat)
			if err != nil {
				t.Fatal(err)
			}
			payload := jsonValue(t, data).(map[string]any)
			if !reflect.DeepEqual(payload["thinking"], jsonValue(t, []byte(test.thinking))) || !reflect.DeepEqual(payload["output_config"], jsonValue(t, []byte(test.output))) {
				t.Fatalf("thinking payload %s", data)
			}
			_, temperature := payload["temperature"]
			if temperature != test.temperature {
				t.Fatalf("temperature payload %s", data)
			}
		})
	}
	for _, effort := range []types.ThinkingEffort{types.ThinkingMax, types.ThinkingXHigh} {
		options := fixtureOptions()
		options.Reasoning = &effort
		compat, _ := resolveCompatibility(fixtureModel())
		data, err := buildPayload(fixtureModel(), types.Context{}, options, false, compat)
		if err != nil {
			t.Fatal(err)
		}
		if jsonValue(t, data).(map[string]any)["thinking"].(map[string]any)["budget_tokens"] != float64(4096) {
			t.Fatal(string(data))
		}
	}
	model := fixtureModel()
	model.Provider = "fireworks"
	compat, _ := resolveCompatibility(model)
	data, err := buildPayload(model, fixtureContext(), fixtureOptions(), false, compat)
	if err != nil {
		t.Fatal(err)
	}
	payload := jsonValue(t, data).(map[string]any)
	tool := payload["tools"].([]any)[0].(map[string]any)
	if tool["eager_input_streaming"] != nil || tool["cache_control"] != nil || payload["system"].([]any)[0].(map[string]any)["cache_control"].(map[string]any)["ttl"] != nil {
		t.Fatal(string(data))
	}
	headers := buildHeaders(model, types.SimpleStreamOptions{Base: types.StreamOptions{SessionID: pointer("session")}}, false, compat, fixtureContext())
	if headers.Get("x-session-affinity") != "session" || !strings.Contains(headers.Get("anthropic-beta"), "fine-grained-tool-streaming") {
		t.Fatal(headers)
	}
}

func TestNonVisionAndAssistantCachePlacement(t *testing.T) {
	model := fixtureModel()
	model.Input = []string{"text"}
	compat, _ := resolveCompatibility(model)
	image := types.ImageBlock{Data: "aW1n", MimeType: "image/png"}
	input := types.Context{Messages: []types.Message{types.UserMessage{Content: types.UserBlocks{image, image, types.TextBlock{Text: "middle"}, image}}, types.ToolResultMessage{Content: []types.ContentBlock{image, image}}, types.AssistantMessage{Content: []types.ContentBlock{types.TextBlock{Text: "last"}}}}}
	data, err := buildPayload(model, input, types.SimpleStreamOptions{}, false, compat)
	if err != nil {
		t.Fatal(err)
	}
	messages := jsonValue(t, data).(map[string]any)["messages"].([]any)
	blocks := messages[0].(map[string]any)["content"].([]any)
	if len(blocks) != 3 || blocks[0].(map[string]any)["text"] != "(image omitted: model does not support images)" {
		t.Fatal(string(data))
	}
	tool := messages[1].(map[string]any)["content"].([]any)[0].(map[string]any)
	if tool["content"].([]any)[0].(map[string]any)["text"] != "(tool image omitted: model does not support images)" {
		t.Fatal(string(data))
	}
	if strings.Contains(string(data), "cache_control") {
		t.Fatal("cache must not be attached when last message is assistant")
	}
}

func TestRequestAuthHeadersAndEndpoints(t *testing.T) {
	for _, test := range []struct {
		provider, key, header, value string
		oauth                        bool
	}{
		{"anthropic", "fake-api-key", "x-api-key", "fake-api-key", false},
		{"anthropic", "prefix-sk-ant-oat-fake", "authorization", "Bearer prefix-sk-ant-oat-fake", true},
		{"github-copilot", "fake-copilot", "authorization", "Bearer fake-copilot", false},
		{"cloudflare-ai-gateway", "fake-gateway", "cf-aig-authorization", "Bearer fake-gateway", false},
	} {
		t.Run(test.provider+test.header, func(t *testing.T) {
			var requests atomic.Int32
			server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
				requests.Add(1)
				if request.URL.Path != "/v1/messages" || request.Method != "POST" {
					t.Error(request.Method, request.URL.Path)
				}
				if request.Header.Get(test.header) != test.value || request.Header.Get("x-custom") != "option" {
					t.Error(request.Header)
				}
				if (request.Header.Get("x-app") == "cli") != test.oauth {
					t.Error(request.Header)
				}
				body, err := io.ReadAll(request.Body)
				if err != nil {
					t.Error(err)
				}
				var payload map[string]any
				if err := json.Unmarshal(body, &payload); err != nil {
					t.Error(err)
				}
				if payload["model"] != "claude-fixture" {
					t.Error(string(body))
				}
				fmt.Fprint(writer, emptyMessage())
			}))
			defer server.Close()
			model := fixtureModel()
			model.BaseURL = server.URL + "/v1/"
			model.Provider = test.provider
			model.Headers = map[string]string{"x-custom": "model"}
			options := types.StreamOptions{APIKey: test.key, Headers: map[string]string{"X-Custom": "option", test.header: "wrong"}}
			var events []types.StreamEvent
			provider := newTestProvider(server.Client())
			if err := provider.Stream(context.Background(), model, types.Context{}, options, func(event types.StreamEvent) { events = append(events, event) }); err != nil {
				t.Fatal(err)
			}
			if requests.Load() != 1 || len(events) != 2 || provider.API() != types.APIAnthropicMessages {
				t.Fatal(requests.Load(), events)
			}
			if _, ok := events[1].(types.Done); !ok {
				t.Fatal(events)
			}
		})
	}
	for base, want := range map[string]string{"": "https://api.anthropic.com/v1/messages", " https://example.test/ ": "https://example.test/v1/messages", "https://example.test/v1/": "https://example.test/v1/messages", "https://example.test/custom/messages": "https://example.test/custom/messages"} {
		got, err := resolveEndpoint(base)
		if err != nil || got != want {
			t.Fatal(base, got, err)
		}
	}
	for _, base := range []string{"file:///tmp/provider", "://", "https:///messages"} {
		if _, err := resolveEndpoint(base); err == nil {
			t.Fatal(base)
		}
	}
}

func emptyMessage() string {
	return sse("message_start", `{"message":{"id":"empty"}}`) + sse("message_stop", `{}`)
}
func sse(event, data string) string { return "event: " + event + "\ndata: " + data + "\n\n" }

type roundTripFunc func(*http.Request) (*http.Response, error)

func (call roundTripFunc) RoundTrip(request *http.Request) (*http.Response, error) {
	return call(request)
}

type byteReader struct{ input string }

func (reader *byteReader) Read(output []byte) (int, error) {
	if reader.input == "" {
		return 0, io.EOF
	}
	output[0] = reader.input[0]
	reader.input = reader.input[1:]
	return 1, nil
}

func TestStreamReferenceFragmentsAndSnapshots(t *testing.T) {
	fixture := readFixture(t, "tool.sse")
	for _, input := range []string{fixture, strings.ReplaceAll(fixture, "\n", "\r\n"), strings.TrimRight(fixture, "\n")} {
		client := &http.Client{Transport: roundTripFunc(func(*http.Request) (*http.Response, error) {
			return &http.Response{StatusCode: 200, Body: io.NopCloser(&byteReader{input: input}), Header: make(http.Header)}, nil
		})}
		var events []types.StreamEvent
		err := newTestProvider(client).StreamSimple(context.Background(), fixtureModel(), fixtureContext(), fixtureOptions(), func(event types.StreamEvent) { events = append(events, event) })
		if err != nil {
			t.Fatal(err)
		}
		var names []string
		for _, event := range events {
			names = append(names, reflect.TypeOf(event).Name())
		}
		want := []string{"Start", "ThinkingStart", "ThinkingDelta", "ThinkingEnd", "ThinkingStart", "ThinkingEnd", "TextStart", "TextDelta", "TextDelta", "TextEnd", "ToolCallStart", "ToolCallDelta", "ToolCallDelta", "ToolCallEnd", "Done"}
		if !reflect.DeepEqual(names, want) {
			t.Fatalf("events %v", names)
		}
		done := events[len(events)-1].(types.Done)
		message := done.Message
		if done.Reason != types.ToolUse || *message.ResponseID != "msg_fixture" || message.API != types.APIAnthropicMessages || message.Timestamp <= 0 || len(message.Content) != 4 {
			t.Fatal(done)
		}
		thinking := message.Content[0].(types.ThinkingBlock)
		if thinking.Thinking != "思考🌹" || *thinking.Signature != "signature" {
			t.Fatal(thinking)
		}
		if content := message.Content[1].(types.ThinkingBlock); !content.Redacted || *content.Signature != "opaque" || content.Thinking != "[Reasoning redacted]" {
			t.Fatal(content)
		}
		if message.Content[2].(types.TextBlock).Text != "你好🌹!" {
			t.Fatal(message.Content)
		}
		tool := message.Content[3].(types.ToolCallBlock)
		if tool.ID != "tool_1-" || tool.Name != "read" || !reflect.DeepEqual(jsonValue(t, tool.Arguments), map[string]any{"path": "目录/🌹"}) {
			t.Fatal(tool)
		}
		usage := message.Usage
		if usage.Input != 0 || usage.Output != 7 || usage.CacheRead != 3 || usage.CacheWrite != 4 || usage.TotalTokens != 14 {
			t.Fatal(usage)
		}
		if usage.Cost.Total != usage.Cost.Input+usage.Cost.Output+usage.Cost.CacheRead+usage.Cost.CacheWrite || usage.Cost.Output != 15.0/1_000_000*7 {
			t.Fatal(usage.Cost)
		}
		if *events[1].(types.ThinkingStart).Partial.Content[0].(types.ThinkingBlock).Signature != "" || events[7].(types.TextDelta).Partial.Content[2].(types.TextBlock).Text != "你好🌹" {
			t.Fatal("snapshots mutated")
		}
		if string(events[10].(types.ToolCallStart).Partial.Content[3].(types.ToolCallBlock).Arguments) != "{}" {
			t.Fatal("tool start mutated")
		}
	}
}

func TestFailureStreamsNeverEmitDone(t *testing.T) {
	start := sse("message_start", `{"message":{"id":"failure"}}`)
	text := sse("content_block_start", `{"index":0,"content_block":{"type":"text"}}`)
	tool := sse("content_block_start", `{"index":0,"content_block":{"type":"tool_use","id":"t","name":"read"}}`)
	for name, fixture := range map[string]string{
		"empty": "", "comments": ": ping\n\n",
		"malformed":             sse("message_start", `{`),
		"missing message":       sse("message_start", `{}`),
		"null":                  sse("message_start", `null`),
		"interrupted":           start,
		"stop without start":    sse("message_stop", `{}`),
		"error":                 start + sse("error", `{"error":{"type":"overloaded_error","message":"busy"}}`),
		"error at EOF":          emptyMessage() + "event: error\ndata: {\"error\":{\"message\":\"late\"}}",
		"delta at EOF":          start + "event: message_delta\ndata: {",
		"invalid utf8":          start + text + sse("content_block_delta", "{\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"\xff\"}}"),
		"missing index":         start + sse("content_block_stop", `{}`),
		"negative index":        start + sse("content_block_start", `{"index":-1,"content_block":{"type":"text"}}`),
		"missing block":         start + sse("content_block_start", `{"index":0}`),
		"unmapped delta":        start + sse("content_block_delta", `{"index":7,"delta":{"type":"text_delta","text":"oops"}}`),
		"wrong block delta":     start + text + sse("content_block_delta", `{"index":0,"delta":{"type":"input_json_delta","partial_json":"{}"}}`),
		"missing delta":         start + text + sse("content_block_delta", `{"index":0}`),
		"duplicate block":       start + text + text,
		"duplicate start":       start + start,
		"open block":            start + text + sse("message_stop", `{}`),
		"closed block delta":    start + text + sse("content_block_stop", `{"index":0}`) + sse("content_block_delta", `{"index":0,"delta":{"type":"text_delta","text":"late"}}`),
		"invalid tool JSON":     start + tool + sse("content_block_delta", `{"index":0,"delta":{"type":"input_json_delta","partial_json":"{\"path\":"}}`) + sse("content_block_stop", `{"index":0}`),
		"negative usage":        sse("message_start", `{"message":{"usage":{"input_tokens":-1}}}`),
		"missing message delta": start + sse("message_delta", `{}`),
	} {
		t.Run(name, func(t *testing.T) {
			var requests atomic.Int32
			server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) { requests.Add(1); fmt.Fprint(writer, fixture) }))
			defer server.Close()
			model := fixtureModel()
			model.BaseURL = server.URL
			var events []types.StreamEvent
			err := newTestProvider(server.Client()).Stream(context.Background(), model, types.Context{}, types.StreamOptions{APIKey: "fake"}, func(event types.StreamEvent) { events = append(events, event) })
			if err == nil || requests.Load() != 1 {
				t.Fatal(err, requests.Load())
			}
			assertFailure(t, events, types.Error)
		})
	}
}

func assertFailure(t *testing.T, events []types.StreamEvent, reason types.StopReason) {
	t.Helper()
	terminals := 0
	for _, event := range events {
		switch event := event.(type) {
		case types.Done:
			t.Fatalf("unexpected Done: %#v", event)
		case types.StreamError:
			terminals++
			if event.Reason != reason || event.Error.StopReason != reason || event.Error.ErrorMessage == nil || *event.Error.ErrorMessage == "" || len(event.Error.Content) != 0 {
				t.Fatal(event)
			}
		}
	}
	if terminals != 1 {
		t.Fatalf("terminal errors=%d, events=%v", terminals, events)
	}
	if _, ok := events[len(events)-1].(types.StreamError); !ok {
		t.Fatal("error was not last event")
	}
}

func TestStopReasonsAndUsage(t *testing.T) {
	for reason, want := range map[string]types.StopReason{"end_turn": types.Stop, "pause_turn": types.Stop, "stop_sequence": types.Stop, "max_tokens": types.Length, "tool_use": types.ToolUse, "refusal": types.Error, "sensitive": types.Error, "future_reason": types.Error} {
		t.Run(reason, func(t *testing.T) {
			fixture := sse("message_start", `{"message":{"usage":{"input_tokens":10,"output_tokens":2,"cache_read_input_tokens":3,"cache_creation_input_tokens":4}}}`) + sse("message_delta", fmt.Sprintf(`{"delta":{"stop_reason":%q},"usage":{"cache_creation_input_tokens":0}}`, reason)) + sse("message_stop", `{}`)
			client := &http.Client{Transport: roundTripFunc(func(*http.Request) (*http.Response, error) {
				return &http.Response{StatusCode: 200, Body: io.NopCloser(strings.NewReader(fixture))}, nil
			})}
			var events []types.StreamEvent
			err := newTestProvider(client).Stream(context.Background(), fixtureModel(), types.Context{}, types.StreamOptions{APIKey: "fake"}, func(event types.StreamEvent) { events = append(events, event) })
			if want == types.Error {
				if err == nil {
					t.Fatal("expected error")
				}
				assertFailure(t, events, types.Error)
				return
			}
			if err != nil {
				t.Fatal(err)
			}
			done := events[len(events)-1].(types.Done)
			if done.Reason != want || done.Message.Usage.Input != 10 || done.Message.Usage.CacheWrite != 0 || done.Message.Usage.TotalTokens != 15 {
				t.Fatal(done)
			}
		})
	}
}

func TestHTTPFailuresAndNoRetry(t *testing.T) {
	for _, status := range []int{401, 403, 429, 500, 503} {
		t.Run(fmt.Sprint(status), func(t *testing.T) {
			var requests atomic.Int32
			server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
				requests.Add(1)
				writer.WriteHeader(status)
				fmt.Fprint(writer, `{"error":{"message":"fixture quota/auth failure"}}`)
			}))
			defer server.Close()
			model := fixtureModel()
			model.BaseURL = server.URL
			var events []types.StreamEvent
			err := newTestProvider(server.Client()).Stream(context.Background(), model, types.Context{}, types.StreamOptions{APIKey: "fake"}, func(event types.StreamEvent) { events = append(events, event) })
			var httpErr *HTTPError
			if !errors.As(err, &httpErr) || httpErr.StatusCode != status || !strings.Contains(httpErr.Body, "quota/auth failure") || requests.Load() != 1 {
				t.Fatal(err, requests.Load())
			}
			if len(events) != 1 {
				t.Fatal(events)
			}
			assertFailure(t, events, types.Error)
		})
	}
}

func TestCancelTimeoutAndDisconnectedBody(t *testing.T) {
	for _, mode := range []string{"before request", "before headers", "during stream", "timeout"} {
		t.Run(mode, func(t *testing.T) {
			var requests atomic.Int32
			server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
				requests.Add(1)
				// Drain the POST body so net/http can observe disconnect while the
				// handler waits without sending response headers.
				if _, err := io.Copy(io.Discard, request.Body); err != nil {
					return
				}
				if mode == "before headers" || mode == "timeout" {
					<-request.Context().Done()
					return
				}
				fmt.Fprint(writer, sse("message_start", `{"message":{"id":"cancel"}}`)+sse("content_block_start", `{"index":0,"content_block":{"type":"text"}}`)+sse("content_block_delta", `{"index":0,"delta":{"type":"text_delta","text":"partial"}}`))
				writer.(http.Flusher).Flush()
				<-request.Context().Done()
			}))
			defer server.Close()
			ctx, cancel := context.WithCancel(context.Background())
			defer cancel()
			if mode == "before request" {
				cancel()
			}
			model := fixtureModel()
			model.BaseURL = server.URL
			options := types.StreamOptions{APIKey: "fake", Timeout: time.Second}
			if mode == "before headers" {
				timer := time.AfterFunc(30*time.Millisecond, cancel)
				defer timer.Stop()
			}
			if mode == "timeout" {
				options.Timeout = 30 * time.Millisecond
			}
			var events []types.StreamEvent
			err := newTestProvider(server.Client()).Stream(ctx, model, types.Context{}, options, func(event types.StreamEvent) {
				events = append(events, event)
				if _, ok := event.(types.TextDelta); ok {
					cancel()
				}
			})
			wantErr, wantReason := context.Canceled, types.Aborted
			if mode == "timeout" {
				wantErr, wantReason = context.DeadlineExceeded, types.Error
			}
			if !errors.Is(err, wantErr) {
				t.Fatal(err)
			}
			assertFailure(t, events, wantReason)
			if mode == "before request" && requests.Load() != 0 {
				t.Fatal("canceled call reached server")
			}
		})
	}
	client := &http.Client{Transport: roundTripFunc(func(*http.Request) (*http.Response, error) {
		return &http.Response{StatusCode: 200, Body: &failedBody{input: strings.NewReader(emptyMessage())}}, nil
	})}
	var events []types.StreamEvent
	err := newTestProvider(client).Stream(context.Background(), fixtureModel(), types.Context{}, types.StreamOptions{APIKey: "fake"}, func(event types.StreamEvent) { events = append(events, event) })
	if !errors.Is(err, io.ErrUnexpectedEOF) {
		t.Fatal(err)
	}
	assertFailure(t, events, types.Error)
}

type failedBody struct{ input *strings.Reader }

func (body *failedBody) Read(output []byte) (int, error) {
	count, err := body.input.Read(output)
	if err == io.EOF {
		err = io.ErrUnexpectedEOF
	}
	return count, err
}
func (*failedBody) Close() error { return nil }

func TestEnvironmentResolutionWithoutRealCredentials(t *testing.T) {
	for _, test := range []struct {
		provider                string
		env                     map[string]string
		explicit, header, value string
	}{
		{"anthropic", map[string]string{"ANTHROPIC_OAUTH_TOKEN": "sk-ant-oat-fake", "ANTHROPIC_API_KEY": "fake-api"}, "", "authorization", "Bearer sk-ant-oat-fake"},
		{"anthropic", map[string]string{"ANTHROPIC_API_KEY": "fake-api"}, "", "x-api-key", "fake-api"},
		{"anthropic", map[string]string{"ANTHROPIC_OAUTH_TOKEN": "sk-ant-oat-fake"}, "explicit", "x-api-key", "explicit"},
		{"cloudflare-ai-gateway", map[string]string{"CLOUDFLARE_API_KEY": "fake-cf"}, "", "cf-aig-authorization", "Bearer fake-cf"},
	} {
		t.Run(test.value, func(t *testing.T) {
			server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
				if request.Header.Get(test.header) != test.value {
					t.Error(request.Header)
				}
				data, err := io.ReadAll(request.Body)
				if err != nil {
					t.Error(err)
				}
				if !strings.Contains(string(data), `"ttl":"1h"`) {
					t.Error(string(data))
				}
				fmt.Fprint(writer, emptyMessage())
			}))
			defer server.Close()
			model := fixtureModel()
			model.BaseURL = server.URL
			model.Provider = test.provider
			provider := newTestProvider(server.Client())
			provider.LookupEnv = func(key string) (string, bool) {
				if key == "ROZSA_CACHE_RETENTION" {
					return "long", true
				}
				value, ok := test.env[key]
				return value, ok
			}
			if err := provider.Stream(context.Background(), model, types.Context{SystemPrompt: pointer("system")}, types.StreamOptions{APIKey: test.explicit}, func(types.StreamEvent) {}); err != nil {
				t.Fatal(err)
			}
		})
	}
}

func TestInvalidRequestFailsBeforeHTTP(t *testing.T) {
	for _, name := range []string{"missing key", "bad URL", "invalid UTF-8", "invalid tool JSON", "invalid compat", "invalid metadata", "invalid tool choice", "nil client", "nil environment", "invalid schema UTF-8", "invalid metadata UTF-8", "invalid compat UTF-8"} {
		t.Run(name, func(t *testing.T) {
			var requests atomic.Int32
			client := &http.Client{Transport: roundTripFunc(func(*http.Request) (*http.Response, error) {
				requests.Add(1)
				return nil, errors.New("unexpected HTTP request")
			})}
			provider := newTestProvider(client)
			model := fixtureModel()
			options := types.StreamOptions{APIKey: "fake"}
			input := types.Context{}
			switch name {
			case "missing key":
				options.APIKey = ""
			case "bad URL":
				model.BaseURL = "://"
			case "invalid UTF-8":
				input.Messages = []types.Message{types.UserMessage{Content: types.UserText("\xff")}}
			case "invalid tool JSON":
				input.Tools = []types.ToolSchema{{Name: "bad", Parameters: json.RawMessage(`{`)}}
			case "invalid compat":
				model.Compat = json.RawMessage(`{`)
			case "invalid metadata":
				options.Metadata = json.RawMessage(`{`)
			case "invalid schema UTF-8":
				input.Tools = []types.ToolSchema{{Name: "bad", Parameters: json.RawMessage("{\"properties\":{\"\xff\":{}}}")}}
			case "invalid metadata UTF-8":
				options.Metadata = json.RawMessage("{\"user_id\":\"\xff\"}")
			case "invalid compat UTF-8":
				model.Compat = json.RawMessage("{\"note\":\"\xff\"}")
			case "invalid tool choice": // Covered through StreamSimple below.
			case "nil client":
				provider.Client = nil
			case "nil environment":
				provider.LookupEnv = nil
			}
			var events []types.StreamEvent
			simple := types.SimpleStreamOptions{Base: options}
			if name == "invalid tool choice" {
				simple.ToolChoice = json.RawMessage(`{`)
			}
			err := provider.StreamSimple(context.Background(), model, input, simple, func(event types.StreamEvent) { events = append(events, event) })
			if err == nil || requests.Load() != 0 {
				t.Fatal(err, requests.Load())
			}
			assertFailure(t, events, types.Error)
		})
	}
}

func TestLongDeltaAndConsumerMutation(t *testing.T) {
	text := strings.Repeat("🌹", 20000)
	fixture := sse("message_start", `{"message":{"id":"long"}}`) + sse("content_block_start", `{"index":0,"content_block":{"type":"text"}}`) + sse("content_block_delta", fmt.Sprintf(`{"index":0,"delta":{"type":"text_delta","text":%q}}`, text)) + sse("content_block_stop", `{"index":0}`) + sse("message_stop", `{}`)
	client := &http.Client{Transport: roundTripFunc(func(*http.Request) (*http.Response, error) {
		return &http.Response{StatusCode: 200, Body: io.NopCloser(strings.NewReader(fixture))}, nil
	})}
	var done types.Done
	err := newTestProvider(client).Stream(context.Background(), fixtureModel(), types.Context{}, types.StreamOptions{APIKey: "fake"}, func(event types.StreamEvent) {
		switch event := event.(type) {
		case types.TextDelta:
			event.Partial.Content[0] = types.TextBlock{Text: "consumer changed"}
			*event.Partial.ResponseID = "consumer changed"
		case types.Done:
			done = event
		}
	})
	if err != nil || done.Message.Content[0].(types.TextBlock).Text != text || *done.Message.ResponseID != "long" {
		t.Fatal(err, done)
	}
	if got := normalizeToolID("!你好" + strings.Repeat("a", 70)); got != strings.Repeat("a", 64) {
		t.Fatal(got)
	}
}

func TestRequestTimeoutOverridesClientWithoutMutation(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		fmt.Fprint(writer, emptyMessage())
	}))
	defer server.Close()
	client := server.Client()
	client.Timeout = time.Nanosecond
	provider := newTestProvider(client)
	model := fixtureModel()
	model.BaseURL = server.URL
	if err := provider.Stream(context.Background(), model, types.Context{}, types.StreamOptions{APIKey: "fake", Timeout: time.Second}, func(types.StreamEvent) {}); err != nil {
		t.Fatal(err)
	}
	var events []types.StreamEvent
	err := provider.Stream(context.Background(), model, types.Context{}, types.StreamOptions{APIKey: "fake"}, func(event types.StreamEvent) { events = append(events, event) })
	if !errors.Is(err, context.DeadlineExceeded) {
		t.Fatal(err)
	}
	assertFailure(t, events, types.Error)
}
