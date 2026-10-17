package runtime

import (
	"context"
	"encoding/json"
	"errors"
	"runtime"
	"strings"
	"testing"
	"time"

	"rozsa/backend/internal/app/questions"
	"rozsa/backend/internal/core/tools"
)

const questionArguments = `{"questions":[{"question":"Pick mode","header":"Mode","options":[{"label":"Fast","description":"Quick"},{"label":"Other","description":"Custom"}]},{"question":"Pick features","header":"Features","multiSelect":true,"options":[{"label":"Lint","description":"Check source"},{"label":"Tests","description":"Run tests"}]}]}`

func productionQuestionTool(sessionID string, owner *questions.Owner) tools.Tool {
	runtime := &Runtime{questions: owner}
	for _, tool := range runtime.applicationTools(sessionID) {
		if tool.Name() == askUserQuestionName {
			return tool
		}
	}
	return nil
}

func TestProductionToolExecutesThroughRuntimeQuestionOwner(t *testing.T) {
	owner := questions.NewOwner()
	tool := productionQuestionTool("session-a", owner)
	if tool == nil {
		t.Fatal("runtime did not register AskUserQuestion")
	}
	if mode := tool.ExecutionMode(); mode == nil || *mode != tools.Sequential {
		t.Fatalf("execution mode = %v", mode)
	}
	type execution struct {
		result tools.Result
		err    error
	}
	finished := make(chan execution, 1)
	go func() {
		result, err := tool.Execute(context.Background(), "transport-call-id", json.RawMessage(questionArguments), nil)
		finished <- execution{result: result, err: err}
	}()
	request, ok := waitForQuestion(owner)
	if !ok {
		t.Fatal("production tool did not submit a question")
	}
	if request.SessionID != "session-a" || request.TransportID != "transport-call-id" || request.RequestID == request.TransportID {
		t.Fatalf("request identity/session = %#v", request)
	}
	answers := questions.Answers{"Mode": questions.Single("custom answer"), "Features": questions.Multiple("Lint", "Tests")}
	if err := owner.Respond(request.RequestID, answers); err != nil {
		t.Fatal(err)
	}
	completed := <-finished
	if completed.err != nil {
		t.Fatal(completed.err)
	}
	var details map[string]any
	if err := json.Unmarshal(completed.result.Details, &details); err != nil {
		t.Fatal(err)
	}
	answerMap := details["answers"].(map[string]any)
	if answerMap["Mode"] != "custom answer" {
		t.Fatalf("single/Other answer = %#v", answerMap["Mode"])
	}
	if got := answerMap["Features"].([]any); len(got) != 2 || got[0] != "Lint" || got[1] != "Tests" {
		t.Fatalf("multi-answer = %#v", got)
	}
}

func TestProductionToolValidationAndCancellationRejectLateResponse(t *testing.T) {
	owner := questions.NewOwner()
	tool := productionQuestionTool("session-a", owner)
	if _, err := tool.Execute(context.Background(), "bad-call", json.RawMessage(`{"questions":[]}`), nil); err == nil || !strings.Contains(err.Error(), "1 to 4") {
		t.Fatalf("invalid parameters error = %v", err)
	}
	if _, ok := owner.Next(); ok {
		t.Fatal("invalid tool arguments left a pending request")
	}

	ctx, cancel := context.WithCancel(context.Background())
	finished := make(chan error, 1)
	go func() {
		_, err := tool.Execute(ctx, "same-transport-id", json.RawMessage(questionArguments), nil)
		finished <- err
	}()
	cancelledRequest, ok := waitForQuestion(owner)
	if !ok {
		t.Fatal("cancelled production tool did not submit request")
	}
	cancel()
	if err := <-finished; !errors.Is(err, context.Canceled) {
		t.Fatalf("tool cancellation error = %v", err)
	}
	if err := owner.Respond(cancelledRequest.RequestID, questions.Answers{"Mode": questions.Single("late"), "Features": questions.Multiple("Lint")}); !errors.Is(err, questions.ErrNoPending) {
		t.Fatalf("late response error = %v", err)
	}

	newRequest, future, err := owner.Submit("session-a", "same-transport-id", mustParseQuestionParams(t))
	if err != nil {
		t.Fatal(err)
	}
	if newRequest.RequestID == cancelledRequest.RequestID {
		t.Fatal("cancelled product identity was reused")
	}
	shown, ok := owner.Next()
	if !ok || shown.RequestID != newRequest.RequestID {
		t.Fatalf("new request after late answer = %#v, %v", shown, ok)
	}
	if err := owner.Respond(newRequest.RequestID, questions.Answers{"Mode": questions.Single("Fast"), "Features": questions.Multiple("Lint")}); err != nil {
		t.Fatal(err)
	}
	if _, err := future.Wait(context.Background()); err != nil {
		t.Fatal(err)
	}
}

func mustParseQuestionParams(t *testing.T) questions.Params {
	t.Helper()
	var params questions.Params
	if err := json.Unmarshal([]byte(questionArguments), &params); err != nil {
		t.Fatal(err)
	}
	return params
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
