package completions

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

func TestStreamSimpleRequestSSEAndToolUsage(t *testing.T) {
	var got map[string]any
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/v1/chat/completions" || r.Method != http.MethodPost {
			t.Errorf("request=%s %s", r.Method, r.URL.Path)
		}
		if r.Header.Get("Authorization") != "Bearer secret" || r.Header.Get("X-Vendor") != "kept" {
			t.Errorf("headers=%v", r.Header)
		}
		if err := json.NewDecoder(r.Body).Decode(&got); err != nil {
			t.Error(err)
		}
		w.Header().Set("Content-Type", "text/event-stream")
		_, _ = fmt.Fprint(w, "data: {\"id\":\"chat-1\",\"model\":\"served-model\",\"choices\":[{\"delta\":{\"content\":\"héllo\"}}]}\n\n")
		_, _ = fmt.Fprint(w, `data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call-1","function":{"name":"lookup","arguments":"{\"q\":"}}]}}]}`+"\n\n")
		_, _ = fmt.Fprint(w, `data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"ab\"}"}}]},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":10,"completion_tokens":3,"prompt_tokens_details":{"cached_tokens":2}}}`+"\n\n")
		_, _ = fmt.Fprint(w, "data: [DONE]\n\n")
	}))
	defer server.Close()
	max := uint32(256)
	model := types.Model{ID: "custom-model", Provider: "custom-compatible", API: types.APIOpenAICompletions, BaseURL: server.URL + "/v1", MaxTokens: int(max), Cost: types.Cost{Input: 2, Output: 4, CacheRead: 1}, Headers: map[string]string{"X-Vendor": "kept"}}
	var events []types.StreamEvent
	err := NewProvider(server.Client()).StreamSimple(context.Background(), model, types.Context{SystemPrompt: str("system"), Messages: []types.Message{types.UserMessage{Content: types.UserText("question")}}, Tools: []types.ToolSchema{{Name: "lookup", Description: "query", Parameters: json.RawMessage(`{"type":"object"}`)}}}, types.SimpleStreamOptions{Base: types.StreamOptions{APIKey: "secret"}}, func(e types.StreamEvent) { events = append(events, e) })
	if err != nil {
		t.Fatal(err)
	}
	if got["model"] != "custom-model" || got["max_completion_tokens"] != float64(256) || got["stream_options"] == nil {
		t.Fatalf("payload=%#v", got)
	}
	var done *types.Done
	for _, e := range events {
		if v, ok := e.(types.Done); ok {
			done = &v
		}
	}
	if done == nil || done.Reason != types.ToolUse {
		t.Fatalf("terminal=%#v", events[len(events)-1])
	}
	if done.Message.ResponseID == nil || *done.Message.ResponseID != "chat-1" || done.Message.ResponseModel == nil || *done.Message.ResponseModel != "served-model" {
		t.Fatalf("response metadata=%#v", done.Message)
	}
	if done.Message.Usage.Input != 8 || done.Message.Usage.CacheRead != 2 || done.Message.Usage.Output != 3 || done.Message.Usage.Cost.Total == 0 {
		t.Fatalf("usage=%#v", done.Message.Usage)
	}
	var tool types.ToolCallBlock
	var text string
	for _, b := range done.Message.Content {
		switch v := b.(type) {
		case types.TextBlock:
			text = v.Text
		case types.ToolCallBlock:
			tool = v
		}
	}
	if text != "héllo" || tool.ID != "call-1" || tool.Name != "lookup" || string(tool.Arguments) != `{"q":"ab"}` {
		t.Fatalf("content=%#v", done.Message.Content)
	}
}

func TestStreamSimpleReportsHTTPAndTruncatedSSEErrors(t *testing.T) {
	for _, tc := range []struct {
		name   string
		status int
		body   string
		want   string
	}{{"http", 503, `{"error":"offline"}`, "503"}, {"truncated", 200, "data: {bad\n\n", "invalid OpenAI chat chunk"}, {"no finish", 200, "data: {\"choices\":[{\"delta\":{\"content\":\"partial\"}}]}\n\n", "without finish_reason"}} {
		t.Run(tc.name, func(t *testing.T) {
			server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { w.WriteHeader(tc.status); _, _ = fmt.Fprint(w, tc.body) }))
			defer server.Close()
			model := types.Model{ID: "m", Provider: "custom", API: types.APIOpenAICompletions, BaseURL: server.URL}
			var terminal types.StreamEvent
			err := NewProvider(server.Client()).StreamSimple(context.Background(), model, types.Context{}, types.SimpleStreamOptions{}, func(e types.StreamEvent) {
				switch e.(type) {
				case types.Done, types.StreamError:
					terminal = e
				}
			})
			if err == nil || !strings.Contains(err.Error(), tc.want) {
				t.Fatalf("err=%v want containing %q", err, tc.want)
			}
			if _, ok := terminal.(types.StreamError); !ok {
				t.Fatalf("terminal=%T, want StreamError", terminal)
			}
		})
	}
}

func TestStreamSimpleCancellationAborts(t *testing.T) {
	started := make(chan struct{})
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "text/event-stream")
		w.WriteHeader(http.StatusOK)
		w.(http.Flusher).Flush()
		close(started)
		<-r.Context().Done()
	}))
	defer server.Close()
	ctx, cancel := context.WithCancel(context.Background())
	model := types.Model{ID: "m", Provider: "custom", API: types.APIOpenAICompletions, BaseURL: server.URL}
	var terminal types.StreamEvent
	done := make(chan error, 1)
	go func() {
		done <- NewProvider(server.Client()).StreamSimple(ctx, model, types.Context{}, types.SimpleStreamOptions{}, func(e types.StreamEvent) {
			switch e.(type) {
			case types.Done, types.StreamError:
				terminal = e
			}
		})
	}()
	<-started
	cancel()
	err := <-done
	if !errors.Is(err, context.Canceled) {
		t.Fatalf("err=%v", err)
	}
	event, ok := terminal.(types.StreamError)
	if !ok || event.Reason != types.Aborted {
		t.Fatalf("terminal=%#v", terminal)
	}
}

func TestCompatOverridesAndEndpoint(t *testing.T) {
	model := types.Model{ID: "m", Provider: "custom", BaseURL: "https://example.test/v1/", Compat: json.RawMessage(`{"maxTokensField":"MaxTokens","supportsStore":false,"supportsUsageInStreaming":false,"sendSessionAffinityHeaders":true}`)}
	body, c, err := buildPayload(model, types.Context{}, types.SimpleStreamOptions{Base: types.StreamOptions{MaxTokens: intp(42)}})
	if err != nil {
		t.Fatal(err)
	}
	var got map[string]any
	if err = json.Unmarshal(body, &got); err != nil {
		t.Fatal(err)
	}
	if c.maxTokens != "max_tokens" || got["max_tokens"] != float64(42) || got["store"] != nil || got["stream_options"] != nil {
		t.Fatalf("compat=%#v payload=%#v", c, got)
	}
	endpoint, err := resolveEndpoint(model.BaseURL)
	if err != nil || endpoint != "https://example.test/v1/chat/completions" {
		t.Fatalf("endpoint=%q err=%v", endpoint, err)
	}
}

func str(v string) *string { return &v }
func intp(v int) *int      { return &v }
