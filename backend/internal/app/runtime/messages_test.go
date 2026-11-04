package runtime_test

import (
	"context"
	"io"
	"os"
	"path/filepath"
	"testing"
	"time"

	"rozsa/backend/internal/app/session"
	"rozsa/backend/internal/model/types"
)

func TestStartMessageReplaysOrderedEventsToLateSubscription(t *testing.T) {
	provider := &promptProvider{started: make(chan struct{}), ended: make(chan struct{})}
	app := newTestRuntime(t, provider, io.Discard)
	operationID, err := app.StartMessage(context.Background(), "hello")
	if err != nil {
		t.Fatal(err)
	}
	select {
	case <-provider.started:
	case <-time.After(time.Second):
		t.Fatal("provider did not start")
	}
	subscription, err := app.SubscribeMessageEvents(context.Background(), operationID)
	if err != nil {
		t.Fatal(err)
	}
	defer subscription.Close()

	first, err := subscription.Next(context.Background())
	if err != nil || first.Kind != MessageTextDelta || first.Sequence != 1 || first.Delta == nil || *first.Delta != "partial" {
		t.Fatalf("first event = %#v, %v", first, err)
	}
	terminal, err := subscription.Next(context.Background())
	if err != nil || terminal.Kind != MessageCompleted || terminal.Sequence != 2 {
		t.Fatalf("terminal event = %#v, %v", terminal, err)
	}
	for _, status := range app.SessionStatus().Sessions {
		if status.Running {
			t.Fatal("message remained marked running after completion")
		}
	}
}

func TestSwitchSessionChangesTheRuntimeMessageOwner(t *testing.T) {
	useProjectTestCWD(t)
	provider := &promptProvider{started: make(chan struct{}), ended: make(chan struct{})}
	workingDirectory, err := os.Getwd()
	if err != nil {
		t.Fatal(err)
	}
	writeDirectory, otherDirectory := filepath.Join(t.TempDir(), "sessions-a"), filepath.Join(t.TempDir(), "sessions-b")
	copyTestFixture(t, filepath.Join(workingDirectory, "sessions-b", "selected.jsonl"), filepath.Join(otherDirectory, "selected.jsonl"))
	app, err := New(Config{
		SessionDir:  writeDirectory,
		SessionDirs: []string{writeDirectory, otherDirectory},
		Model:       types.Model{ID: "custom-model", Provider: "custom-provider", API: types.APIAnthropicMessages, BaseURL: "http://127.0.0.1"},
		Provider:    provider, Output: io.Discard,
	})
	if err != nil {
		t.Fatal(err)
	}
	manager, err := session.OpenManager(filepath.Join(otherDirectory, "selected.jsonl"))
	if err != nil {
		t.Fatal(err)
	}
	if _, err := app.Switch(context.Background(), manager.Path(), "selected"); err != nil {
		t.Fatal(err)
	}
	if err := app.Prompt(context.Background(), "continue selected"); err != nil {
		t.Fatal(err)
	}
	snapshot, err := app.Switch(context.Background(), manager.Path(), "selected")
	if err != nil || len(snapshot.Messages) != 3 {
		t.Fatalf("runtime exposed stale session snapshot: %+v, %v", snapshot.Messages, err)
	}
	if got := app.SessionStatus().ActivePath; got != manager.Path() {
		t.Fatalf("runtime session path = %q, want %q", got, manager.Path())
	}
	if got := len(activeSession(t, app).ContextMessages()); got != 3 {
		t.Fatalf("selected session messages = %d, want existing user and new user/assistant", got)
	}
}

func TestCancelMessageProducesTerminalEventAndPersistsAbortedAssistant(t *testing.T) {
	provider := &promptProvider{started: make(chan struct{}), ended: make(chan struct{}), block: true}
	app := newTestRuntime(t, provider, io.Discard)
	operationID, err := app.StartMessage(context.Background(), "hello")
	if err != nil {
		t.Fatal(err)
	}
	select {
	case <-provider.started:
	case <-time.After(time.Second):
		t.Fatal("provider did not start")
	}
	subscription, err := app.SubscribeMessageEvents(context.Background(), operationID)
	if err != nil {
		t.Fatal(err)
	}
	defer subscription.Close()
	if err := app.CancelMessage(context.Background(), operationID); err != nil {
		t.Fatal(err)
	}
	first, err := subscription.Next(context.Background())
	if err != nil {
		t.Fatalf("first event = %#v, %v", first, err)
	}
	terminal := first
	expectedSequence := uint64(1)
	if first.Kind == MessageTextDelta {
		terminal, err = subscription.Next(context.Background())
		expectedSequence++
	}
	if err != nil || terminal.Kind != MessageCanceled || terminal.Sequence != expectedSequence {
		t.Fatalf("terminal event = %#v, %v", terminal, err)
	}
	select {
	case <-provider.ended:
	case <-time.After(time.Second):
		t.Fatal("provider did not stop after cancellation")
	}
	messages := activeSession(t, app).ContextMessages()
	if len(messages) != 2 {
		t.Fatalf("persisted messages = %d, want user and aborted assistant", len(messages))
	}
	assistant, ok := messages[1].(session.AssistantMessage)
	if !ok || assistant.StopReason != session.Aborted {
		t.Fatalf("persisted assistant = %#v, want aborted", messages[1])
	}
}

func TestContinueMessageReturnsCompletedEventWithoutDuplicatePrompt(t *testing.T) {
	provider := &promptProvider{started: make(chan struct{}), ended: make(chan struct{})}
	app := newTestRuntime(t, provider, io.Discard)
	if err := app.Prompt(context.Background(), "first"); err != nil {
		t.Fatal(err)
	}
	before := len(activeSession(t, app).ContextMessages())
	operationID, err := app.ContinueMessage(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	subscription, err := app.SubscribeMessageEvents(context.Background(), operationID)
	if err != nil {
		t.Fatal(err)
	}
	defer subscription.Close()
	event, err := subscription.Next(context.Background())
	if err != nil || event.Kind != MessageCompleted || event.Sequence != 1 {
		t.Fatalf("continue event = %#v, %v", event, err)
	}
	if after := len(activeSession(t, app).ContextMessages()); after != before {
		t.Fatalf("continue changed persisted messages: before=%d after=%d", before, after)
	}
}
