// Tests the selected library framing contract, never a project implementation.
package framing_test

import (
	"go.lsp.dev/jsonrpc2"
	"io"
	"strings"
	"testing"
)

type transport struct {
	io.Reader
	io.Writer
}

func (transport) Close() error { return nil }

func TestLibraryFramingContract(t *testing.T) {
	// T066 explicitly approves LF and last-value-wins for repeated lengths.
	for _, prefix := range []string{"Content-Length: 0\nContent-Length: 36\n\n", "Content-Length: 36\r\n\r\n"} {
		msg, _, err := jsonrpc2.NewStream(transport{strings.NewReader(prefix + `{"jsonrpc":"2.0","id":1,"result":[]}`), io.Discard}).Read(t.Context())
		if err != nil || msg == nil {
			t.Fatalf("%v %v", msg, err)
		}
	}
	for _, input := range []string{"Content-Length: 4\r\n\r\n{}", "Content-Length: -1\r\n\r\n", "Content-Length: 1073741825\r\n\r\n", "X: y\r\n\r\n"} {
		if _, _, err := jsonrpc2.NewStream(transport{strings.NewReader(input), io.Discard}).Read(t.Context()); err == nil {
			t.Fatal("accepted broken frame")
		}
	}
}
