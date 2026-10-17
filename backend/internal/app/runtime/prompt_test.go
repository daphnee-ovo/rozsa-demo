package runtime

import (
	"context"
	"errors"
	"io"
	"strings"
	"testing"
	"time"

	"rozsa/backend/internal/app/session"
	"rozsa/backend/internal/core/agent"
	"rozsa/backend/internal/model/types"
)

type promptProvider struct {
	started chan struct{}
	ended   chan struct{}
	block   bool
}

func (provider *promptProvider) StreamSimple(ctx context.Context, model types.Model, _ types.Context, _ types.SimpleStreamOptions, emit func(types.StreamEvent)) error {
	defer close(provider.ended)
	message := types.AssistantMessage{API: model.API, Provider: model.Provider, Model: model.ID, Timestamp: time.Now().UnixMilli(), Content: []types.ContentBlock{}}
	emit(types.Start{Partial: message})
	emit(types.TextStart{ContentIndex: 0, Partial: message})
	partial := message
	partial.Content = []types.ContentBlock{types.TextBlock{Text: "partial"}}
	emit(types.TextDelta{ContentIndex: 0, Delta: "partial", Partial: partial})
	close(provider.started)
	if provider.block {
		<-ctx.Done()
		return ctx.Err()
	}
	final := partial
	final.StopReason = types.Stop
	emit(types.Done{Reason: types.Stop, Message: final})
	return nil
}

type rejectingWriter struct{ err error }

func (writer rejectingWriter) Write([]byte) (int, error) { return 0, writer.err }

func newTestRuntime(t *testing.T, provider agent.Provider, output io.Writer) *Runtime {
	t.Helper()
	app, err := New(Config{SessionDir: t.TempDir(), Model: types.Model{ID: "custom-model", Provider: "custom-anthropic", API: types.APIAnthropicMessages, BaseURL: "http://127.0.0.1"}, Provider: provider, Output: output})
	if err != nil {
		t.Fatal(err)
	}
	return app
}

func TestPromptDrainsProviderAndPersistsAfterOutputWriteFailure(t *testing.T) {
	writeError := errors.New("stdout closed")
	provider := &promptProvider{started: make(chan struct{}), ended: make(chan struct{})}
	app := newTestRuntime(t, provider, rejectingWriter{err: writeError})
	err := app.Prompt(context.Background(), "hello")
	if !errors.Is(err, writeError) {
		t.Fatalf("Prompt error = %v, want writer error", err)
	}
	select {
	case <-provider.ended:
	case <-time.After(time.Second):
		t.Fatal("provider goroutine did not finish")
	}
	messages := app.session.ContextMessages()
	if len(messages) != 2 {
		t.Fatalf("persisted messages = %d, want user and assistant: %#v", len(messages), messages)
	}
	if !strings.Contains(app.session.Path(), ".jsonl") {
		t.Fatalf("session path = %q", app.session.Path())
	}
}

func TestPromptPersistsResponsesAPIAssistantMessage(t *testing.T) {
	provider := &promptProvider{started: make(chan struct{}), ended: make(chan struct{})}
	app, err := New(Config{
		SessionDir: t.TempDir(),
		Model:      types.Model{ID: "custom-model", Provider: "custom-responses", API: types.APIOpenAIResponses},
		Provider:   provider,
		Output:     io.Discard,
	})
	if err != nil {
		t.Fatal(err)
	}
	if err := app.Prompt(context.Background(), "hello Responses"); err != nil {
		t.Fatal(err)
	}
	messages := app.session.ContextMessages()
	if len(messages) != 2 {
		t.Fatalf("persisted messages = %d, want user and assistant: %#v", len(messages), messages)
	}
	assistant, ok := messages[1].(session.AssistantMessage)
	if !ok || assistant.API != string(types.APIOpenAIResponses) || assistant.Provider != "custom-responses" {
		t.Fatalf("assistant = %#v, want Responses API with configured provider identity", messages[1])
	}
}

func TestPromptCancellationWaitsForProviderAndPersistsAbortedAssistant(t *testing.T) {
	provider := &promptProvider{started: make(chan struct{}), ended: make(chan struct{}), block: true}
	app := newTestRuntime(t, provider, io.Discard)
	ctx, cancel := context.WithCancel(context.Background())
	result := make(chan error, 1)
	go func() { result <- app.Prompt(ctx, "hello") }()
	select {
	case <-provider.started:
	case <-time.After(time.Second):
		t.Fatal("provider did not start")
	}
	cancel()
	select {
	case err := <-result:
		if !errors.Is(err, context.Canceled) {
			t.Fatalf("Prompt error = %v, want context cancellation", err)
		}
	case <-time.After(time.Second):
		t.Fatal("Prompt did not return after cancellation")
	}
	select {
	case <-provider.ended:
	case <-time.After(time.Second):
		t.Fatal("provider goroutine leaked after cancellation")
	}
	messages := app.session.ContextMessages()
	if len(messages) != 2 {
		t.Fatalf("persisted messages = %d, want user and aborted assistant", len(messages))
	}
	assistant, ok := messages[1].(session.AssistantMessage)
	if !ok || assistant.StopReason != session.Aborted {
		t.Fatalf("last message = %#v, want aborted assistant", messages[1])
	}
}
