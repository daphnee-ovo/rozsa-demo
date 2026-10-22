package server_test

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"go.lsp.dev/jsonrpc2"
	"net"
	"rozsa/backend/internal/app/session"
	"rozsa/backend/internal/transport/server"
	"rozsa/backend/internal/transport/types"
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

type testPeer struct {
	t      *testing.T
	client net.Conn
	stream jsonrpc2.Stream
	ended  chan error
}

func transportID(id any) jsonrpc2.ID {
	switch id := id.(type) {
	case int:
		return jsonrpc2.NewNumberID(int64(id))
	case string:
		return jsonrpc2.NewStringID(id)
	default:
		panic("test ID must be integer or string")
	}
}

func (peer *testPeer) send(id any, method string, params any) {
	peer.t.Helper()
	data, err := json.Marshal(params)
	if err != nil {
		peer.t.Fatal(err)
	}
	if _, err := peer.stream.Write(peer.t.Context(), jsonrpc2.NewCall(transportID(id), method, jsonrpc2.RawMessage(data))); err != nil {
		peer.t.Fatal(err)
	}
}

func (peer *testPeer) cancel(id any) {
	peer.t.Helper()
	data, err := json.Marshal(struct {
		ID any `json:"id"`
	}{id})
	if err != nil {
		peer.t.Fatal(err)
	}
	if _, err := peer.stream.Write(peer.t.Context(), jsonrpc2.NewNotification("$/cancelRequest", jsonrpc2.RawMessage(data))); err != nil {
		peer.t.Fatal(err)
	}
}

func (peer *testPeer) read() *jsonrpc2.Response {
	peer.t.Helper()
	message, _, err := peer.stream.Read(peer.t.Context())
	if err != nil {
		peer.t.Fatal(err)
	}
	response, ok := message.(*jsonrpc2.Response)
	if !ok {
		peer.t.Fatalf("unexpected message %T", message)
	}
	return response
}

func (peer *testPeer) readOK() *jsonrpc2.Response {
	peer.t.Helper()
	response := peer.read()
	if response.Err() != nil {
		peer.t.Fatal(response.Err())
	}
	return response
}

func errorKind(t *testing.T, err error, want types.ErrorKind) {
	t.Helper()
	var rpcError *jsonrpc2.Error
	if !errors.As(err, &rpcError) {
		t.Fatalf("want %s typed error, got %v", want, err)
	}
	var data types.ErrorData
	if json.Unmarshal(rpcError.Data, &data) != nil || data.Kind != want {
		t.Fatalf("want %s, got %+v", want, rpcError)
	}
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
	go func() { ended <- server.Serve(backend, backend, catalog); backend.Close() }()
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
	if err := server.Serve(strings.NewReader("Content-Length: 5\r\n\r\n{}"), &bytes.Buffer{}, session.Catalog{}); err == nil {
		t.Fatal("truncated frame accepted")
	}
}

type notificationTestApplication struct {
	server.MessageApplication
	notifications <-chan server.AppNotification
}

func (application notificationTestApplication) AppNotifications() <-chan server.AppNotification {
	return application.notifications
}

func TestAppNotificationSourcePublishesRendererEvent(t *testing.T) {
	client, backend := net.Pipe()
	client.SetDeadline(time.Now().Add(5 * time.Second))
	notifications := make(chan server.AppNotification, 1)
	ended := make(chan error, 1)
	go func() {
		ended <- server.ServeProductApplication(backend, backend, session.NewService(session.Catalog{}), notificationTestApplication{notifications: notifications})
		_ = backend.Close()
	}()
	stream := jsonrpc2.NewStream(client)
	if _, err := stream.Write(t.Context(), jsonrpc2.NewCall(jsonrpc2.NewNumberID(1), "initialize", jsonrpc2.RawMessage(`{"protocolVersion":1}`))); err != nil {
		t.Fatal(err)
	}
	if _, _, err := stream.Read(t.Context()); err != nil {
		t.Fatal(err)
	}
	notifications <- server.AppNotification{Type: "upsert", ID: "builtin-skills-initialization", Severity: "error", Title: "Built-in skills unavailable", Message: "remote unavailable", TimeoutMs: 6000}
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
