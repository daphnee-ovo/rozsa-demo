// Package server routes implemented business methods over the library's peer.
package server

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"strings"
	"sync"
	"time"

	"go.lsp.dev/jsonrpc2"
	"rozsa/backend/internal/app/session"
	"rozsa/backend/internal/transport/types"
)

// Only adapts stdio ownership to io.ReadWriteCloser; contains no protocol code.
type stdio struct {
	io.Reader
	io.Writer
}

func (stdio stdio) Close() error {
	if closer, ok := stdio.Reader.(io.Closer); ok {
		return closer.Close()
	}
	return nil
}

// I/O lifetime hooks only: this is not a custom Stream or protocol parser.
type ownedIO struct {
	stdio
	stop context.CancelCauseFunc
}

func (ownedIO ownedIO) Read(data []byte) (int, error) {
	bytesRead, err := ownedIO.Reader.Read(data)
	if err != nil {
		ownedIO.stop(types.Error(types.Disconnected, err.Error()))
	}
	return bytesRead, err
}

func (ownedIO ownedIO) Write(data []byte) (int, error) {
	bytesWritten, err := ownedIO.Writer.Write(data)
	if err != nil {
		ownedIO.stop(types.Error(types.Disconnected, err.Error()))
	}
	return bytesWritten, err
}

const maxConcurrentRequests = 8
const requestTimeout = 30 * time.Second

func Serve(input io.Reader, output io.Writer, catalog session.Catalog) error {
	return ServeApplication(input, output, session.NewService(catalog))
}

// ServeApplication routes RPCs through the app-owned session service. The
// transport owns delivery and cancellation, not session business state.
func ServeApplication(input io.Reader, output io.Writer, app session.ApplicationService) error {
	return serve(input, output, app, requestTimeout)
}

// MessageApplication is the typed product boundary for agent message work.
// The application owns operation lifecycle, stream state and cancellation.
type MessageApplication interface {
	StartMessage(context.Context, string) (string, error)
	ContinueMessage(context.Context) (string, error)
	SubscribeMessageEvents(context.Context, string) (MessageEventSubscription, error)
	CancelMessage(context.Context, string) error
}

type RuntimeStatusApplication interface {
	RuntimeStatus() types.RuntimeStatusResult
}

type MessageEventSubscription interface {
	Next(context.Context) (types.MessageEvent, error)
	Close()
}

// ServeProductApplication exposes session and message methods over the same
// library-owned JSON-RPC connection.
func ServeProductApplication(input io.Reader, output io.Writer, sessions session.ApplicationService, messages MessageApplication) error {
	return serveWithMessages(input, output, sessions, messages, requestTimeout)
}

func serve(input io.Reader, output io.Writer, app session.ApplicationService, timeout time.Duration) error {
	return serveWithMessages(input, output, app, nil, timeout)
}

func serveWithMessages(input io.Reader, output io.Writer, app session.ApplicationService, messages MessageApplication, timeout time.Duration) error {
	lifetime, stop := context.WithCancelCause(context.Background())
	defer stop(types.Error(types.Disconnected, "Connection ended"))
	server := newServer(app, lifetime, timeout, messages)
	defer func() {
		stop(types.Error(types.Disconnected, "Connection ended"))
		server.sync.close()
		server.messageSync.close()
		server.jobs.Wait()
	}()
	conn := jsonrpc2.NewConn(jsonrpc2.NewStream(ownedIO{stdio{input, output}, stop}), jsonrpc2.WithPreempter(server))
	conn.Go(context.Background(), server.handle)
	// Done waits for incoming handlers to drain. The native public Err exposes
	// parser failures during that drain, but provides no read-failure callback.
	// Observe Err to unwind a blocked app/long poll without wrapping Stream.
	ticker := time.NewTicker(10 * time.Millisecond)
	defer ticker.Stop()
	for {
		select {
		case <-conn.Done():
			return conn.Err()
		case <-ticker.C:
			if err := conn.Err(); err != nil {
				stop(types.Error(types.Disconnected, err.Error()))
			}
		}
	}
}

type rpcServer struct {
	app        session.ApplicationService
	messages   MessageApplication
	lifetime   context.Context
	timeout    time.Duration
	slots      chan struct{}
	mu         sync.Mutex
	pending    map[jsonrpc2.ID]context.CancelCauseFunc
	jobs       sync.WaitGroup
	switchTail <-chan struct{}
	// Only accessed inline, before Async releases the read loop.
	ready, closing bool
	sync           sessionSync
	messageSync    messageSync
}

func newServer(app session.ApplicationService, lifetime context.Context, timeout time.Duration, messages ...MessageApplication) *rpcServer {
	var messageApp MessageApplication
	if len(messages) > 0 {
		messageApp = messages[0]
	}
	return &rpcServer{app: app, lifetime: lifetime, timeout: timeout,
		slots: make(chan struct{}, maxConcurrentRequests), pending: make(map[jsonrpc2.ID]context.CancelCauseFunc),
		sync: sessionSync{app: app}, messages: messageApp, messageSync: messageSync{app: messageApp}}
}

// Cancellation is the only accepted incoming notification. This ID is a
// cancellation parameter, not an independently decoded JSON-RPC envelope.
func (server *rpcServer) Preempt(_ context.Context, req *jsonrpc2.Request) (any, error) {
	if req.Method() != "$/cancelRequest" || req.IsCall() {
		return nil, jsonrpc2.ErrNotHandled
	}
	var params struct {
		ID json.RawMessage `json:"id"`
	}
	if !types.Params(req.Params(), &params) {
		return nil, nil
	}
	var id jsonrpc2.ID
	var number int64
	var text string
	if len(params.ID) == 0 || string(params.ID) == "null" {
		return nil, nil
	}
	if json.Unmarshal(params.ID, &number) == nil {
		id = jsonrpc2.NewNumberID(number)
	} else if json.Unmarshal(params.ID, &text) == nil {
		id = jsonrpc2.NewStringID(text)
	} else {
		return nil, nil
	}
	server.mu.Lock()
	if cancel := server.pending[id]; cancel != nil {
		cancel(types.Error(types.Canceled, "Request delivery canceled"))
	}
	server.mu.Unlock()
	return true, nil
}

func (server *rpcServer) handle(ctx context.Context, req *jsonrpc2.Request) (any, error) {
	if !req.IsCall() {
		return nil, nil
	}
	if server.closing {
		return nil, types.Error(types.Closing, "Backend is closing")
	}
	method := types.Method(strings.Clone(req.Method()))
	var switchParams types.SwitchParams
	var subscriptionParams subscriptionParams
	var startParams types.StartMessageParams
	var operationParams types.MessageOperationParams
	var messageSubscriptionParams types.MessageSubscriptionParams
	switch method {
	case types.Initialize:
		traceRPCStartup("initialize-request-received")
		var params types.InitializeParams
		if !types.Params(req.Params(), &params) || params.ProtocolVersion != 1 {
			return nil, types.Error(types.InvalidParams, "Unsupported protocol version or invalid params")
		}
		server.ready = true
		traceRPCStartup("initialize-response-ready")
		capabilities := []string{string(types.GetSessions), string(types.SwitchSession), string(types.SubscribeSessions), string(types.NextSessionEvent), string(types.UnsubscribeSessions)}
		if server.messages != nil {
			if _, ok := server.messages.(RuntimeStatusApplication); ok {
				capabilities = append(capabilities, string(types.RuntimeStatus))
			}
			capabilities = append(capabilities, string(types.StartMessage), string(types.ContinueMessage), string(types.SubscribeMessage), string(types.NextMessageEvent), string(types.UnsubscribeMessage), string(types.CancelMessage))
		}
		return types.InitializeResult{ProtocolVersion: 1, Capabilities: capabilities}, nil
	case types.Shutdown:
		if !types.Params(req.Params(), &types.EmptyParams{}) {
			return nil, types.Error(types.InvalidParams, "Invalid params")
		}
		server.closing = true
		server.mu.Lock()
		for _, cancel := range server.pending {
			cancel(types.Error(types.Closing, "Backend is closing"))
		}
		server.mu.Unlock()
		server.sync.close()
		server.messageSync.close()
		return true, nil
	case types.GetSessions, types.SubscribeSessions:
		if !types.Params(req.Params(), &types.EmptyParams{}) {
			return nil, types.Error(types.InvalidParams, "Invalid params")
		}
	case types.SwitchSession:
		if !types.Params(req.Params(), &switchParams) || switchParams.Path == "" || switchParams.CatalogID == "" {
			return nil, types.Error(types.InvalidParams, "Invalid params")
		}
	case types.NextSessionEvent, types.UnsubscribeSessions:
		if !types.Params(req.Params(), &subscriptionParams) || subscriptionParams.SubscriptionID == "" {
			return nil, types.Error(types.InvalidParams, "Invalid params")
		}
	case types.StartMessage:
		if !types.Params(req.Params(), &startParams) || strings.TrimSpace(startParams.Text) == "" {
			return nil, types.Error(types.InvalidParams, "Message text must not be empty")
		}
	case types.ContinueMessage:
		if !types.Params(req.Params(), &types.EmptyParams{}) {
			return nil, types.Error(types.InvalidParams, "Invalid params")
		}
	case types.SubscribeMessage:
		if !types.Params(req.Params(), &operationParams) || operationParams.OperationID == "" {
			return nil, types.Error(types.InvalidParams, "Invalid params")
		}
	case types.NextMessageEvent, types.UnsubscribeMessage:
		if !types.Params(req.Params(), &messageSubscriptionParams) || messageSubscriptionParams.SubscriptionID == "" {
			return nil, types.Error(types.InvalidParams, "Invalid params")
		}
	case types.CancelMessage:
		if !types.Params(req.Params(), &operationParams) || operationParams.OperationID == "" {
			return nil, types.Error(types.InvalidParams, "Invalid params")
		}
	case types.RuntimeStatus:
		if !types.Params(req.Params(), &types.EmptyParams{}) {
			return nil, types.Error(types.InvalidParams, "Invalid params")
		}
		status, ok := server.messages.(RuntimeStatusApplication)
		if !ok {
			return nil, types.Error(types.MethodNotFound, "Runtime status is unavailable")
		}
		result := status.RuntimeStatus()
		if result.State == "failed" {
			return nil, types.Error(types.RuntimeInitialization, result.Error)
		}
		return result, nil
	default:
		return nil, types.Error(types.MethodNotFound, "Method not found")
	}
	if !server.ready {
		return nil, types.Error(types.NotInitialized, "Backend is not initialized")
	}
	if method == types.UnsubscribeMessage {
		return server.messageSync.unsubscribe(messageSubscriptionParams.SubscriptionID)
	}
	if method == types.UnsubscribeSessions {
		return server.sync.unsubscribe(subscriptionParams.SubscriptionID)
	}
	select {
	case server.slots <- struct{}{}:
	default:
		return nil, types.Error(types.Busy, "Concurrent request capacity reached")
	}
	// Register in wire order before releasing, so cancellation cannot overtake
	// registration. The work context is owned, never a pooled request context.
	deadline, endDeadline := context.WithTimeout(server.lifetime, server.timeout)
	defer endDeadline()
	work, cancel := context.WithCancelCause(deadline)
	defer cancel(context.Canceled)
	stopRequest := context.AfterFunc(ctx, func() { cancel(types.Error(types.Disconnected, "Request connection interrupted")) })
	defer stopRequest()
	id := req.ID()
	server.mu.Lock()
	server.pending[id] = cancel
	server.mu.Unlock()
	defer func() { server.mu.Lock(); delete(server.pending, id); server.mu.Unlock() }()
	var cursor *sessionCursor
	if method == types.SubscribeSessions {
		cursor = server.sync.subscribe()
	}
	// Preserve accepted switch order while reads and event waits overlap. Slots
	// belong to workers, so canceled delivery cannot create unbounded app work.
	var previous <-chan struct{}
	var completed chan struct{}
	if method == types.SwitchSession {
		previous = server.switchTail
		completed = make(chan struct{})
		server.switchTail = completed
	}
	type outcome struct {
		result any
		err    error
	}
	finished := make(chan outcome, 1)
	server.jobs.Add(1)
	go func() {
		defer server.jobs.Done()
		defer func() { <-server.slots }()
		if completed != nil {
			defer close(completed)
			if previous != nil {
				<-previous
			}
		}
		if work.Err() != nil {
			finished <- outcome{err: work.Err()}
			return
		}
		var result any
		var err error
		switch method {
		case types.GetSessions:
			result, err = server.app.GetSessions(work)
			if err != nil {
				err = types.Error(types.Catalog, err.Error())
			}
		case types.SwitchSession:
			result, err = server.app.Switch(work, switchParams.Path, switchParams.CatalogID)
			if err != nil {
				err = types.Error(types.Session, err.Error())
			}
		case types.SubscribeSessions:
			result, err = cursor.snapshot(work)
		case types.NextSessionEvent:
			result, err = server.sync.next(work, subscriptionParams.SubscriptionID)
		case types.StartMessage, types.ContinueMessage, types.SubscribeMessage, types.NextMessageEvent, types.CancelMessage:
			result, err = server.messageSync.handle(work, method, startParams, operationParams, messageSubscriptionParams)
		}
		finished <- outcome{result, err}
	}()
	jsonrpc2.Async(ctx)
	select {
	case response := <-finished:
		return response.result, requestError(work, response.err)
	case <-work.Done():
		if cursor != nil {
			cursor.close()
		}
		return nil, requestError(work, work.Err())
	}
}

func traceRPCStartup(stage string) {
	if os.Getenv("ROZSA_PERF_TRACE") == "1" {
		fmt.Fprintf(os.Stderr, "ROZSA_PERF_BACKEND %s %d\n", stage, time.Now().UnixMilli())
	}
}

func requestError(ctx context.Context, err error) error {
	if cause := context.Cause(ctx); cause != nil {
		var rpcError *jsonrpc2.Error
		if errors.As(cause, &rpcError) {
			return rpcError
		}
		if errors.Is(cause, context.DeadlineExceeded) {
			return types.Error(types.Timeout, "Request timed out")
		}
		return types.Error(types.Canceled, "Request delivery canceled")
	}
	return err
}
