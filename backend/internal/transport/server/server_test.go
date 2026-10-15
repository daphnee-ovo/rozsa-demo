package server

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"go.lsp.dev/jsonrpc2"
	"io"
	"rozsa/backend/internal/app/session"
	"strings"
	"testing"
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
	var in, out bytes.Buffer
	writer := jsonrpc2.NewStream(stdio{strings.NewReader(""), &in}).(rawFrames)
	for _, req := range requests {
		if _, err := writer.WriteFrame(t.Context(), []byte(req)); err != nil {
			t.Fatal(err)
		}
	}
	serveErr := Serve(&in, &out, catalog)
	reader := jsonrpc2.NewStream(stdio{&out, io.Discard}).(rawFrames)
	responses := []json.RawMessage{}
	for {
		body, _, err := reader.ReadFrame(t.Context())
		if err == io.EOF {
			break
		}
		if err != nil {
			t.Fatal(err)
		}
		responses = append(responses, append(json.RawMessage(nil), body...))
	}
	return responses, serveErr
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
	for i, want := range []string{`"capabilities":["get_sessions","switch_session"]`, `"result":[]`, `"result":true`, `"kind":"closing"`} {
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
