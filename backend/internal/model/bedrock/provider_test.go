package bedrock

import (
	"bytes"
	"context"
	"encoding/json"
	"encoding/pem"
	"errors"
	"math"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/aws/aws-sdk-go-v2/aws"
	aweventstream "github.com/aws/aws-sdk-go-v2/aws/protocol/eventstream"
	"github.com/aws/aws-sdk-go-v2/credentials"
	"github.com/aws/aws-sdk-go-v2/service/bedrockruntime"
	"github.com/aws/aws-sdk-go-v2/service/bedrockruntime/types"
	modeltypes "rozsa/backend/internal/model/types"
)

func TestBuildMessagesPreservesUserAndAssistantToolUse(t *testing.T) {
	toolCallID := "call-1"
	input := modeltypes.Context{Messages: []modeltypes.Message{
		modeltypes.UserMessage{Content: modeltypes.UserText("question")},
		modeltypes.AssistantMessage{Content: []modeltypes.ContentBlock{modeltypes.TextBlock{Text: "checking"}, modeltypes.ToolCallBlock{ID: toolCallID, Name: "lookup", Arguments: json.RawMessage(`{"q":"x"}`)}}},
	}}
	messages, err := buildMessages(input)
	if err != nil {
		t.Fatal(err)
	}
	if len(messages) != 2 || string(messages[0].Role) != "user" || string(messages[1].Role) != "assistant" {
		t.Fatalf("messages roles = %#v", messages)
	}
	if len(messages[0].Content) != 1 || len(messages[1].Content) != 2 {
		t.Fatalf("messages content sizes = %d, %d", len(messages[0].Content), len(messages[1].Content))
	}
}

func TestBuildMessagesMergesToolResultsAndPreservesSignedThinking(t *testing.T) {
	signature := "verified-signature"
	input := modeltypes.Context{Messages: []modeltypes.Message{
		modeltypes.UserMessage{Content: modeltypes.UserText("question")},
		modeltypes.AssistantMessage{Content: []modeltypes.ContentBlock{
			modeltypes.ThinkingBlock{Thinking: "analysis", Signature: &signature},
			modeltypes.ToolCallBlock{ID: "call-1", Name: "lookup", Arguments: json.RawMessage(`{"key":"value"}`)},
		}},
		modeltypes.ToolResultMessage{ToolCallID: "call-1", Content: []modeltypes.ContentBlock{modeltypes.TextBlock{Text: "first"}}},
		modeltypes.ToolResultMessage{ToolCallID: "call-2", Content: []modeltypes.ContentBlock{modeltypes.TextBlock{Text: "second"}}, IsError: true},
	}}
	messages, err := buildMessages(input)
	if err != nil {
		t.Fatal(err)
	}
	if len(messages) != 3 || len(messages[2].Content) != 2 {
		t.Fatalf("messages = %#v; want user, assistant, merged user results", messages)
	}
	if _, ok := messages[1].Content[0].(*types.ContentBlockMemberReasoningContent); !ok {
		t.Fatalf("signed thinking content = %T", messages[1].Content[0])
	}
	secondResult, ok := messages[2].Content[1].(*types.ContentBlockMemberToolResult)
	if !ok || secondResult.Value.Status != types.ToolResultStatusError {
		t.Fatalf("second tool result = %#v", messages[2].Content[1])
	}
}

func TestBuildRequestAppliesBedrockToolCacheAndReasoningFields(t *testing.T) {
	temperature := 0.3
	systemPrompt := "System instructions"
	model := modeltypes.Model{ID: "anthropic.claude-opus-4-6-v1:0", Name: "Claude Opus 4.6", Reasoning: true, MaxTokens: 4096}
	input := modeltypes.Context{SystemPrompt: &systemPrompt, Messages: []modeltypes.Message{modeltypes.UserMessage{Content: modeltypes.UserText("question")}}, Tools: []modeltypes.ToolSchema{{Name: "lookup", Description: "look up", Parameters: json.RawMessage(`{"type":"object"}`)}}}
	options := modeltypes.SimpleStreamOptions{Base: modeltypes.StreamOptions{CacheRetention: modeltypes.CacheLong, Temperature: &temperature}, Reasoning: pointer(modeltypes.ThinkingHigh)}
	request, err := buildRequest(model, input, options)
	if err != nil {
		t.Fatal(err)
	}
	if len(request.System) != 2 || request.ToolConfig == nil || len(request.ToolConfig.Tools) != 1 {
		t.Fatalf("request system/tools = %#v / %#v", request.System, request.ToolConfig)
	}
	if len(request.Messages) != 1 || len(request.Messages[0].Content) != 2 {
		t.Fatalf("request messages = %#v", request.Messages)
	}
	if cachePoint, ok := request.Messages[0].Content[1].(*types.ContentBlockMemberCachePoint); !ok || cachePoint.Value.Ttl != types.CacheTTLOneHour {
		t.Fatalf("message cache point = %#v", request.Messages[0].Content[1])
	}
	if request.InferenceConfig.MaxTokens == nil || *request.InferenceConfig.MaxTokens != int32(model.MaxTokens) || request.InferenceConfig.Temperature == nil || *request.InferenceConfig.Temperature != float32(temperature) {
		t.Fatalf("inference config = %#v", request.InferenceConfig)
	}
	if request.AdditionalModelRequestFields == nil {
		t.Fatal("missing adaptive thinking fields")
	}
	var fields map[string]any
	encodedFields, err := request.AdditionalModelRequestFields.MarshalSmithyDocument()
	if err != nil {
		t.Fatal(err)
	}
	if err := json.Unmarshal(encodedFields, &fields); err != nil {
		t.Fatal(err)
	}
	if fields["thinking"].(map[string]any)["type"] != "adaptive" || fields["output_config"].(map[string]any)["effort"] != "high" {
		t.Fatalf("additional model fields = %#v", fields)
	}
}

func pointer[T any](value T) *T { return &value }

func TestStreamSimpleDecodesBinaryEventStreamTextAndUsage(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(responseWriter http.ResponseWriter, request *http.Request) {
		responseWriter.Header().Set("Content-Type", "application/vnd.amazon.eventstream")
		responseWriter.WriteHeader(http.StatusOK)
		encoder := aweventstream.NewEncoder()
		frames := []struct{ event, payload string }{
			{"messageStart", `{"role":"assistant"}`},
			{"contentBlockDelta", `{"contentBlockIndex":0,"delta":{"text":"hel"}}`},
			{"contentBlockDelta", `{"contentBlockIndex":0,"delta":{"text":"lo"}}`},
			{"contentBlockStop", `{"contentBlockIndex":0}`},
			{"contentBlockStart", `{"contentBlockIndex":1,"start":{"toolUse":{"toolUseId":"call-1","name":"lookup"}}}`},
			{"contentBlockDelta", `{"contentBlockIndex":1,"delta":{"toolUse":{"input":"{\"q\":"}}}`},
			{"contentBlockDelta", `{"contentBlockIndex":1,"delta":{"toolUse":{"input":"\"x\"}"}}}`},
			{"contentBlockStop", `{"contentBlockIndex":1}`},
			{"contentBlockDelta", `{"contentBlockIndex":2,"delta":{"reasoningContent":{"text":"thinking"}}}`},
			{"contentBlockDelta", `{"contentBlockIndex":2,"delta":{"reasoningContent":{"signature":"signature"}}}`},
			{"contentBlockStop", `{"contentBlockIndex":2}`},
			{"contentBlockDelta", `{"contentBlockIndex":3,"delta":{"reasoningContent":{"redactedContent":"AAE="}}}`},
			{"contentBlockStop", `{"contentBlockIndex":3}`},
			{"metadata", `{"usage":{"inputTokens":10,"outputTokens":2,"totalTokens":12,"cacheReadInputTokens":3},"metrics":{"latencyMs":1}}`},
			{"messageStop", `{"stopReason":"tool_use"}`},
		}
		for _, frame := range frames {
			headers := aweventstream.Headers{}
			headers.Set(":message-type", aweventstream.StringValue("event"))
			headers.Set(":event-type", aweventstream.StringValue(frame.event))
			headers.Set(":content-type", aweventstream.StringValue("application/json"))
			if err := encoder.Encode(responseWriter, aweventstream.Message{Headers: headers, Payload: []byte(frame.payload)}); err != nil {
				t.Errorf("encode %s frame: %v", frame.event, err)
				return
			}
		}
	}))
	defer server.Close()
	awsConfig := aws.Config{Region: "us-west-2", Credentials: credentials.NewStaticCredentialsProvider("fake-access", "fake-secret", "")}
	client := bedrockruntime.NewFromConfig(awsConfig, func(clientOptions *bedrockruntime.Options) { clientOptions.BaseEndpoint = aws.String(server.URL) })
	model := modeltypes.Model{ID: "amazon.nova-test", Provider: "amazon-bedrock", Cost: modeltypes.Cost{Input: 1, Output: 2, CacheRead: 3}}
	var events []modeltypes.StreamEvent
	err := NewProvider(client).StreamSimple(context.Background(), model, modeltypes.Context{Messages: []modeltypes.Message{modeltypes.UserMessage{Content: modeltypes.UserText("question")}}}, modeltypes.SimpleStreamOptions{}, func(streamEvent modeltypes.StreamEvent) { events = append(events, streamEvent) })
	if err != nil {
		t.Fatal(err)
	}
	var done *modeltypes.Done
	for i := range events {
		if value, ok := events[i].(modeltypes.Done); ok {
			done = &value
		}
	}
	if done == nil {
		t.Fatalf("events = %#v, want Done", events)
	}
	if done.Message.StopReason != modeltypes.ToolUse || len(done.Message.Content) != 4 {
		t.Fatalf("done = %#v", done)
	}
	textBlock, ok := done.Message.Content[0].(modeltypes.TextBlock)
	if !ok || textBlock.Text != "hello" {
		t.Fatalf("content = %#v", done.Message.Content)
	}
	var textDeltas []modeltypes.TextDelta
	for _, streamEvent := range events {
		if textDelta, isTextDelta := streamEvent.(modeltypes.TextDelta); isTextDelta {
			textDeltas = append(textDeltas, textDelta)
		}
	}
	if len(textDeltas) != 2 {
		t.Fatalf("text deltas = %#v", textDeltas)
	}
	firstPartial := textDeltas[0].Partial.Content[0].(modeltypes.TextBlock)
	secondPartial := textDeltas[1].Partial.Content[0].(modeltypes.TextBlock)
	if firstPartial.Text != "hel" || secondPartial.Text != "hello" {
		t.Fatalf("delta snapshots mutated: first=%q second=%q", firstPartial.Text, secondPartial.Text)
	}
	toolCall, ok := done.Message.Content[1].(modeltypes.ToolCallBlock)
	if !ok || toolCall.ID != "call-1" || toolCall.Name != "lookup" || string(toolCall.Arguments) != `{"q":"x"}` {
		t.Fatalf("tool call = %#v", done.Message.Content[1])
	}
	thinkingBlock, ok := done.Message.Content[2].(modeltypes.ThinkingBlock)
	if !ok || thinkingBlock.Thinking != "thinking" || thinkingBlock.Signature == nil || *thinkingBlock.Signature != "signature" {
		t.Fatalf("thinking block = %#v", done.Message.Content[2])
	}
	redactedBlock, ok := done.Message.Content[3].(modeltypes.ThinkingBlock)
	if !ok || !redactedBlock.Redacted {
		t.Fatalf("redacted block = %#v", done.Message.Content[3])
	}
	if done.Message.Usage.Input != 10 || done.Message.Usage.Output != 2 || done.Message.Usage.CacheRead != 3 || math.Abs(done.Message.Usage.Cost.Total-0.000023) > 1e-12 {
		t.Fatalf("usage = %#v", done.Message.Usage)
	}
}

func TestStreamSimpleRejectsCorruptAndTruncatedEventFrames(t *testing.T) {
	for _, testCase := range []struct {
		name   string
		mutate func([]byte) []byte
	}{
		{name: "checksum", mutate: func(frame []byte) []byte { frame[len(frame)-1] ^= 0xff; return frame }},
		{name: "truncated", mutate: func(frame []byte) []byte { return frame[:len(frame)/2] }},
	} {
		t.Run(testCase.name, func(t *testing.T) {
			frame := encodeEvent(t, "messageStart", `{"role":"assistant"}`)
			frame = testCase.mutate(frame)
			server := httptest.NewServer(http.HandlerFunc(func(responseWriter http.ResponseWriter, _ *http.Request) {
				responseWriter.Header().Set("Content-Type", "application/vnd.amazon.eventstream")
				responseWriter.WriteHeader(http.StatusOK)
				_, _ = responseWriter.Write(frame)
			}))
			defer server.Close()
			provider := NewProvider(testClient(server.URL))
			var events []modeltypes.StreamEvent
			err := provider.StreamSimple(context.Background(), modeltypes.Model{ID: "m", Provider: "amazon-bedrock"}, modeltypes.Context{}, modeltypes.SimpleStreamOptions{}, func(streamEvent modeltypes.StreamEvent) { events = append(events, streamEvent) })
			if err == nil {
				t.Fatal("StreamSimple() succeeded for invalid event frame")
			}
			if len(events) == 0 {
				t.Fatal("missing terminal error event")
			}
			if _, ok := events[len(events)-1].(modeltypes.StreamError); !ok {
				t.Fatalf("last event = %T, want StreamError", events[len(events)-1])
			}
		})
	}
}

func TestStreamSimpleMapsEventStreamException(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(responseWriter http.ResponseWriter, _ *http.Request) {
		responseWriter.Header().Set("Content-Type", "application/vnd.amazon.eventstream")
		responseWriter.WriteHeader(http.StatusOK)
		header := aweventstream.Headers{}
		header.Set(":message-type", aweventstream.StringValue("exception"))
		header.Set(":exception-type", aweventstream.StringValue("InternalServerException"))
		header.Set(":content-type", aweventstream.StringValue("application/json"))
		if err := aweventstream.NewEncoder().Encode(responseWriter, aweventstream.Message{Headers: header, Payload: []byte(`{"message":"service stream failed"}`)}); err != nil {
			t.Errorf("encode exception: %v", err)
		}
	}))
	defer server.Close()
	var events []modeltypes.StreamEvent
	err := NewProvider(testClient(server.URL)).StreamSimple(context.Background(), modeltypes.Model{ID: "m", Provider: "amazon-bedrock"}, modeltypes.Context{}, modeltypes.SimpleStreamOptions{}, func(streamEvent modeltypes.StreamEvent) { events = append(events, streamEvent) })
	if err == nil || !strings.Contains(strings.ToLower(err.Error()), "internalserverexception") {
		t.Fatalf("error = %v", err)
	}
	if len(events) == 0 {
		t.Fatal("missing terminal error event")
	}
}

func TestStreamSimpleCancellationWhileReadingEventStream(t *testing.T) {
	started := make(chan struct{})
	server := httptest.NewServer(http.HandlerFunc(func(responseWriter http.ResponseWriter, request *http.Request) {
		responseWriter.Header().Set("Content-Type", "application/vnd.amazon.eventstream")
		responseWriter.WriteHeader(http.StatusOK)
		if flusher, ok := responseWriter.(http.Flusher); ok {
			flusher.Flush()
		}
		close(started)
		<-request.Context().Done()
	}))
	defer server.Close()
	ctx, cancel := context.WithCancel(context.Background())
	var events []modeltypes.StreamEvent
	done := make(chan error, 1)
	go func() {
		done <- NewProvider(testClient(server.URL)).StreamSimple(ctx, modeltypes.Model{ID: "m", Provider: "amazon-bedrock"}, modeltypes.Context{}, modeltypes.SimpleStreamOptions{}, func(streamEvent modeltypes.StreamEvent) { events = append(events, streamEvent) })
	}()
	<-started
	cancel()
	if err := <-done; err == nil {
		t.Fatal("expected cancellation error")
	}
	if len(events) == 0 {
		t.Fatal("missing cancellation terminal event")
	}
	if event, ok := events[len(events)-1].(modeltypes.StreamError); !ok || event.Reason != modeltypes.Aborted {
		t.Fatalf("terminal event = %#v", events[len(events)-1])
	}
}

func encodeEvent(t *testing.T, event, payload string) []byte {
	t.Helper()
	headers := aweventstream.Headers{}
	headers.Set(":message-type", aweventstream.StringValue("event"))
	headers.Set(":event-type", aweventstream.StringValue(event))
	headers.Set(":content-type", aweventstream.StringValue("application/json"))
	var buffer bytes.Buffer
	if err := aweventstream.NewEncoder().Encode(&buffer, aweventstream.Message{Headers: headers, Payload: []byte(payload)}); err != nil {
		t.Fatal(err)
	}
	return buffer.Bytes()
}

func testClient(endpoint string) *bedrockruntime.Client {
	awsConfig := aws.Config{Region: "us-west-2", Credentials: credentials.NewStaticCredentialsProvider("fake-access", "fake-secret", "")}
	return bedrockruntime.NewFromConfig(awsConfig, func(clientOptions *bedrockruntime.Options) { clientOptions.BaseEndpoint = aws.String(endpoint) })
}

func TestStreamSimpleSignsConverseRequestAndMapsServiceError(t *testing.T) {
	var gotAuthorization, gotTarget, gotBody string
	server := httptest.NewServer(http.HandlerFunc(func(responseWriter http.ResponseWriter, request *http.Request) {
		gotAuthorization = request.Header.Get("Authorization")
		gotTarget = request.URL.Path
		var body map[string]any
		if err := json.NewDecoder(request.Body).Decode(&body); err != nil {
			t.Errorf("decode request: %v", err)
		}
		encoded, _ := json.Marshal(body)
		gotBody = string(encoded)
		responseWriter.Header().Set("Content-Type", "application/json")
		responseWriter.WriteHeader(http.StatusBadRequest)
		_, _ = responseWriter.Write([]byte(`{"message":"invalid request","__type":"ValidationException"}`))
	}))
	defer server.Close()
	awsConfig := aws.Config{Region: "us-west-2", Credentials: credentials.NewStaticCredentialsProvider("fake-access", "fake-secret", "")}
	client := bedrockruntime.NewFromConfig(awsConfig, func(clientOptions *bedrockruntime.Options) { clientOptions.BaseEndpoint = aws.String(server.URL) })
	provider := NewProvider(client)
	model := modeltypes.Model{ID: "anthropic.claude-test", Provider: "amazon-bedrock", API: modeltypes.APIBedrockConverseStream}
	var events []modeltypes.StreamEvent
	err := provider.StreamSimple(context.Background(), model, modeltypes.Context{Messages: []modeltypes.Message{modeltypes.UserMessage{Content: modeltypes.UserText("hello")}}}, modeltypes.SimpleStreamOptions{}, func(streamEvent modeltypes.StreamEvent) { events = append(events, streamEvent) })
	if err == nil || !strings.Contains(err.Error(), "ValidationException") {
		t.Fatalf("error = %v, want service ValidationException", err)
	}
	if !strings.HasPrefix(gotAuthorization, "AWS4-HMAC-SHA256 ") {
		t.Fatalf("authorization = %q", gotAuthorization)
	}
	if !strings.Contains(gotTarget, "/model/anthropic.claude-test/converse-stream") {
		t.Fatalf("path = %q", gotTarget)
	}
	if !strings.Contains(gotBody, `"messages"`) || !strings.Contains(gotBody, `"hello"`) {
		t.Fatalf("request body = %s", gotBody)
	}
	if len(events) != 1 {
		t.Fatalf("events = %#v, want one terminal error", events)
	}
	if _, ok := events[0].(modeltypes.StreamError); !ok {
		t.Fatalf("terminal event = %T, want StreamError", events[0])
	}
}

func TestSDKDefaultCredentialChainSignsBedrockRequest(t *testing.T) {
	t.Setenv("AWS_REGION", "us-west-2")
	t.Setenv("AWS_ACCESS_KEY_ID", "environment-access")
	t.Setenv("AWS_SECRET_ACCESS_KEY", "environment-secret")
	t.Setenv("AWS_SESSION_TOKEN", "")
	t.Setenv("AWS_BEDROCK_SKIP_AUTH", "")
	t.Setenv("AWS_BEARER_TOKEN_BEDROCK", "")
	var authorization string
	server := httptest.NewServer(http.HandlerFunc(func(responseWriter http.ResponseWriter, request *http.Request) {
		authorization = request.Header.Get("Authorization")
		responseWriter.Header().Set("Content-Type", "application/json")
		responseWriter.WriteHeader(http.StatusBadRequest)
		_, _ = responseWriter.Write([]byte(`{"message":"fixture error","__type":"ValidationException"}`))
	}))
	defer server.Close()
	model := modeltypes.Model{ID: "amazon.nova-test", Provider: "amazon-bedrock", BaseURL: server.URL}
	err := NewProvider(nil).StreamSimple(context.Background(), model, modeltypes.Context{}, modeltypes.SimpleStreamOptions{}, func(modeltypes.StreamEvent) {})
	if err == nil {
		t.Fatal("expected fixture service error")
	}
	if !strings.HasPrefix(authorization, "AWS4-HMAC-SHA256 ") || !strings.Contains(authorization, "Credential=environment-access/") {
		t.Fatalf("Authorization = %q", authorization)
	}
}

func TestSDKBedrockBearerTokenAuthentication(t *testing.T) {
	t.Setenv("AWS_REGION", "us-west-2")
	t.Setenv("AWS_ACCESS_KEY_ID", "")
	t.Setenv("AWS_SECRET_ACCESS_KEY", "")
	t.Setenv("AWS_BEDROCK_SKIP_AUTH", "")
	t.Setenv("AWS_BEARER_TOKEN_BEDROCK", "fixture-bearer-token")
	var authorization string
	server := httptest.NewTLSServer(http.HandlerFunc(func(responseWriter http.ResponseWriter, request *http.Request) {
		authorization = request.Header.Get("Authorization")
		responseWriter.Header().Set("Content-Type", "application/json")
		responseWriter.WriteHeader(http.StatusBadRequest)
		_, _ = responseWriter.Write([]byte(`{"message":"fixture error","__type":"ValidationException"}`))
	}))
	defer server.Close()
	caPath := filepath.Join(t.TempDir(), "test-ca.pem")
	certificate := server.TLS.Certificates[0].Certificate[0]
	ca := pem.EncodeToMemory(&pem.Block{Type: "CERTIFICATE", Bytes: certificate})
	if err := os.WriteFile(caPath, ca, 0o600); err != nil {
		t.Fatal(err)
	}
	t.Setenv("AWS_CA_BUNDLE", caPath)
	model := modeltypes.Model{ID: "amazon.nova-test", Provider: "amazon-bedrock", BaseURL: server.URL}
	err := NewProvider(nil).StreamSimple(context.Background(), model, modeltypes.Context{}, modeltypes.SimpleStreamOptions{}, func(modeltypes.StreamEvent) {})
	if err == nil {
		t.Fatal("expected fixture service error")
	}
	if authorization != "Bearer fixture-bearer-token" {
		t.Fatalf("Authorization = %q, request error = %v", authorization, err)
	}
}

func TestSDKSkipAuthOverridesBearerToken(t *testing.T) {
	t.Setenv("AWS_REGION", "us-west-2")
	t.Setenv("AWS_BEDROCK_SKIP_AUTH", "1")
	t.Setenv("AWS_BEARER_TOKEN_BEDROCK", "fixture-bearer-token")
	var authorization string
	server := httptest.NewServer(http.HandlerFunc(func(responseWriter http.ResponseWriter, request *http.Request) {
		authorization = request.Header.Get("Authorization")
		responseWriter.Header().Set("Content-Type", "application/json")
		responseWriter.WriteHeader(http.StatusBadRequest)
		_, _ = responseWriter.Write([]byte(`{"message":"fixture error","__type":"ValidationException"}`))
	}))
	defer server.Close()
	model := modeltypes.Model{ID: "amazon.nova-test", Provider: "amazon-bedrock", BaseURL: server.URL}
	err := NewProvider(nil).StreamSimple(context.Background(), model, modeltypes.Context{}, modeltypes.SimpleStreamOptions{}, func(modeltypes.StreamEvent) {})
	if err == nil {
		t.Fatal("expected fixture service error")
	}
	if !strings.HasPrefix(authorization, "AWS4-HMAC-SHA256 ") || !strings.Contains(authorization, "Credential=dummy-access-key/") {
		t.Fatalf("Authorization = %q", authorization)
	}
}

func TestStreamSimpleCancellationBeforeRequest(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(http.ResponseWriter, *http.Request) { t.Error("cancelled request reached server") }))
	defer server.Close()
	awsConfig := aws.Config{Region: "us-west-2", Credentials: credentials.NewStaticCredentialsProvider("fake-access", "fake-secret", "")}
	client := bedrockruntime.NewFromConfig(awsConfig, func(clientOptions *bedrockruntime.Options) { clientOptions.BaseEndpoint = aws.String(server.URL) })
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	var events []modeltypes.StreamEvent
	err := NewProvider(client).StreamSimple(ctx, modeltypes.Model{ID: "m", Provider: "amazon-bedrock"}, modeltypes.Context{}, modeltypes.SimpleStreamOptions{}, func(streamEvent modeltypes.StreamEvent) { events = append(events, streamEvent) })
	if !errors.Is(err, context.Canceled) {
		t.Fatalf("error = %v, want context.Canceled", err)
	}
	if len(events) != 1 {
		t.Fatalf("events = %#v, want terminal error", events)
	}
	event, ok := events[0].(modeltypes.StreamError)
	if !ok || event.Reason != modeltypes.Aborted {
		t.Fatalf("terminal event = %#v, want aborted StreamError", events[0])
	}
}
