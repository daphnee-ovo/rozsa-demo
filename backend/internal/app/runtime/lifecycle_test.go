package runtime

import (
	"context"
	"encoding/json"
	"errors"
	"io"
	"os"
	"path/filepath"
	goruntime "runtime"
	"strings"
	"testing"
	"time"

	"rozsa/backend/internal/app/permissions"
	"rozsa/backend/internal/app/questions"
	"rozsa/backend/internal/app/session"
	"rozsa/backend/internal/app/sessions"
	"rozsa/backend/internal/model/types"
)

// Each actual provider stream has an independent gate. Tests drive the same
// runtime entry points and persistence as production without network timing.
type controlledCall struct {
	id      string
	input   types.Context
	proceed chan string
}

type controlledProvider struct{ calls chan *controlledCall }

func (provider *controlledProvider) StreamSimple(ctx context.Context, model types.Model, input types.Context, options types.SimpleStreamOptions, emit func(types.StreamEvent)) error {
	call := &controlledCall{id: *options.Base.SessionID, input: input, proceed: make(chan string, 8)}
	message := types.AssistantMessage{API: model.API, Provider: model.Provider, Model: model.ID, Content: []types.ContentBlock{}, Timestamp: time.Now().UnixMilli()}
	emit(types.Start{Partial: message})
	emit(types.TextStart{ContentIndex: 0, Partial: message})
	provider.calls <- call
	text := ""
	for {
		select {
		case <-ctx.Done():
			return ctx.Err()
		case delta, ok := <-call.proceed:
			if !ok {
				message.Content = []types.ContentBlock{types.TextBlock{Text: text}}
				message.StopReason = types.Stop
				emit(types.Done{Reason: types.Stop, Message: message})
				return nil
			}
			text += delta
			message.Content = []types.ContentBlock{types.TextBlock{Text: text}}
			emit(types.TextDelta{ContentIndex: 0, Delta: delta, Partial: message})
		}
	}
}

func nextCall(t *testing.T, provider *controlledProvider) *controlledCall {
	t.Helper()
	select {
	case call := <-provider.calls:
		return call
	case <-time.After(3 * time.Second):
		t.Fatal("provider did not start")
		return nil
	}
}

func nextMessageEvent(t *testing.T, subscription MessageEventSubscription) MessageEvent {
	t.Helper()
	ctx, cancel := context.WithTimeout(t.Context(), 3*time.Second)
	defer cancel()
	event, err := subscription.Next(ctx)
	if err != nil {
		t.Fatal(err)
	}
	return event
}

func subscribeMessage(t *testing.T, app *Runtime, operation string) MessageEventSubscription {
	t.Helper()
	subscription, err := app.SubscribeMessageEvents(t.Context(), operation)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(subscription.Close)
	return subscription
}

func createOtherSession(t *testing.T, app *Runtime, id string) *session.Manager {
	t.Helper()
	manager, err := session.Create(filepath.Join(app.sessionDir, id+".jsonl"), id, "/workspace", nil)
	if err != nil {
		t.Fatal(err)
	}
	return manager
}

func TestConcurrentSessionsKeepEventsCancellationAndSelectionIsolated(t *testing.T) {
	provider := &controlledProvider{calls: make(chan *controlledCall, 8)}
	app := newTestRuntime(t, provider, io.Discard)
	t.Cleanup(func() {
		ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
		defer cancel()
		if err := app.Shutdown(ctx); err != nil {
			t.Error(err)
		}
	})
	managerA := app.session
	operationA, err := app.StartMessage(t.Context(), "question A")
	if err != nil {
		t.Fatal(err)
	}
	callA := nextCall(t, provider)
	subA := subscribeMessage(t, app, operationA)
	managerB := createOtherSession(t, app, "session-b")
	if _, err := app.Switch(t.Context(), managerB.Path(), "session-b"); err != nil {
		t.Fatal(err)
	}
	operationB, err := app.StartMessage(t.Context(), "question B")
	if err != nil {
		t.Fatal(err)
	}
	callB := nextCall(t, provider)
	subB := subscribeMessage(t, app, operationB)
	if _, err := app.StartMessage(t.Context(), "duplicate B"); !errors.Is(err, sessions.ErrRunning) {
		t.Fatalf("duplicate accepted: %v", err)
	}
	if callA.id == callB.id {
		t.Fatal("provider session identities crossed")
	}
	callA.proceed <- "A only"
	callB.proceed <- "B only"
	for _, pair := range []struct {
		sub        MessageEventSubscription
		path, text string
	}{{subA, managerA.Path(), "A only"}, {subB, managerB.Path(), "B only"}} {
		event := nextMessageEvent(t, pair.sub)
		if event.SessionPath != pair.path || event.Kind != MessageTextDelta || event.Delta == nil || *event.Delta != pair.text {
			t.Fatalf("crossed delta: %+v", event)
		}
	}
	status := app.SessionStatus()
	if status.ActivePath != managerB.Path() || len(status.Sessions) != 2 || !status.Sessions[0].Running || !status.Sessions[1].Running {
		t.Fatalf("concurrent status: %+v", status)
	}
	// Closing the subscription simulates reload: it must not cancel the turn.
	subB.Close()
	subB = subscribeMessage(t, app, operationB)
	if replay := nextMessageEvent(t, subB); replay.Delta == nil || *replay.Delta != "B only" {
		t.Fatalf("reload replay: %+v", replay)
	}
	if err := app.CancelMessage(t.Context(), operationA); err != nil {
		t.Fatal(err)
	}
	if event := nextMessageEvent(t, subA); event.Kind != MessageCanceled || event.SessionPath != managerA.Path() {
		t.Fatalf("cancel crossed sessions: %+v", event)
	}
	state, err := app.Snapshot(t.Context())
	if err != nil || state.Active == nil || state.Active.Path != managerB.Path() {
		t.Fatalf("background completion changed selection: %+v %v", state, err)
	}
	callB.proceed <- " still running"
	if event := nextMessageEvent(t, subB); event.Delta == nil || *event.Delta != " still running" {
		t.Fatalf("B stopped with A: %+v", event)
	}
	close(callB.proceed)
	if event := nextMessageEvent(t, subB); event.Kind != MessageCompleted || event.SessionPath != managerB.Path() {
		t.Fatalf("B terminal: %+v", event)
	}
	for _, test := range []struct {
		manager *session.Manager
		want    session.StopReason
		text    string
	}{{managerA, session.Aborted, "A only"}, {app.session, session.Stop, "B only still running"}} {
		messages := test.manager.ContextMessages()
		if len(messages) != 2 {
			t.Fatalf("messages for %s: %#v", test.manager.Path(), messages)
		}
		assistant, ok := messages[1].(session.AssistantMessage)
		if !ok || assistant.StopReason != test.want || assistant.Content[0].(session.TextBlock).Text != test.text {
			t.Fatalf("wrong persisted owner: %#v", messages)
		}
	}
	if _, err := app.Switch(t.Context(), managerA.Path(), managerA.SessionID()); err != nil {
		t.Fatalf("re-entry lost the original writable owner: %v", err)
	}
	if app.session != managerA {
		t.Fatal("re-entry replaced the initial session manager")
	}
}

func TestRuntimeConsumesQueueSeriallyAndSteeringBetweenTurns(t *testing.T) {
	provider := &controlledProvider{calls: make(chan *controlledCall, 8)}
	app := newTestRuntime(t, provider, io.Discard)
	path := app.session.Path()
	operation, err := app.StartMessage(t.Context(), "initial")
	if err != nil {
		t.Fatal(err)
	}
	call := nextCall(t, provider)
	sub := subscribeMessage(t, app, operation)
	for _, text := range []string{"remove me", "queued one", "queued two"} {
		if err := app.SendRunningMessage(t.Context(), path, sessions.Queue, text); err != nil {
			t.Fatal(err)
		}
	}
	if removed, err := app.RemoveQueuedMessage(t.Context(), path, 0); err != nil || removed != "remove me" {
		t.Fatalf("remove: %q %v", removed, err)
	}
	if err := app.SendRunningMessage(t.Context(), path, sessions.Steer, "change direction"); err != nil {
		t.Fatal(err)
	}
	call.proceed <- "initial answer"
	close(call.proceed)
	steered := nextCall(t, provider)
	last := steered.input.Messages[len(steered.input.Messages)-1].(types.UserMessage)
	if last.Content.(types.UserText) != "change direction" || last.DisplayText == nil || *last.DisplayText != "[steer] change direction" {
		t.Fatalf("steering context: %#v", steered.input.Messages)
	}
	close(steered.proceed)
	for _, text := range []string{"queued one", "queued two"} {
		queued := nextCall(t, provider)
		retainedSteering := false
		for _, message := range queued.input.Messages {
			if user, ok := message.(types.UserMessage); ok && user.DisplayText != nil && *user.DisplayText == "[steer] change direction" {
				retainedSteering = true
			}
		}
		if !retainedSteering {
			t.Fatal("live context lost delivered steering before queued prompt")
		}
		last := queued.input.Messages[len(queued.input.Messages)-1].(types.UserMessage)
		if string(last.Content.(types.UserText)) != text {
			t.Fatalf("FIFO prompt: %#v want %q", last, text)
		}
		close(queued.proceed)
	}
	for {
		if event := nextMessageEvent(t, sub); event.Kind == MessageCompleted {
			break
		} else if event.Kind != MessageTextDelta {
			t.Fatalf("terminal: %+v", event)
		}
	}
	for _, status := range app.SessionStatus().Sessions {
		if status.Running || len(status.Queued) != 0 || len(status.Steering) != 0 {
			t.Fatalf("not drained: %+v", status)
		}
	}
	reader, err := session.Open(path)
	if err != nil {
		t.Fatal(err)
	}
	var users []string
	for _, message := range reader.ContextMessages() {
		if user, ok := message.(session.UserMessage); ok {
			switch content := user.Content.(type) {
			case session.UserText:
				users = append(users, string(content))
			case session.UserBlocks:
				text := ""
				for _, block := range content {
					if block, ok := block.(session.TextBlock); ok {
						text += block.Text
					}
				}
				users = append(users, text)
			}
		}
	}
	if len(users) != 3 || users[0] != "initial" || users[1] != "queued one" || users[2] != "queued two" {
		t.Fatalf("persisted prompts: %v", users)
	}
	fork, err := app.ForkSession(t.Context(), 2)
	if err != nil || len(fork.Messages) != 3 || fork.Messages[2].DisplayText == nil || *fork.Messages[2].DisplayText != "[steer] change direction" {
		t.Fatalf("fork lost live steering: %+v %v", fork, err)
	}
}

func TestForkCopiesOnlyPrefixThroughUserAndRestoresContext(t *testing.T) {
	provider := &controlledProvider{calls: make(chan *controlledCall, 8)}
	app := newTestRuntime(t, provider, io.Discard)
	source := app.session
	for _, message := range []session.Message{session.UserMessage{Content: session.UserText("first")}, session.AssistantMessage{Content: []session.ContentBlock{session.TextBlock{Text: "answer"}}, StopReason: session.Stop}, session.UserMessage{Content: session.UserText("fork here")}, session.AssistantMessage{Content: []session.ContentBlock{session.TextBlock{Text: "later"}}, StopReason: session.Stop}} {
		if _, err := source.AppendMessage(message); err != nil {
			t.Fatal(err)
		}
	}
	if _, err := app.ForkSession(t.Context(), 1); err == nil {
		t.Fatal("accepted assistant fork point")
	}
	snapshot, err := app.ForkSession(t.Context(), 2)
	if err != nil {
		t.Fatal(err)
	}
	reader, err := session.Open(snapshot.Path)
	if err != nil {
		t.Fatal(err)
	}
	header := reader.Header()
	if header.ParentSession == nil || *header.ParentSession != source.Path() || len(reader.ContextMessages()) != 3 {
		t.Fatalf("fork restoration: %+v %#v", header, reader.ContextMessages())
	}
	if source.Entries()[0].ID == reader.Entries()[0].ID {
		t.Fatal("fork reused entry identity")
	}
	operation, err := app.ContinueMessage(t.Context())
	if err != nil {
		t.Fatal(err)
	}
	call := nextCall(t, provider)
	if len(call.input.Messages) != 3 || call.id != header.ID {
		t.Fatalf("fork provider context: %+v", call.input)
	}
	close(call.proceed)
	if event := nextMessageEvent(t, subscribeMessage(t, app, operation)); event.Kind != MessageCompleted {
		t.Fatalf("fork continuation: %+v", event)
	}
}

type questionProvider struct{ calls chan string }

func (provider *questionProvider) StreamSimple(ctx context.Context, model types.Model, input types.Context, options types.SimpleStreamOptions, emit func(types.StreamEvent)) error {
	message := types.AssistantMessage{API: model.API, Provider: model.Provider, Model: model.ID, Content: []types.ContentBlock{}, StopReason: types.Stop}
	if _, ok := input.Messages[len(input.Messages)-1].(types.ToolResultMessage); !ok {
		message.Content = []types.ContentBlock{types.ToolCallBlock{ID: "same-call", Name: askUserQuestionName, Arguments: json.RawMessage(questionArguments)}}
		message.StopReason = types.ToolUse
	}
	emit(types.Start{Partial: message})
	emit(types.Done{Reason: message.StopReason, Message: message})
	provider.calls <- *options.Base.SessionID
	return nil
}

func TestRuntimeQuestionAndApprovalCleanupUsesOriginSession(t *testing.T) {
	provider := &questionProvider{calls: make(chan string, 8)}
	app := newTestRuntime(t, provider, io.Discard)
	managerA := app.session
	operationA, err := app.StartMessage(t.Context(), "A asks")
	if err != nil {
		t.Fatal(err)
	}
	requestA, ok := waitForQuestion(app.QuestionOwner())
	if !ok {
		t.Fatal("A production tool did not submit")
	}
	managerB, err := session.Create(filepath.Join(app.sessionDir, "b.jsonl"), "header-b", "/workspace", nil)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := app.Switch(t.Context(), managerB.Path(), "header-b"); err != nil {
		t.Fatal(err)
	}
	operationB, err := app.StartMessage(t.Context(), "B asks")
	if err != nil {
		t.Fatal(err)
	}
	for range 2 {
		select {
		case <-provider.calls:
		case <-time.After(3 * time.Second):
			t.Fatal("both providers did not run")
		}
	}
	for _, id := range []string{managerA.SessionID(), "b"} {
		if err := app.approvals.Add(permissions.Identity{SessionID: id, RequestID: "same-call"}, permissions.ApprovalInfo{}); err != nil {
			t.Fatal(err)
		}
	}
	if err := app.CancelMessage(t.Context(), operationA); err != nil {
		t.Fatal(err)
	}
	subA := subscribeMessage(t, app, operationA)
	if event := nextMessageEvent(t, subA); event.Kind != MessageCanceled {
		t.Fatalf("A terminal: %+v", event)
	}
	foundToolCall := false
	for _, message := range managerA.ContextMessages() {
		if assistant, ok := message.(session.AssistantMessage); ok {
			foundToolCall = true
			if assistant.StopReason != session.ToolUse {
				t.Fatalf("cancellation rewrote completed tool-call assistant: %+v", assistant)
			}
			break
		}
	}
	if !foundToolCall {
		t.Fatal("cancellation lost original tool-call assistant")
	}
	requestB, ok := waitForQuestion(app.QuestionOwner())
	if !ok || requestB.SessionID != "b" || requestB.TransportID != requestA.TransportID || requestB.RequestID == requestA.RequestID {
		t.Fatalf("question route: A=%+v B=%+v", requestA, requestB)
	}
	approvalA, _ := app.approvals.Get(permissions.Identity{SessionID: managerA.SessionID(), RequestID: "same-call"})
	approvalB, _ := app.approvals.Get(permissions.Identity{SessionID: "b", RequestID: "same-call"})
	if approvalA.Status != permissions.Cancelled || approvalB.Status != permissions.Pending {
		t.Fatalf("approval cleanup crossed sessions: A=%+v B=%+v", approvalA, approvalB)
	}
	answers := questions.Answers{"Mode": questions.Single("Fast"), "Features": questions.Multiple("Lint")}
	if err := app.QuestionOwner().Respond(requestA.RequestID, answers); !errors.Is(err, questions.ErrNoPending) {
		t.Fatalf("accepted late A answer: %v", err)
	}
	if err := app.QuestionOwner().Respond(requestB.RequestID, answers); err != nil {
		t.Fatal(err)
	}
	if event := nextMessageEvent(t, subscribeMessage(t, app, operationB)); event.Kind != MessageCompleted || event.SessionID != "b" || event.SessionPath != managerB.Path() {
		t.Fatalf("B terminal: %+v", event)
	}
}

func TestRuntimeShutdownRejectsNewWorkWithoutChangingSelection(t *testing.T) {
	provider := &controlledProvider{calls: make(chan *controlledCall, 8)}
	app := newTestRuntime(t, provider, io.Discard)
	manager := createOtherSession(t, app, "selected")
	if _, err := app.Switch(t.Context(), manager.Path(), "selected"); err != nil {
		t.Fatal(err)
	}
	before, err := app.Snapshot(t.Context())
	if err != nil {
		t.Fatal(err)
	}
	if err := app.Shutdown(t.Context()); err != nil {
		t.Fatal(err)
	}
	if _, err := app.Switch(t.Context(), manager.Path(), "selected"); !errors.Is(err, sessions.ErrClosed) {
		t.Fatalf("switch after shutdown: %v", err)
	}
	if _, err := app.StartMessage(t.Context(), "late"); !errors.Is(err, sessions.ErrClosed) {
		t.Fatalf("start after shutdown: %v", err)
	}
	if _, err := app.ForkSession(t.Context(), 0); !errors.Is(err, sessions.ErrClosed) {
		t.Fatalf("fork after shutdown: %v", err)
	}
	after, err := app.Snapshot(t.Context())
	if err != nil || before.Watermark != after.Watermark || after.Active == nil || after.Active.Path != manager.Path() {
		t.Fatalf("shutdown admission failure changed selection: %+v %v", after, err)
	}
}

func TestRuntimeReportsWriteFailureWithoutContaminatingOtherSession(t *testing.T) {
	provider := &controlledProvider{calls: make(chan *controlledCall, 8)}
	app := newTestRuntime(t, provider, io.Discard)
	operationA, err := app.StartMessage(t.Context(), "A")
	if err != nil {
		t.Fatal(err)
	}
	callA := nextCall(t, provider)
	managerB := createOtherSession(t, app, "b")
	if _, err := app.Switch(t.Context(), managerB.Path(), "b"); err != nil {
		t.Fatal(err)
	}
	operationB, err := app.StartMessage(t.Context(), "B")
	if err != nil {
		t.Fatal(err)
	}
	callB := nextCall(t, provider)
	// Replace only this synthetic session's file with a directory after its
	// user message has been committed, forcing the assistant append to fail.
	if err := os.Rename(managerB.Path(), managerB.Path()+".saved"); err != nil {
		t.Fatal(err)
	}
	if err := os.Mkdir(managerB.Path(), 0700); err != nil {
		t.Fatal(err)
	}
	close(callB.proceed)
	event := nextMessageEvent(t, subscribeMessage(t, app, operationB))
	if event.Kind != MessageFailed || event.Error == nil || !strings.Contains(*event.Error, "persist agent message") {
		t.Fatalf("write failure hidden: %+v", event)
	}
	close(callA.proceed)
	if event := nextMessageEvent(t, subscribeMessage(t, app, operationA)); event.Kind != MessageCompleted {
		t.Fatalf("write failure crossed sessions: %+v", event)
	}
}

func TestCancellationDoesNotHideCleanupFailure(t *testing.T) {
	provider := &controlledProvider{calls: make(chan *controlledCall, 8)}
	app, err := New(Config{SessionDir: t.TempDir(), Model: types.Model{ID: "model", Provider: "provider", API: types.APIAnthropicMessages}, Provider: provider, LifecycleHooks: sessions.Hooks{RejectApprovals: func(string) error { return errors.New("response rejected") }}})
	if err != nil {
		t.Fatal(err)
	}
	operation, err := app.StartMessage(t.Context(), "question")
	if err != nil {
		t.Fatal(err)
	}
	nextCall(t, provider)
	if err := app.CancelMessage(t.Context(), operation); err == nil || !strings.Contains(err.Error(), "response rejected") {
		t.Fatalf("cancel hid responder failure: %v", err)
	}
	event := nextMessageEvent(t, subscribeMessage(t, app, operation))
	if event.Kind != MessageFailed || event.Error == nil || !strings.Contains(*event.Error, "response rejected") {
		t.Fatalf("terminal hid responder failure: %+v", event)
	}
}

func TestCancelBeforeProviderStartsDoesNotBecomeEmptyResponseFailure(t *testing.T) {
	provider := &controlledProvider{calls: make(chan *controlledCall, 8)}
	app := newTestRuntime(t, provider, io.Discard)
	operation, err := app.StartMessage(t.Context(), "cancel immediately")
	if err != nil {
		t.Fatal(err)
	}
	if err := app.CancelMessage(t.Context(), operation); err != nil {
		t.Fatal(err)
	}
	subscription := subscribeMessage(t, app, operation)
	for {
		event := nextMessageEvent(t, subscription)
		if event.Kind == MessageTextDelta {
			continue
		}
		if event.Kind != MessageCanceled {
			t.Fatalf("early cancel terminal: %+v", event)
		}
		break
	}
}

func TestForkFromDeliveredSteeringWhileSourceContinuesInBackground(t *testing.T) {
	provider := &controlledProvider{calls: make(chan *controlledCall, 8)}
	app := newTestRuntime(t, provider, io.Discard)
	source := app.session
	operation, err := app.StartMessage(t.Context(), "initial")
	if err != nil {
		t.Fatal(err)
	}
	call := nextCall(t, provider)
	if err := app.SendRunningMessage(t.Context(), source.Path(), sessions.Steer, "live fork point"); err != nil {
		t.Fatal(err)
	}
	close(call.proceed)
	steered := nextCall(t, provider)
	deadline := time.Now().Add(time.Second)
	for len(app.conversation(source).displayMessages()) < 3 && time.Now().Before(deadline) {
		goruntime.Gosched()
	}
	fork, err := app.ForkSession(t.Context(), 2)
	if err != nil {
		t.Fatal(err)
	}
	if len(fork.Messages) != 3 || fork.Messages[2].DisplayText == nil || *fork.Messages[2].DisplayText != "[steer] live fork point" {
		t.Fatalf("fork prefix: %+v", fork.Messages)
	}
	close(steered.proceed)
	if event := nextMessageEvent(t, subscribeMessage(t, app, operation)); event.Kind != MessageCompleted || event.SessionPath != source.Path() {
		t.Fatalf("background source terminal: %+v", event)
	}
	state, err := app.Snapshot(t.Context())
	if err != nil || state.Active == nil || state.Active.Path != fork.Path {
		t.Fatalf("background source stole fork selection: %+v %v", state, err)
	}
	reader, err := session.Open(source.Path())
	if err != nil {
		t.Fatal(err)
	}
	for _, message := range reader.ContextMessages() {
		if user, ok := message.(session.UserMessage); ok && user.DisplayText != nil {
			t.Fatal("delivered steering was silently made durable in source")
		}
	}
}

type drainingProvider struct {
	started  chan struct{}
	release  chan struct{}
	returned chan struct{}
}

func (provider drainingProvider) StreamSimple(ctx context.Context, model types.Model, _ types.Context, _ types.SimpleStreamOptions, emit func(types.StreamEvent)) error {
	defer close(provider.returned)
	emit(types.Start{Partial: types.AssistantMessage{API: model.API, Provider: model.Provider, Model: model.ID}})
	close(provider.started)
	<-ctx.Done()
	<-provider.release
	return ctx.Err()
}

func TestShutdownReportsTimeoutUntilProviderCleanupReturns(t *testing.T) {
	provider := drainingProvider{started: make(chan struct{}), release: make(chan struct{}), returned: make(chan struct{})}
	app := newTestRuntime(t, provider, io.Discard)
	if _, err := app.StartMessage(t.Context(), "wait for cleanup"); err != nil {
		t.Fatal(err)
	}
	select {
	case <-provider.started:
	case <-time.After(time.Second):
		t.Fatal("provider did not start")
	}
	ctx, cancel := context.WithTimeout(t.Context(), 50*time.Millisecond)
	err := app.Shutdown(ctx)
	cancel()
	close(provider.release)
	if !errors.Is(err, context.DeadlineExceeded) {
		t.Errorf("reported shutdown success before provider returned: %v", err)
	}
	select {
	case <-provider.returned:
	case <-time.After(time.Second):
		t.Fatal("provider did not return after cleanup release")
	}
	ctx, cancel = context.WithTimeout(t.Context(), time.Second)
	defer cancel()
	if err := app.Shutdown(ctx); err != nil {
		t.Fatal(err)
	}
}
