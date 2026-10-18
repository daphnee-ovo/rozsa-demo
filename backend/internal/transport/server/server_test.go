package server

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"go.lsp.dev/jsonrpc2"
	"net"
	"rozsa/backend/internal/app/session"
	"strings"
	"testing"
	"time"
)

// Uses the library's raw frame methods to inject malformed JSON test inputs.
// This is a view of the concrete library stream, not a custom implementation.
type rawFrames interface {
	ReadFrame(context.Context) ([]byte, int64, error)
	WriteFrame(context.Context, []byte) (int64, error)
}

func call(t *testing.T, catalog session.Catalog, requests ...string) []json.RawMessage {
	responses, err := exchange(t, catalog, requests...)
	if err != nil {
		t.Fatal(err)
	}
	return responses
}
func exchange(t *testing.T, catalog session.Catalog, requests ...string) ([]json.RawMessage, error) {
	t.Helper()
	client, backend := net.Pipe()
	defer client.Close()
	defer backend.Close()
	client.SetDeadline(time.Now().Add(5 * time.Second))
	ended := make(chan error, 1)
	go func() { ended <- Serve(backend, backend, catalog); backend.Close() }()
	frames := jsonrpc2.NewStream(client).(rawFrames)
	responses := []json.RawMessage{}
	for _, req := range requests {
		if _, err := frames.WriteFrame(t.Context(), []byte(req)); err != nil {
			t.Fatal(err)
		}
		message, err := jsonrpc2.DecodeMessage([]byte(req))
		// The shared fixture includes an empty batch, which the connection
		// answers even though DecodeMessage's single-message API rejects it.
		emptyBatch := bytes.Equal(bytes.TrimSpace([]byte(req)), []byte("[]"))
		if err != nil && !emptyBatch {
			break
		}
		if _, ok := message.(*jsonrpc2.Call); ok || emptyBatch {
			body, _, err := frames.ReadFrame(t.Context())
			if err != nil {
				t.Fatal(err)
			}
			responses = append(responses, append(json.RawMessage(nil), body...))
		}
	}
	client.Close()
	return responses, <-ended
}

func TestParserFailureIsTerminationNotResponse(t *testing.T) {
	for _, input := range []string{`{`, `{"jsonrpc":"2.0","id":true,"method":"get_sessions"}`} {
		responses, err := exchange(t, session.Catalog{}, input, initRequest)
		var rpcError *jsonrpc2.Error
		if !errors.As(err, &rpcError) || len(responses) != 0 {
			t.Fatalf("error=%v responses=%s", err, responses)
		}
	}
}

const initRequest = `{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":1}}`

func TestHandshakeListShutdown(t *testing.T) {
	responses := call(t, session.Catalog{Directories: []string{"../../../../tmp/nonexistent-catalog-layer"}}, initRequest,
		`{"jsonrpc":"2.0","id":2,"method":"get_sessions","params":{}}`,
		`{"jsonrpc":"2.0","id":"close","method":"shutdown","params":null}`,
		`{"jsonrpc":"2.0","id":"late","method":"get_sessions"}`)
	for i, want := range []string{`"capabilities":["get_sessions","switch_session","subscribe_sessions","next_session_event","unsubscribe_sessions"]`, `"result":[]`, `"result":true`, `"kind":"closing"`} {
		if !bytes.Contains(responses[i], []byte(want)) {
			t.Fatalf("%s", responses[i])
		}
	}
}
func TestUninitializedAndNotificationGating(t *testing.T) {
	responses := call(t, session.Catalog{},
		`{"jsonrpc":"2.0","method":"initialize","params":{"protocolVersion":1}}`,
		`{"jsonrpc":"2.0","id":null,"method":"shutdown"}`,
		`{"jsonrpc":"2.0","id":2,"method":"get_sessions"}`,
		initRequest, `{"jsonrpc":"2.0","method":"unknown"}`, `{"jsonrpc":"2.0","id":3,"method":"get_sessions"}`)
	if len(responses) != 3 {
		t.Fatalf("notification produced a reply: %s", responses)
	}
	if !bytes.Contains(responses[0], []byte(`"kind":"not_initialized"`)) {
		t.Fatal(string(responses[0]))
	}
	if !bytes.Contains(responses[2], []byte(`"kind":"catalog"`)) {
		t.Fatal(string(responses[2]))
	}
	if err := Serve(strings.NewReader("Content-Length: 5\r\n\r\n{}"), &bytes.Buffer{}, session.Catalog{}); err == nil {
		t.Fatal("truncated frame accepted")
	}
}

type notificationTestApplication struct {
	MessageApplication
	notifications <-chan AppNotification
}

func (application notificationTestApplication) AppNotifications() <-chan AppNotification {
	return application.notifications
}

func TestAppNotificationSourcePublishesRendererEvent(t *testing.T) {
	client, backend := net.Pipe()
	client.SetDeadline(time.Now().Add(5 * time.Second))
	notifications := make(chan AppNotification, 1)
	ended := make(chan error, 1)
	go func() {
		ended <- ServeProductApplication(backend, backend, session.NewService(session.Catalog{}), notificationTestApplication{notifications: notifications})
		_ = backend.Close()
	}()
	stream := jsonrpc2.NewStream(client)
	if _, err := stream.Write(t.Context(), jsonrpc2.NewCall(jsonrpc2.NewNumberID(1), "initialize", jsonrpc2.RawMessage(`{"protocolVersion":1}`))); err != nil {
		t.Fatal(err)
	}
	if _, _, err := stream.Read(t.Context()); err != nil {
		t.Fatal(err)
	}
	notifications <- AppNotification{Type: "upsert", ID: "builtin-skills-initialization", Severity: "error", Title: "Built-in skills unavailable", Message: "remote unavailable", TimeoutMs: 6000}
	frames := stream.(rawFrames)
	frame, _, err := frames.ReadFrame(t.Context())
	if err != nil {
		t.Fatal(err)
	}
	for _, expected := range []string{`"method":"app-notification"`, `"severity":"error"`, `remote unavailable`} {
		if !bytes.Contains(frame, []byte(expected)) {
			t.Fatalf("notification frame missing %q: %s", expected, frame)
		}
	}
	if _, err := stream.Write(t.Context(), jsonrpc2.NewCall(jsonrpc2.NewNumberID(2), "shutdown", jsonrpc2.RawMessage(`{}`))); err != nil {
		t.Fatal(err)
	}
	if _, _, err := stream.Read(t.Context()); err != nil {
		t.Fatal(err)
	}
	_ = client.Close()
	if err := <-ended; err != nil {
		t.Fatal(err)
	}
}
