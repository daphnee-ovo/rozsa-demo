package runtime_test

import (
	"context"
	"encoding/json"
	"errors"
	"runtime"
	"strings"
	"testing"
	"time"

	"rozsa/backend/internal/app/questions"
	"rozsa/backend/internal/model/types"
)

const questionArguments = `{"questions":[{"question":"Pick mode","header":"Mode","options":[{"label":"Fast","description":"Quick"},{"label":"Other","description":"Custom"}]},{"question":"Pick features","header":"Features","multiSelect":true,"options":[{"label":"Lint","description":"Check source"},{"label":"Tests","description":"Run tests"}]}]}`

type questionToolProvider struct {
	arguments string
	calls     chan struct{}
}

func newQuestionToolProvider(arguments string) *questionToolProvider {
	return &questionToolProvider{arguments: arguments, calls: make(chan struct{}, 8)}
}

func (provider *questionToolProvider) StreamSimple(_ context.Context, model types.Model, input types.Context, _ types.SimpleStreamOptions, emit func(types.StreamEvent)) error {
	provider.calls <- struct{}{}
	message := types.AssistantMessage{API: model.API, Provider: model.Provider, Model: model.ID}
	if len(input.Messages) > 0 {
		if _, isResult := input.Messages[len(input.Messages)-1].(types.ToolResultMessage); isResult {
			message.StopReason = types.Stop
			message.Content = []types.ContentBlock{types.TextBlock{Text: "question flow complete"}}
			emit(types.Done{Reason: types.Stop, Message: message})
			return nil
		}
	}
	message.StopReason = types.ToolUse
	message.Content = []types.ContentBlock{types.ToolCallBlock{ID: "question-call", Name: "askUserQuestion", Arguments: json.RawMessage(provider.arguments)}}
	emit(types.Done{Reason: types.ToolUse, Message: message})
	return nil
}

func TestRuntimeRoutesQuestionAnswersIntoPersistedToolResult(t *testing.T) {
	provider := newQuestionToolProvider(questionArguments)
	app := newTestRuntime(t, provider, nil)
	defer app.Shutdown(context.Background())
	operationID, err := app.StartMessage(t.Context(), "ask a question")
	if err != nil {
		t.Fatal(err)
	}
	request, ok := waitForQuestion(app.QuestionOwner())
	if !ok {
		t.Fatal("runtime did not submit a question")
	}
	if request.SessionID != activeSession(t, app).SessionID() || request.TransportID != "question-call" || request.RequestID == request.TransportID {
		t.Fatalf("request identity/session = %#v", request)
	}
	answers := questions.Answers{"Mode": questions.Single("custom answer"), "Features": questions.Multiple("Lint", "Tests")}
	if err := app.QuestionOwner().Respond(request.RequestID, answers); err != nil {
		t.Fatal(err)
	}
	if terminal := waitForTerminal(t, app, operationID); terminal.Kind != MessageCompleted {
		t.Fatalf("question turn = %+v", terminal)
	}
	result := persistedToolResult(t, activeSession(t, app).ContextMessages())
	var details map[string]any
	if err := json.Unmarshal(result.Details, &details); err != nil {
		t.Fatal(err)
	}
	answerMap, ok := details["answers"].(map[string]any)
	if !ok {
		t.Fatalf("persisted answer details = %#v", details)
	}
	if answerMap["Mode"] != "custom answer" {
		t.Fatalf("single/Other answer = %#v", answerMap["Mode"])
	}
	if got := answerMap["Features"].([]any); len(got) != 2 || got[0] != "Lint" || got[1] != "Tests" {
		t.Fatalf("multi-answer = %#v", got)
	}
}

func TestRuntimeRejectsInvalidQuestionsAndCancelledLateAnswers(t *testing.T) {
	invalidProvider := newQuestionToolProvider(`{"questions":[]}`)
	invalidApp := newTestRuntime(t, invalidProvider, nil)
	defer invalidApp.Shutdown(context.Background())
	invalidOperation, err := invalidApp.StartMessage(t.Context(), "invalid question")
	if err != nil {
		t.Fatal(err)
	}
	if terminal := waitForTerminal(t, invalidApp, invalidOperation); terminal.Kind != MessageCompleted {
		t.Fatalf("invalid question turn = %+v", terminal)
	}
	<-invalidProvider.calls
	<-invalidProvider.calls
	if _, ok := invalidApp.QuestionOwner().Next(); ok {
		t.Fatal("invalid arguments left a pending runtime question")
	}
	invalidResult := persistedToolResult(t, activeSession(t, invalidApp).ContextMessages())
	if !invalidResult.IsError || !strings.Contains(persistedText(invalidResult), "1 to 4") {
		t.Fatalf("invalid question result = %+v", invalidResult)
	}

	provider := newQuestionToolProvider(questionArguments)
	app := newTestRuntime(t, provider, nil)
	defer app.Shutdown(context.Background())
	operationID, err := app.StartMessage(t.Context(), "cancelled question")
	if err != nil {
		t.Fatal(err)
	}
	cancelledRequest, ok := waitForQuestion(app.QuestionOwner())
	if !ok {
		t.Fatal("runtime did not submit the cancellable question")
	}
	if err := app.CancelMessage(t.Context(), operationID); err != nil {
		t.Fatal(err)
	}
	if terminal := waitForTerminal(t, app, operationID); terminal.Kind != MessageCanceled {
		t.Fatalf("cancelled question turn = %+v", terminal)
	}
	if err := app.QuestionOwner().Respond(cancelledRequest.RequestID, questions.Answers{"Mode": questions.Single("late"), "Features": questions.Multiple("Lint")}); !errors.Is(err, questions.ErrNoPending) {
		t.Fatalf("late response error = %v", err)
	}
	newOperationID, err := app.StartMessage(t.Context(), "retry question")
	if err != nil {
		t.Fatal(err)
	}
	newRequest, ok := waitForQuestion(app.QuestionOwner())
	if !ok || newRequest.RequestID == cancelledRequest.RequestID || newRequest.TransportID != cancelledRequest.TransportID {
		t.Fatalf("new request after cancellation = %#v", newRequest)
	}
	if err := app.QuestionOwner().Respond(newRequest.RequestID, questions.Answers{"Mode": questions.Single("Fast"), "Features": questions.Multiple("Lint")}); err != nil {
		t.Fatal(err)
	}
	if terminal := waitForTerminal(t, app, newOperationID); terminal.Kind != MessageCompleted {
		t.Fatalf("retried question turn = %+v", terminal)
	}
}

func waitForQuestion(owner *questions.Owner) (questions.Request, bool) {
	deadline := time.Now().Add(time.Second)
	for time.Now().Before(deadline) {
		if request, ok := owner.Next(); ok {
			return request, true
		}
		runtime.Gosched()
	}
	return questions.Request{}, false
}
