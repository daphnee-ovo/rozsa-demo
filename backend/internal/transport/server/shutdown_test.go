package server

import (
	"context"
	"errors"
	"net"
	"sync/atomic"
	"testing"
	"time"

	"go.lsp.dev/jsonrpc2"
	"rozsa/backend/internal/app/session"
)

type shutdownMessageApp struct {
	*fakeMessageApp
	calls   atomic.Int32
	failure error
}

func (app *shutdownMessageApp) Shutdown(ctx context.Context) error {
	app.calls.Add(1)
	if _, ok := ctx.Deadline(); !ok {
		return errors.New("shutdown has no deadline")
	}
	return app.failure
}

func TestShutdownAndDisconnectInvokeBusinessCleanupExactlyOnce(t *testing.T) {
	for _, mode := range []string{"rpc", "eof", "failure"} {
		t.Run(mode, func(t *testing.T) {
			app := &shutdownMessageApp{fakeMessageApp: &fakeMessageApp{}}
			if mode == "failure" {
				app.failure = errors.New("final write failed")
			}
			client, backend := net.Pipe()
			defer client.Close()
			defer backend.Close()
			client.SetDeadline(time.Now().Add(4 * time.Second))
			ended := make(chan error, 1)
			go func() {
				ended <- serveWithMessages(backend, backend, session.NewService(session.Catalog{}), app, time.Second)
				backend.Close()
			}()
			if mode != "eof" {
				stream := jsonrpc2.NewStream(client)
				if _, err := stream.Write(t.Context(), jsonrpc2.NewCall(jsonrpc2.NewNumberID(1), "shutdown", jsonrpc2.RawMessage(`{}`))); err != nil {
					t.Fatal(err)
				}
				message, _, err := stream.Read(t.Context())
				if err != nil {
					t.Fatal(err)
				}
				response := message.(*jsonrpc2.Response)
				if mode == "failure" && response.Err() == nil {
					t.Fatal("acknowledged failed final write")
				}
				if mode == "rpc" && (response.Err() != nil || string(response.Result()) != "true") {
					t.Fatalf("shutdown response: %s %v", response.Result(), response.Err())
				}
			}
			client.Close()
			select {
			case err := <-ended:
				if !errors.Is(err, app.failure) {
					t.Fatalf("termination error: %v want %v", err, app.failure)
				}
			case <-time.After(time.Second):
				t.Fatal("server did not finish")
			}
			if app.calls.Load() != 1 {
				t.Fatalf("cleanup calls: %d", app.calls.Load())
			}
		})
	}
}
