package questions

import (
	"context"
	"errors"
	"testing"
)

func sampleParams() Params {
	return Params{Questions: []Question{
		{Question: "Choose a mode", Header: "Mode", Options: []Option{{Label: "Fast", Description: "Quick"}, {Label: "Other", Description: "Custom"}}},
		{Question: "Choose features", Header: "Features", MultiSelect: true, Options: []Option{{Label: "Lint", Description: "Check source"}, {Label: "Tests", Description: "Run tests"}}},
	}}
}

func TestValidateParamsAndAnswersPreserveMultiQuestionOtherContract(t *testing.T) {
	params := sampleParams()
	if err := ValidateParams(params); err != nil {
		t.Fatal(err)
	}
	answers := Answers{"Mode": Single("my custom answer"), "Features": Multiple("Lint", "Tests")}
	if err := ValidateAnswers(params.Questions, answers); err != nil {
		t.Fatalf("Other and multi-select answers should be accepted: %v", err)
	}

	invalid := []Answers{
		{"Mode": Single("Fast")},
		{"Mode": Multiple("Fast"), "Features": Multiple("Lint")},
		{"Mode": Single("Fast"), "Features": Single("Lint")},
		{"Mode": Single("Fast"), "Features": Multiple()},
		{"Mode": Single("Fast"), "Features": Multiple("Lint", "Lint")},
	}
	for index, answer := range invalid {
		if err := ValidateAnswers(params.Questions, answer); err == nil {
			t.Errorf("invalid answer case %d unexpectedly passed", index)
		}
	}
	params.Questions[1].Header = "Mode"
	if err := ValidateParams(params); err == nil {
		t.Fatal("duplicate headers must fail")
	}
}

func TestOwnerSeparatesTransportAndProductIDsAndQueuesAcrossSessionSwitch(t *testing.T) {
	owner := NewOwner()
	first, firstFuture, err := owner.Submit("session-a", "wire-1", sampleParams())
	if err != nil {
		t.Fatal(err)
	}
	second, secondFuture, err := owner.Submit("session-b", "wire-1", sampleParams())
	if err != nil {
		t.Fatal(err)
	}
	if first.RequestID == second.RequestID || first.RequestID == first.TransportID {
		t.Fatal("product request IDs must be independent from transport IDs")
	}
	shown, ok := owner.Next()
	if !ok || shown.RequestID != first.RequestID {
		t.Fatalf("first queued request = %#v, %v", shown, ok)
	}
	shown, ok = owner.Next()
	if !ok || shown.SessionID != "session-a" {
		t.Fatalf("active request was not retained during session switch: %#v, %v", shown, ok)
	}
	if err := owner.Respond(first.RequestID, Answers{"Mode": Single("Fast"), "Features": Multiple("Lint")}); err != nil {
		t.Fatal(err)
	}
	if _, err := firstFuture.Wait(context.Background()); err != nil {
		t.Fatalf("first answer: %v", err)
	}
	shown, ok = owner.Next()
	if !ok || shown.RequestID != second.RequestID {
		t.Fatalf("queued request was not restored next: %#v, %v", shown, ok)
	}
	if err := owner.Respond(second.RequestID, Answers{"Mode": Single("Other"), "Features": Multiple("Tests")}); err != nil {
		t.Fatal(err)
	}
	if _, err := secondFuture.Wait(context.Background()); err != nil {
		t.Fatalf("second answer: %v", err)
	}
}

func TestCancelSessionAndLateResponseCannotTouchNewRequest(t *testing.T) {
	owner := NewOwner()
	oldRequest, oldFuture, err := owner.Submit("session-a", "same-wire-id", sampleParams())
	if err != nil {
		t.Fatal(err)
	}
	if count := owner.CancelSession("session-a"); count != 1 {
		t.Fatalf("cancel count = %d", count)
	}
	if _, err := oldFuture.Wait(context.Background()); !errors.Is(err, ErrCancelled) {
		t.Fatalf("old future error = %v", err)
	}
	newRequest, newFuture, err := owner.Submit("session-a", "same-wire-id", sampleParams())
	if err != nil {
		t.Fatal(err)
	}
	if oldRequest.RequestID == newRequest.RequestID {
		t.Fatal("new product request reused cancelled identity")
	}
	if err := owner.Respond(oldRequest.RequestID, Answers{"Mode": Single("late"), "Features": Multiple("Lint")}); !errors.Is(err, ErrNoPending) {
		t.Fatalf("late response error = %v", err)
	}
	if _, ok := owner.Next(); !ok {
		t.Fatal("new request disappeared after late response")
	}
	if err := owner.Respond(newRequest.RequestID, Answers{"Mode": Single("Fast"), "Features": Multiple("Lint")}); err != nil {
		t.Fatal(err)
	}
	if _, err := newFuture.Wait(context.Background()); err != nil {
		t.Fatalf("new future error = %v", err)
	}
}

func TestCancelQueuedRequestDoesNotFabricateAnswerOrBlockFollowingRequest(t *testing.T) {
	owner := NewOwner()
	_, activeFuture, err := owner.Submit("session-a", "call-a", sampleParams())
	if err != nil {
		t.Fatal(err)
	}
	activeRequest, _ := owner.Next()
	queuedRequest, queuedFuture, err := owner.Submit("session-b", "call-b", sampleParams())
	if err != nil {
		t.Fatal(err)
	}
	if !owner.Cancel(queuedRequest.RequestID) {
		t.Fatal("queued cancellation failed")
	}
	if _, err := queuedFuture.Wait(context.Background()); !errors.Is(err, ErrCancelled) {
		t.Fatalf("queued future error = %v", err)
	}
	if err := owner.Respond(queuedRequest.RequestID, Answers{"Mode": Single("Fast"), "Features": Multiple("Lint")}); !errors.Is(err, ErrNoPending) {
		t.Fatalf("cancelled request accepted late response: %v", err)
	}
	if err := owner.Respond(activeRequest.RequestID, Answers{"Mode": Single("Fast"), "Features": Multiple("Lint")}); err != nil {
		t.Fatal(err)
	}
	if _, err := activeFuture.Wait(context.Background()); err != nil {
		t.Fatal(err)
	}
	if _, ok := owner.Next(); ok {
		t.Fatal("cancelled queued request was shown")
	}
}

func TestFutureContextCancellationIsObservable(t *testing.T) {
	owner := NewOwner()
	_, future, err := owner.Submit("session-a", "call", sampleParams())
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if _, err := future.Wait(ctx); !errors.Is(err, context.Canceled) {
		t.Fatalf("future cancellation = %v", err)
	}
}

func TestInvalidResponseDoesNotConsumeRequestAndFailureRemainsExplicit(t *testing.T) {
	owner := NewOwner()
	request, future, err := owner.Submit("session-a", "call", sampleParams())
	if err != nil {
		t.Fatal(err)
	}
	if err := owner.Respond(request.RequestID, Answers{"Mode": Single("Fast")}); err == nil {
		t.Fatal("incomplete response unexpectedly passed")
	}
	if _, ok := owner.Next(); !ok {
		t.Fatal("invalid response consumed the pending request")
	}
	wantFailure := errors.New("question event delivery failed")
	if err := owner.Fail(request.RequestID, wantFailure); err != nil {
		t.Fatal(err)
	}
	if _, err := future.Wait(context.Background()); !errors.Is(err, wantFailure) {
		t.Fatalf("failure result = %v", err)
	}
}
