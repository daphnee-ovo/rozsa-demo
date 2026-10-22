package server

import (
	"bytes"
	"context"
	"encoding/json"
	"net"
	"testing"
	"time"

	"go.lsp.dev/jsonrpc2"
	"rozsa/backend/internal/app/permissions"
	"rozsa/backend/internal/app/questions"
	"rozsa/backend/internal/app/session"
)

// rawFrames exposes the library stream's native frame methods for malformed
// transport inputs; it does not implement framing or protocol parsing.
type rawFrames interface {
	ReadFrame(context.Context) ([]byte, int64, error)
	WriteFrame(context.Context, []byte) (int64, error)
}

type interruptionTestApplication struct {
	MessageApplication
	requests  []permissions.Request
	question  questions.Request
	responded permissions.Response
	answer    questions.Answers
	cancelled string
	failReply error
}

func (application *interruptionTestApplication) PendingApprovals(_ context.Context, sessionID string) ([]permissions.Request, error) {
	if application.failReply != nil {
		return nil, application.failReply
	}
	requests := make([]permissions.Request, 0, len(application.requests))
	for _, request := range application.requests {
		if request.Identity.SessionID == sessionID {
			requests = append(requests, request)
		}
	}
	return requests, nil
}

func (application *interruptionTestApplication) NextQuestion(_ context.Context, sessionID string) (questions.Request, bool, error) {
	if application.failReply != nil {
		return questions.Request{}, false, application.failReply
	}
	if application.question.SessionID != sessionID || application.question.RequestID == "" {
		return questions.Request{}, false, nil
	}
	return application.question, true, nil
}

func (application *interruptionTestApplication) RespondApproval(_ context.Context, _ permissions.Identity, response permissions.Response) error {
	application.responded = response
	return application.failReply
}

func (application *interruptionTestApplication) RespondQuestion(_ context.Context, _ string, answer questions.Answers) error {
	application.answer = answer
	return application.failReply
}

func (application *interruptionTestApplication) CancelQuestion(_ context.Context, requestID string) error {
	application.cancelled = requestID
	return application.failReply
}

func TestInterruptionRequestsExposeTypedStateAndResponses(t *testing.T) {
	application := &interruptionTestApplication{
		requests: []permissions.Request{{
			Identity: permissions.Identity{SessionID: "session-a", RequestID: "approval-a"},
			Info: permissions.ApprovalInfo{
				ToolName: "Bash", ArgsSummary: "make test", Risk: permissions.RiskShell,
				TrustLevels: []permissions.TrustLevel{{Key: "Bash:make", Label: "make"}},
				TrustGroups: []permissions.TrustGroup{{Target: "make test", Levels: []permissions.TrustLevel{{Key: "Bash:make", Label: "make"}}}},
			}, Status: permissions.Failed,
		}},
		question: questions.Request{SessionID: "session-a", RequestID: "question-a", TransportID: "tool-call-a", Questions: []questions.Question{
			{Header: "Mode", Question: "Select a mode", Options: []questions.Option{{Label: "Fast", Description: "Quick"}}},
			{Header: "Features", Question: "Select features", MultiSelect: true, Options: []questions.Option{{Label: "Tests", Description: "Run tests"}}},
		}},
	}
	responses, err := exchangeProduct(t, application,
		`{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":1}}`,
		`{"jsonrpc":"2.0","id":2,"method":"get_interruption_state","params":{"sessionId":"session-a"}}`,
		`{"jsonrpc":"2.0","id":3,"method":"respond_permission","params":{"sessionId":"session-a","requestId":"approval-a","choice":"allow-session","trustKeys":["Bash:make"]}}`,
		`{"jsonrpc":"2.0","id":4,"method":"respond_user_question","params":{"requestId":"question-a","answers":{"Mode":"Fast","Features":["Tests","Review"]}}}`,
		`{"jsonrpc":"2.0","id":5,"method":"cancel_user_question","params":{"requestId":"question-a"}}`,
		`{"jsonrpc":"2.0","id":6,"method":"shutdown","params":{}}`,
	)
	if err != nil {
		t.Fatal(err)
	}
	if len(responses) != 6 {
		t.Fatalf("response count = %d, want 6: %s", len(responses), responses)
	}
	for _, expected := range []string{`"get_interruption_state"`, `"respond_permission"`, `"respond_user_question"`, `"cancel_user_question"`} {
		if !stringContains(responses[0], expected) {
			t.Fatalf("handshake missing %s: %s", expected, responses[0])
		}
	}
	for _, expected := range []string{`"requestId":"approval-a"`, `"status":"failed"`, `"trustGroups"`, `"requestId":"question-a"`, `"multiSelect":true`} {
		if !stringContains(responses[1], expected) {
			t.Fatalf("state missing %s: %s", expected, responses[1])
		}
	}
	if application.responded.Kind != permissions.AllowSession || len(application.responded.TrustKeys) != 1 || application.responded.TrustKeys[0] != "Bash:make" {
		t.Fatalf("permission response = %#v", application.responded)
	}
	if application.answer["Mode"].Single == nil || *application.answer["Mode"].Single != "Fast" || len(application.answer["Features"].Multiple) != 2 {
		t.Fatalf("question answer = %#v", application.answer)
	}
	if application.cancelled != "question-a" {
		t.Fatalf("cancelled request = %q", application.cancelled)
	}
}

func TestMalformedQuestionAnswerAndInterruptionFailureRemainVisible(t *testing.T) {
	application := &interruptionTestApplication{failReply: context.DeadlineExceeded}
	responses, err := exchangeProduct(t, application,
		`{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":1}}`,
		`{"jsonrpc":"2.0","id":2,"method":"respond_user_question","params":{"requestId":"q","answers":{"Mode":true}}}`,
		`{"jsonrpc":"2.0","id":3,"method":"get_interruption_state","params":{"sessionId":"a"}}`,
		`{"jsonrpc":"2.0","id":4,"method":"shutdown","params":{}}`,
	)
	if err != nil {
		t.Fatal(err)
	}
	if len(responses) != 4 || !stringContains(responses[1], `"code":-32602`) || !stringContains(responses[2], `"kind":"interruption"`) {
		t.Fatalf("malformed answer or failure was hidden: %s", responses)
	}
}

func exchangeProduct(t *testing.T, messages MessageApplication, requests ...string) ([]json.RawMessage, error) {
	t.Helper()
	client, backend := net.Pipe()
	defer client.Close()
	defer backend.Close()
	client.SetDeadline(time.Now().Add(5 * time.Second))
	ended := make(chan error, 1)
	go func() {
		ended <- ServeProductApplication(backend, backend, session.NewService(session.Catalog{}), messages)
		_ = backend.Close()
	}()
	frames := jsonrpc2.NewStream(client).(rawFrames)
	responses := make([]json.RawMessage, 0, len(requests))
	for _, request := range requests {
		if _, err := frames.WriteFrame(t.Context(), []byte(request)); err != nil {
			t.Fatal(err)
		}
		body, _, err := frames.ReadFrame(t.Context())
		if err != nil {
			t.Fatal(err)
		}
		responses = append(responses, append(json.RawMessage(nil), body...))
	}
	_ = client.Close()
	return responses, <-ended
}

func stringContains(value []byte, fragment string) bool {
	return json.Valid(value) && bytes.Contains(value, []byte(fragment))
}
