package server_test

import (
	"bytes"
	"context"
	"encoding/json"
	"net"
	"strconv"
	"testing"
	"time"

	"go.lsp.dev/jsonrpc2"
	"rozsa/backend/internal/app/permissions"
	"rozsa/backend/internal/app/questions"
	"rozsa/backend/internal/app/session"
	"rozsa/backend/internal/transport/server"
)

type interruptionTestApplication struct {
	*fakeMessageApp
	requests            []permissions.Request
	question            questions.Request
	responded           permissions.Response
	respondedIdentity   permissions.Identity
	respondedQuestionID string
	expectedIdentity    permissions.Identity
	expectedQuestionID  string
	approvalCalls       int
	questionCalls       int
	answer              questions.Answers
	cancelled           string
	failReply           error
}

// This fake checks transport forwarding and error mapping; it does not define business-owner identity validity.

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

func (application *interruptionTestApplication) RespondApproval(_ context.Context, identity permissions.Identity, response permissions.Response) error {
	application.approvalCalls++
	application.respondedIdentity = identity
	application.responded = response
	if identity != application.expectedIdentity {
		return permissions.ErrRequestNotFound
	}
	return application.failReply
}

func (application *interruptionTestApplication) RespondQuestion(_ context.Context, requestID string, answer questions.Answers) error {
	application.questionCalls++
	application.respondedQuestionID = requestID
	application.answer = answer
	if requestID != application.expectedQuestionID {
		return questions.ErrNoPending
	}
	return application.failReply
}

func (application *interruptionTestApplication) CancelQuestion(_ context.Context, requestID string) error {
	application.cancelled = requestID
	return application.failReply
}

func TestInterruptionRequestsExposeTypedStateAndResponses(t *testing.T) {
	application := &interruptionTestApplication{fakeMessageApp: &fakeMessageApp{},
		expectedIdentity: permissions.Identity{SessionID: "session-a", RequestID: "approval-a"}, expectedQuestionID: "question-a",
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
	for _, responseCase := range []struct{ index, id int }{{2, 3}, {3, 4}, {4, 5}, {5, 6}} {
		assertRPCSuccess(t, responses[responseCase.index], responseCase.id)
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
	if application.respondedIdentity != (permissions.Identity{SessionID: "session-a", RequestID: "approval-a"}) {
		t.Fatalf("permission identity = %+v", application.respondedIdentity)
	}
	if application.respondedQuestionID != "question-a" {
		t.Fatalf("question request identity = %q", application.respondedQuestionID)
	}
	if application.answer["Mode"].Single == nil || *application.answer["Mode"].Single != "Fast" || len(application.answer["Features"].Multiple) != 2 || application.answer["Features"].Multiple[0] != "Tests" || application.answer["Features"].Multiple[1] != "Review" {
		t.Fatalf("question answer = %#v", application.answer)
	}
	if application.cancelled != "question-a" {
		t.Fatalf("cancelled request = %q", application.cancelled)
	}
}

// Prevents incomplete or stale interruption identities from reaching successful response handling.
func TestInterruptionResponsesRejectWrongEmptyAndStaleIdentities(t *testing.T) {
	for _, testCase := range []struct {
		name, sessionID, requestID string
		expectedCode               int
	}{
		{"wrong session", "session-b", "approval-a", -32012},
		{"stale request", "session-a", "approval-old", -32012},
		{"empty session", "", "approval-a", -32602},
		{"empty request", "session-a", "", -32602},
	} {
		t.Run(testCase.name, func(t *testing.T) {
			application := &interruptionTestApplication{fakeMessageApp: &fakeMessageApp{}, expectedIdentity: permissions.Identity{SessionID: "session-a", RequestID: "approval-a"}}
			responses, err := exchangeProduct(t, application,
				`{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":1}}`,
				`{"jsonrpc":"2.0","id":2,"method":"respond_permission","params":{"sessionId":"`+testCase.sessionID+`","requestId":"`+testCase.requestID+`","choice":"allow-once"}}`,
				`{"jsonrpc":"2.0","id":3,"method":"shutdown","params":{}}`)
			if err != nil {
				t.Fatal(err)
			}
			if len(responses) != 3 {
				t.Fatalf("response count = %d, want 3: %s", len(responses), responses)
			}
			assertRPCError(t, responses[1], 2, testCase.expectedCode)
			if testCase.expectedCode == -32012 && application.respondedIdentity != (permissions.Identity{SessionID: testCase.sessionID, RequestID: testCase.requestID}) {
				t.Fatalf("forwarded identity = %+v", application.respondedIdentity)
			}
			if testCase.expectedCode == -32602 && application.approvalCalls != 0 {
				t.Fatalf("invalid empty identity reached application %d times", application.approvalCalls)
			}
		})
	}
	for _, requestID := range []string{"question-old", ""} {
		application := &interruptionTestApplication{fakeMessageApp: &fakeMessageApp{}, expectedQuestionID: "question-current"}
		responses, err := exchangeProduct(t, application,
			`{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":1}}`,
			`{"jsonrpc":"2.0","id":2,"method":"respond_user_question","params":{"requestId":"`+requestID+`","answers":{"Mode":"Fast"}}}`,
			`{"jsonrpc":"2.0","id":3,"method":"shutdown","params":{}}`)
		if err != nil {
			t.Fatal(err)
		}
		if len(responses) != 3 {
			t.Fatalf("response count = %d, want 3: %s", len(responses), responses)
		}
		wantCode := -32012
		if requestID == "" {
			wantCode = -32602
		}
		assertRPCError(t, responses[1], 2, wantCode)
		if requestID != "" && application.respondedQuestionID != requestID {
			t.Fatalf("forwarded question identity = %q", application.respondedQuestionID)
		}
		if requestID == "" && application.questionCalls != 0 {
			t.Fatalf("empty identity reached application %d times", application.questionCalls)
		}
	}
}

func TestMalformedQuestionAnswerAndInterruptionFailureRemainVisible(t *testing.T) {
	application := &interruptionTestApplication{fakeMessageApp: &fakeMessageApp{}, failReply: context.DeadlineExceeded}
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

func exchangeProduct(t *testing.T, messages server.MessageApplication, requests ...string) ([]json.RawMessage, error) {
	t.Helper()
	client, backend := net.Pipe()
	defer client.Close()
	defer backend.Close()
	client.SetDeadline(time.Now().Add(5 * time.Second))
	ended := make(chan error, 1)
	go func() {
		ended <- server.ServeProductApplication(backend, backend, session.NewService(session.Catalog{}), messages)
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

func assertRPCSuccess(t *testing.T, raw json.RawMessage, id int) {
	t.Helper()
	var response struct {
		ID     json.RawMessage `json:"id"`
		Result json.RawMessage `json:"result"`
		Error  json.RawMessage `json:"error"`
	}
	if err := json.Unmarshal(raw, &response); err != nil {
		t.Fatal(err)
	}
	if string(response.ID) != strconv.Itoa(id) || string(response.Result) != "true" || len(response.Error) != 0 {
		t.Fatalf("RPC response = %s, want id %d, result true, no error", raw, id)
	}
}

func assertRPCError(t *testing.T, raw json.RawMessage, id, code int) {
	t.Helper()
	var response struct {
		ID    json.RawMessage `json:"id"`
		Error *struct {
			Code int `json:"code"`
		} `json:"error"`
	}
	if err := json.Unmarshal(raw, &response); err != nil {
		t.Fatal(err)
	}
	if string(response.ID) != strconv.Itoa(id) || response.Error == nil || response.Error.Code != code {
		t.Fatalf("RPC response = %s, want id %d error code %d", raw, id, code)
	}
}

func stringContains(value []byte, fragment string) bool {
	return json.Valid(value) && bytes.Contains(value, []byte(fragment))
}
