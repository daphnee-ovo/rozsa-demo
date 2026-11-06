package server_test

import (
	"context"
	"encoding/json"
	"errors"
	"net"
	"testing"
	"time"

	"go.lsp.dev/jsonrpc2"
	"rozsa/backend/internal/app/session"
	"rozsa/backend/internal/transport/server"
	"rozsa/backend/internal/transport/types"
)

type fakeMessageApp struct {
	startedText   string
	continued     bool
	canceled      string
	startErr      error
	cancelErr     error
	events        chan types.MessageEvent
	nextStarted   chan struct{}
	runtimeStatus types.RuntimeStatusResult
}

func (app *fakeMessageApp) RuntimeStatus() types.RuntimeStatusResult { return app.runtimeStatus }

func (app *fakeMessageApp) StartMessage(_ context.Context, text string) (string, error) {
	app.startedText = text
	return "operation-start", app.startErr
}

func (app *fakeMessageApp) ContinueMessage(context.Context) (string, error) {
	app.continued = true
	return "operation-continue", nil
}

func (app *fakeMessageApp) SubscribeMessageEvents(_ context.Context, operationID string) (server.MessageEventSubscription, error) {
	if operationID != "operation-start" {
		return nil, errors.New("unknown operation")
	}
	return &fakeMessageSubscription{events: app.events, nextStarted: app.nextStarted}, nil
}

func (app *fakeMessageApp) CancelMessage(_ context.Context, operationID string) error {
	app.canceled = operationID
	if app.cancelErr != nil {
		return app.cancelErr
	}
	app.events <- types.MessageEvent{Sequence: 3, Kind: types.MessageCanceled}
	return nil
}

type fakeMessageSubscription struct {
	events      chan types.MessageEvent
	nextStarted chan struct{}
}

func (subscription *fakeMessageSubscription) Next(ctx context.Context) (types.MessageEvent, error) {
	if subscription.nextStarted != nil {
		select {
		case subscription.nextStarted <- struct{}{}:
		default:
		}
	}
	select {
	case <-ctx.Done():
		return types.MessageEvent{}, ctx.Err()
	case event := <-subscription.events:
		return event, nil
	}
}

func (subscription *fakeMessageSubscription) Close() {}

func startMessagePeer(t *testing.T, app *fakeMessageApp) *testPeer {
	t.Helper()
	client, backend := net.Pipe()
	client.SetDeadline(time.Now().Add(8 * time.Second))
	peer := &testPeer{t: t, client: client, stream: jsonrpc2.NewStream(client), ended: make(chan error, 1)}
	go func() {
		peer.ended <- server.ServeProductApplication(backend, backend, session.NewService(session.Catalog{}), app)
		backend.Close()
	}()
	t.Cleanup(func() {
		client.Close()
		select {
		case <-peer.ended:
		case <-time.After(3 * time.Second):
			backend.Close()
			t.Error("server did not drain")
		}
	})
	peer.send(1, "initialize", types.InitializeParams{ProtocolVersion: 1})
	peer.readOK()
	return peer
}

func TestMessageMethodsReachTypedApplicationAndDeliverEvents(t *testing.T) {
	app := &fakeMessageApp{events: make(chan types.MessageEvent, 4), runtimeStatus: types.RuntimeStatusResult{State: "ready"}}
	peer := startMessagePeer(t, app)
	peer.send(20, string(types.RuntimeStatus), types.EmptyParams{})
	var status types.RuntimeStatusResult
	if err := json.Unmarshal(peer.readOK().Result(), &status); err != nil || status.State != "ready" {
		t.Fatalf("runtime status=%+v err=%v", status, err)
	}
	peer.send(2, string(types.StartMessage), types.StartMessageParams{Text: "hello"})
	var operation types.MessageOperation
	if err := json.Unmarshal(peer.readOK().Result(), &operation); err != nil || operation.OperationID != "operation-start" || app.startedText != "hello" {
		t.Fatalf("operation=%+v app=%+v err=%v", operation, app, err)
	}
	peer.send(3, string(types.SubscribeMessage), types.MessageOperationParams{OperationID: operation.OperationID})
	var subscription types.MessageSubscription
	if err := json.Unmarshal(peer.readOK().Result(), &subscription); err != nil || subscription.OperationID != operation.OperationID || subscription.SubscriptionID == "" {
		t.Fatalf("subscription=%+v err=%v", subscription, err)
	}
	delta := "partial"
	app.events <- types.MessageEvent{Sequence: 1, Kind: types.MessageTextDelta, Delta: &delta}
	peer.send(4, string(types.NextMessageEvent), types.MessageSubscriptionParams{SubscriptionID: subscription.SubscriptionID})
	var received types.MessageEventResult
	if err := json.Unmarshal(peer.readOK().Result(), &received); err != nil || received.Event.Delta == nil || *received.Event.Delta != delta || received.OperationID != operation.OperationID {
		t.Fatalf("received=%+v err=%v", received, err)
	}
	streamError := "provider stream failed"
	app.events <- types.MessageEvent{Sequence: 2, Kind: types.MessageFailed, Error: &streamError}
	peer.send(9, string(types.NextMessageEvent), types.MessageSubscriptionParams{SubscriptionID: subscription.SubscriptionID})
	if err := json.Unmarshal(peer.readOK().Result(), &received); err != nil || received.Event.Kind != types.MessageFailed || received.Event.Error == nil || *received.Event.Error != streamError {
		t.Fatalf("stream error event=%+v err=%v", received, err)
	}
	peer.send(5, string(types.CancelMessage), types.MessageOperationParams{OperationID: operation.OperationID})
	if result := string(peer.readOK().Result()); result != "true" || app.canceled != operation.OperationID {
		t.Fatalf("cancel result=%s app=%+v", result, app)
	}
	peer.send(6, string(types.NextMessageEvent), types.MessageSubscriptionParams{SubscriptionID: subscription.SubscriptionID})
	if err := json.Unmarshal(peer.readOK().Result(), &received); err != nil || received.Event.Kind != types.MessageCanceled || received.Event.Sequence != 3 {
		t.Fatalf("canceled event=%+v err=%v", received, err)
	}
	peer.send(7, string(types.UnsubscribeMessage), types.MessageSubscriptionParams{SubscriptionID: subscription.SubscriptionID})
	if result := string(peer.readOK().Result()); result != "true" {
		t.Fatalf("unsubscribe result=%s", result)
	}
	peer.send(8, string(types.ContinueMessage), types.EmptyParams{})
	if err := json.Unmarshal(peer.readOK().Result(), &operation); err != nil || operation.OperationID != "operation-continue" || !app.continued {
		t.Fatalf("continue=%+v app=%+v err=%v", operation, app, err)
	}
}

func TestToolPhaseEventsPreserveCallIdentityAndRejectMalformedPhases(t *testing.T) {
	for _, invalid := range []types.MessageEvent{
		{Sequence: 2, Kind: types.MessageToolPhase, Phase: types.ToolFinished},
		{Sequence: 2, Kind: types.MessageToolPhase, CallID: "call-read", Phase: "unknown"},
	} {
		app := &fakeMessageApp{events: make(chan types.MessageEvent, 2), runtimeStatus: types.RuntimeStatusResult{State: "ready"}}
		peer := startMessagePeer(t, app)
		peer.send(1, string(types.SubscribeMessage), types.MessageOperationParams{OperationID: "operation-start"})
		var subscription types.MessageSubscription
		if err := json.Unmarshal(peer.readOK().Result(), &subscription); err != nil {
			t.Fatal(err)
		}
		app.events <- types.MessageEvent{Sequence: 1, Kind: types.MessageToolPhase, CallID: "call-read", Phase: types.ToolStarted}
		peer.send(2, string(types.NextMessageEvent), types.MessageSubscriptionParams{SubscriptionID: subscription.SubscriptionID})
		var received types.MessageEventResult
		if err := json.Unmarshal(peer.readOK().Result(), &received); err != nil || received.Event.CallID != "call-read" ||
			received.Event.Phase != types.ToolStarted || received.Event.Sequence != 1 {
			t.Fatalf("tool phase event=%+v err=%v", received, err)
		}
		app.events <- invalid
		peer.send(3, string(types.NextMessageEvent), types.MessageSubscriptionParams{SubscriptionID: subscription.SubscriptionID})
		errorKind(t, peer.read().Err(), types.Message)
	}
}

func TestRuntimeInitializationFailureIsReturnedAsTypedStatusError(t *testing.T) {
	app := &fakeMessageApp{
		events:        make(chan types.MessageEvent, 1),
		runtimeStatus: types.RuntimeStatusResult{State: "failed", Error: "model credentials are missing"},
	}
	peer := startMessagePeer(t, app)
	peer.send(30, string(types.RuntimeStatus), types.EmptyParams{})
	response := peer.read()
	var rpcError *jsonrpc2.Error
	if !errors.As(response.Err(), &rpcError) || rpcError.Code != -32009 {
		t.Fatalf("runtime failure response = %v, want typed runtime initialization error", response.Err())
	}
	var data types.ErrorData
	if err := json.Unmarshal(rpcError.Data, &data); err != nil || data.Kind != types.RuntimeInitialization || rpcError.Message != "model credentials are missing" {
		t.Fatalf("runtime failure error = code %d data %s message %q err=%v", rpcError.Code, rpcError.Data, rpcError.Message, err)
	}
}

func TestMessageApplicationErrorsRemainVisible(t *testing.T) {
	app := &fakeMessageApp{events: make(chan types.MessageEvent, 1), startErr: errors.New("provider unavailable"), cancelErr: errors.New("operation missing")}
	peer := startMessagePeer(t, app)
	peer.send(2, string(types.StartMessage), types.StartMessageParams{Text: "hello"})
	errorKind(t, peer.read().Err(), types.Message)
	peer.send(3, string(types.CancelMessage), types.MessageOperationParams{OperationID: "absent"})
	errorKind(t, peer.read().Err(), types.Message)
}

func TestCanceledReceiveCanResumeOnSameMessageSubscription(t *testing.T) {
	app := &fakeMessageApp{events: make(chan types.MessageEvent, 2), nextStarted: make(chan struct{}, 1)}
	peer := startMessagePeer(t, app)
	peer.send(2, string(types.StartMessage), types.StartMessageParams{Text: "hello"})
	var operation types.MessageOperation
	if err := json.Unmarshal(peer.readOK().Result(), &operation); err != nil {
		t.Fatal(err)
	}
	peer.send(3, string(types.SubscribeMessage), types.MessageOperationParams{OperationID: operation.OperationID})
	var subscription types.MessageSubscription
	if err := json.Unmarshal(peer.readOK().Result(), &subscription); err != nil {
		t.Fatal(err)
	}
	peer.send(4, string(types.NextMessageEvent), types.MessageSubscriptionParams{SubscriptionID: subscription.SubscriptionID})
	select {
	case <-app.nextStarted:
	case <-time.After(time.Second):
		t.Fatal("receive did not reach message subscription")
	}
	peer.cancel(4)
	errorKind(t, peer.read().Err(), types.Canceled)
	delta := "resumed"
	app.events <- types.MessageEvent{Sequence: 1, Kind: types.MessageTextDelta, Delta: &delta}
	peer.send(5, string(types.NextMessageEvent), types.MessageSubscriptionParams{SubscriptionID: subscription.SubscriptionID})
	var received types.MessageEventResult
	if err := json.Unmarshal(peer.readOK().Result(), &received); err != nil || received.Event.Delta == nil || *received.Event.Delta != delta {
		t.Fatalf("resumed event=%+v err=%v", received, err)
	}
}
