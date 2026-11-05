package server_test

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net"
	"os"
	"path/filepath"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"go.lsp.dev/jsonrpc2"
	"rozsa/backend/internal/app/session"
	"rozsa/backend/internal/transport/server"
	"rozsa/backend/internal/transport/types"
)

const maxConcurrentRequests = 8

type subscriptionParams struct {
	SubscriptionID string `json:"subscriptionId"`
}
type sessionUpdate struct {
	SubscriptionID string                 `json:"subscriptionId"`
	Event          *session.Event         `json:"event"`
	Snapshot       *session.StateSnapshot `json:"snapshot"`
}
type readWriteCloser struct {
	io.Reader
	io.Writer
}

func (readWriteCloser) Close() error { return nil }

type fakeApp struct {
	*session.Service
	get           func(context.Context) ([]session.ListEntry, error)
	switchSession func(context.Context, string, string) (session.LoadedSnapshot, error)
	snapshot      func(context.Context) (session.StateSnapshot, error)
	subscribe     func() session.EventSubscription
}

func (fakeApp *fakeApp) GetSessions(ctx context.Context) ([]session.ListEntry, error) {
	if fakeApp.get != nil {
		return fakeApp.get(ctx)
	}
	return fakeApp.Service.GetSessions(ctx)
}
func (fakeApp *fakeApp) Switch(ctx context.Context, path, id string) (session.LoadedSnapshot, error) {
	if fakeApp.switchSession != nil {
		return fakeApp.switchSession(ctx, path, id)
	}
	return fakeApp.Service.Switch(ctx, path, id)
}
func (fakeApp *fakeApp) Snapshot(ctx context.Context) (session.StateSnapshot, error) {
	if fakeApp.snapshot != nil {
		return fakeApp.snapshot(ctx)
	}
	return fakeApp.Service.Snapshot(ctx)
}
func (fakeApp *fakeApp) Subscribe() session.EventSubscription {
	if fakeApp.subscribe != nil {
		return fakeApp.subscribe()
	}
	return fakeApp.Service.Subscribe()
}

func startPeer(t *testing.T, app session.ApplicationService, timeout time.Duration) *testPeer {
	t.Helper()
	client, backend := net.Pipe()
	client.SetDeadline(time.Now().Add(timeout))
	peer := &testPeer{t: t, client: client, stream: jsonrpc2.NewStream(client), ended: make(chan error, 1)}
	go func() { peer.ended <- server.ServeApplication(backend, backend, app); backend.Close() }()
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
func (peer *testPeer) update() sessionUpdate {
	peer.t.Helper()
	var update sessionUpdate
	if err := json.Unmarshal(peer.readOK().Result(), &update); err != nil {
		peer.t.Fatal(err)
	}
	if (update.Event == nil) == (update.Snapshot == nil) {
		peer.t.Fatalf("not a snapshot/event union: %+v", update)
	}
	return update
}
func waitFor[T any](t *testing.T, channel <-chan T) T {
	t.Helper()
	select {
	case value := <-channel:
		return value
	case <-time.After(3 * time.Second):
		t.Fatal("observable app step timed out")
		var zero T
		return zero
	}
}

func TestConcurrentCapacityCancellationAndShutdown(t *testing.T) {
	started := make(chan struct{}, maxConcurrentRequests)
	var calls atomic.Int32
	app := &fakeApp{get: func(ctx context.Context) ([]session.ListEntry, error) {
		calls.Add(1)
		started <- struct{}{}
		<-ctx.Done()
		return nil, ctx.Err()
	}}
	peer := startPeer(t, app, 5*time.Second)
	for i := 0; i < maxConcurrentRequests; i++ {
		peer.send(i+2, "get_sessions", nil)
		waitFor(t, started)
	}
	peer.send("excess", "get_sessions", nil)
	errorKind(t, peer.read().Err(), types.Busy)
	peer.cancel(2)
	response := peer.read()
	if response.ID() != transportID(2) {
		t.Fatal(response.ID())
	}
	errorKind(t, response.Err(), types.Canceled)
	peer.send("shutdown", "shutdown", nil)
	for i := 0; i < maxConcurrentRequests; i++ {
		response := peer.read()
		if response.ID() != transportID("shutdown") {
			errorKind(t, response.Err(), types.Closing)
		}
	}
	peer.send("late", "get_sessions", nil)
	errorKind(t, peer.read().Err(), types.Closing)
	if calls.Load() != maxConcurrentRequests {
		t.Fatalf("work retried or excess work accepted: %d", calls.Load())
	}
}

func TestCancellationRegistrationAndStringIdentity(t *testing.T) {
	started := make(chan struct{}, 1)
	app := &fakeApp{get: func(ctx context.Context) ([]session.ListEntry, error) {
		started <- struct{}{}
		<-ctx.Done()
		return nil, ctx.Err()
	}}
	peer := startPeer(t, app, 5*time.Second)
	peer.send("界面", "get_sessions", nil)
	// No synchronization with app start: the immediately following cancel must
	// not overtake registration. Numeric IDs cannot cancel the string request.
	peer.cancel(2)
	peer.cancel("界面")
	response := peer.read()
	if response.ID() != transportID("界面") {
		t.Fatal(response.ID())
	}
	errorKind(t, response.Err(), types.Canceled)
}

func TestTimeoutDoesNotRetryAppWork(t *testing.T) {
	var calls atomic.Int32
	started := make(chan struct{})
	app := &fakeApp{get: func(ctx context.Context) ([]session.ListEntry, error) {
		if calls.Add(1) == 1 {
			close(started)
		}
		<-ctx.Done()
		return nil, ctx.Err()
	}}
	peer := startPeer(t, app, 35*time.Second)
	peer.send(2, "get_sessions", nil)
	waitFor(t, started)
	errorKind(t, peer.read().Err(), types.Timeout)
	if calls.Load() != 1 {
		t.Fatalf("timed-out app calls = %d, want 1", calls.Load())
	}
}

func TestCanceledDeliveryKeepsCapacityUntilAppFinishes(t *testing.T) {
	release := make(chan struct{})
	defer close(release)
	started := make(chan struct{}, maxConcurrentRequests)
	var calls atomic.Int32
	app := &fakeApp{get: func(context.Context) ([]session.ListEntry, error) {
		calls.Add(1)
		started <- struct{}{}
		<-release
		return []session.ListEntry{}, nil
	}}
	peer := startPeer(t, app, 5*time.Second)
	for i := 0; i < maxConcurrentRequests; i++ {
		peer.send(i+2, "get_sessions", nil)
		waitFor(t, started)
		peer.cancel(i + 2)
		errorKind(t, peer.read().Err(), types.Canceled)
	}
	peer.send("capacity", "get_sessions", nil)
	errorKind(t, peer.read().Err(), types.Busy)
	if calls.Load() != maxConcurrentRequests {
		t.Fatal(calls.Load())
	}
}

func TestAcceptedSwitchesRemainInWireOrder(t *testing.T) {
	first := make(chan struct{})
	release := make(chan struct{})
	var switchOrderMutex sync.Mutex
	order := []string{}
	app := &fakeApp{switchSession: func(ctx context.Context, path, id string) (session.LoadedSnapshot, error) {
		if path == "first" {
			close(first)
			select {
			case <-release:
			case <-ctx.Done():
				return session.LoadedSnapshot{}, ctx.Err()
			}
		}
		switchOrderMutex.Lock()
		order = append(order, path)
		switchOrderMutex.Unlock()
		return session.LoadedSnapshot{SessionID: path}, nil
	}}
	peer := startPeer(t, app, 5*time.Second)
	peer.send(2, "switch_session", types.SwitchParams{Path: "first", CatalogID: "id"})
	waitFor(t, first)
	for i := 0; i < 5; i++ {
		peer.send(i+3, "switch_session", types.SwitchParams{Path: fmt.Sprint(i), CatalogID: "id"})
	}
	peer.cancel(4) // skip a queued switch, without releasing the next one early
	errorKind(t, peer.read().Err(), types.Canceled)
	close(release)
	for i := 0; i < 5; i++ {
		peer.readOK()
	}
	switchOrderMutex.Lock()
	defer switchOrderMutex.Unlock()
	if fmt.Sprint(order) != "[first 0 2 3 4]" {
		t.Fatal(order)
	}
}

type fakeSubscription struct {
	events chan session.Event
	done   chan struct{}
	once   sync.Once
}

func (subscription *fakeSubscription) Next(ctx context.Context) (session.Event, error) {
	select {
	case event := <-subscription.events:
		return event, nil
	case <-subscription.done:
		return session.Event{}, session.ErrSubscriptionClosed
	case <-ctx.Done():
		return session.Event{}, ctx.Err()
	}
}
func (subscription *fakeSubscription) Close() {
	subscription.once.Do(func() { close(subscription.done) })
}

func TestSnapshotRaceReplayRejectsOldEpochAndDuplicates(t *testing.T) {
	source := &fakeSubscription{events: make(chan session.Event, 16), done: make(chan struct{})}
	snapshotStarted := make(chan struct{})
	release := make(chan struct{})
	var subscribed atomic.Bool
	app := &fakeApp{
		subscribe: func() session.EventSubscription { subscribed.Store(true); return source },
		snapshot: func(ctx context.Context) (session.StateSnapshot, error) {
			if !subscribed.Load() {
				return session.StateSnapshot{}, errors.New("snapshot before subscribe")
			}
			close(snapshotStarted)
			select {
			case <-release:
				return session.StateSnapshot{Epoch: "current", Watermark: 3}, nil
			case <-ctx.Done():
				return session.StateSnapshot{}, ctx.Err()
			}
		},
		get: func(context.Context) ([]session.ListEntry, error) { return []session.ListEntry{}, nil },
	}
	peer := startPeer(t, app, 5*time.Second)
	peer.send(2, "subscribe_sessions", nil)
	waitFor(t, snapshotStarted)
	peer.send(3, "get_sessions", nil)
	if peer.readOK().ID() != transportID(3) {
		t.Fatal("snapshot blocked concurrent read")
	}
	for _, event := range []session.Event{{Epoch: "current", Sequence: 2}, {Epoch: "old", Sequence: 100}, {Epoch: "current", Sequence: 3}, {Epoch: "current", Sequence: 4}, {Epoch: "current", Sequence: 4}, {Epoch: "current", Sequence: 5}} {
		source.events <- event
	}
	close(release)
	initial := peer.update()
	if initial.Snapshot == nil || initial.Snapshot.Watermark != 3 {
		t.Fatal(initial)
	}
	for _, want := range []uint64{4, 5} {
		peer.send(int(want)+10, "next_session_event", subscriptionParams{initial.SubscriptionID})
		update := peer.update()
		if update.Event == nil || update.Event.Sequence != want || update.Event.Epoch != "current" {
			t.Fatal(update)
		}
	}
}

func syntheticService(t *testing.T) (*session.Service, string) {
	t.Helper()
	dir := t.TempDir()
	path := filepath.Join(dir, "filename.jsonl")
	fixture, err := os.ReadFile("../../../../testdata/fixtures/backend/transport/session.jsonl")
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, fixture, 0600); err != nil {
		t.Fatal(err)
	}
	return session.NewService(session.Catalog{Directories: []string{dir}}), path
}

func TestOverflowIsVisibleAndNextReadResynchronizes(t *testing.T) {
	app, path := syntheticService(t)
	peer := startPeer(t, app, 5*time.Second)
	peer.send(2, "subscribe_sessions", nil)
	initial := peer.update()
	for i := 0; i < session.EventBufferSize+1; i++ {
		peer.send(i+3, "switch_session", types.SwitchParams{Path: path, CatalogID: "catalog"})
		peer.readOK()
	}
	peer.send(100, "next_session_event", subscriptionParams{initial.SubscriptionID})
	errorKind(t, peer.read().Err(), types.ResyncRequired)
	peer.send(101, "next_session_event", subscriptionParams{initial.SubscriptionID})
	synced := peer.update()
	if synced.Snapshot == nil || synced.Snapshot.Watermark != session.EventBufferSize+1 || synced.Snapshot.Active.CatalogID != "catalog" || synced.Snapshot.Active.SessionID != "filename" {
		t.Fatal(synced)
	}
	peer.send(102, "switch_session", types.SwitchParams{Path: path, CatalogID: "catalog"})
	peer.readOK()
	peer.send(103, "next_session_event", subscriptionParams{initial.SubscriptionID})
	if update := peer.update(); update.Event == nil || update.Event.Sequence != session.EventBufferSize+2 {
		t.Fatal(update)
	}
}

func TestConnectionLossCancelsAppWithoutRetry(t *testing.T) {
	for _, malformed := range []bool{false, true} {
		t.Run(fmt.Sprint(malformed), func(t *testing.T) {
			started := make(chan struct{})
			ended := make(chan error, 1)
			var calls atomic.Int32
			app := &fakeApp{get: func(ctx context.Context) ([]session.ListEntry, error) {
				calls.Add(1)
				close(started)
				<-ctx.Done()
				ended <- context.Cause(ctx)
				return nil, ctx.Err()
			}}
			peer := startPeer(t, app, 5*time.Second)
			peer.send(2, "get_sessions", nil)
			waitFor(t, started)
			if malformed {
				if _, err := peer.stream.(rawFrames).WriteFrame(t.Context(), []byte(`{"jsonrpc":"2.0","id":true,"method":"get_sessions"}`)); err != nil {
					t.Fatal(err)
				}
			} else {
				peer.client.Close()
			}
			errorKind(t, waitFor(t, ended), types.Disconnected)
			if calls.Load() != 1 {
				t.Fatal(calls.Load())
			}
		})
	}
}

func TestFragmentedAndCoalescedUTF8ThroughServer(t *testing.T) {
	app := &fakeApp{get: func(context.Context) ([]session.ListEntry, error) {
		return []session.ListEntry{{Name: "玫瑰 🌹"}}, nil
	}}
	peer := startPeer(t, app, 5*time.Second)
	var batch bytes.Buffer
	stream := jsonrpc2.NewStream(readWriteCloser{Reader: bytes.NewReader(nil), Writer: &batch})
	for i := 2; i < 5; i++ {
		if _, err := stream.Write(t.Context(), jsonrpc2.NewCall(transportID(i), "get_sessions", nil)); err != nil {
			t.Fatal(err)
		}
	}
	writeDone := make(chan error, 1)
	go func() {
		data := batch.Bytes()
		for _, size := range []int{1, 7, 18} {
			if _, err := peer.client.Write(data[:size]); err != nil {
				writeDone <- err
				return
			}
			data = data[size:]
		}
		_, err := peer.client.Write(data)
		writeDone <- err
	}()
	seen := map[jsonrpc2.ID]bool{}
	for i := 0; i < 3; i++ {
		response := peer.readOK()
		var entries []session.ListEntry
		if err := json.Unmarshal(response.Result(), &entries); err != nil || len(entries) != 1 || entries[0].Name != "玫瑰 🌹" {
			t.Fatalf("%s %v", response.Result(), err)
		}
		seen[response.ID()] = true
	}
	if len(seen) != 3 {
		t.Fatal(seen)
	}
	if err := waitFor(t, writeDone); err != nil {
		t.Fatal(err)
	}
}

func TestServerRejectsOversizedFrameWithoutAppCall(t *testing.T) {
	var calls atomic.Int32
	app := &fakeApp{get: func(context.Context) ([]session.ListEntry, error) { calls.Add(1); return nil, nil }}
	peer := startPeer(t, app, 5*time.Second)
	if _, err := io.WriteString(peer.client, "Content-Length: 1073741825\r\n\r\n"); err != nil {
		t.Fatal(err)
	}
	if _, _, err := peer.stream.Read(t.Context()); err == nil {
		t.Fatal("oversized frame accepted")
	}
	if calls.Load() != 0 {
		t.Fatal(calls.Load())
	}
}

func TestReplacedSubscriptionCannotDeliverLateSnapshot(t *testing.T) {
	started, release := make(chan struct{}), make(chan struct{})
	var snapshots atomic.Int32
	app := &fakeApp{
		subscribe: func() session.EventSubscription {
			return &fakeSubscription{events: make(chan session.Event), done: make(chan struct{})}
		},
		snapshot: func(ctx context.Context) (session.StateSnapshot, error) {
			if snapshots.Add(1) == 1 {
				close(started)
				select {
				case <-release:
				case <-ctx.Done():
					return session.StateSnapshot{}, ctx.Err()
				}
			}
			return session.StateSnapshot{Epoch: "app"}, nil
		},
	}
	peer := startPeer(t, app, 5*time.Second)
	peer.send(2, "subscribe_sessions", nil)
	waitFor(t, started)
	peer.send(3, "subscribe_sessions", nil)
	if peer.update().SubscriptionID != "2" {
		t.Fatal("replacement not installed")
	}
	close(release)
	response := peer.read()
	if response.ID() != transportID(2) {
		t.Fatal(response.ID())
	}
	errorKind(t, response.Err(), types.ResyncRequired)
}

func TestSequenceGapResubscribesBeforeFreshSnapshot(t *testing.T) {
	sources := []*fakeSubscription{
		{events: make(chan session.Event, 2), done: make(chan struct{})},
		{events: make(chan session.Event, 2), done: make(chan struct{})},
	}
	var subscriptions, snapshots atomic.Int32
	app := &fakeApp{
		subscribe: func() session.EventSubscription { return sources[subscriptions.Add(1)-1] },
		snapshot: func(context.Context) (session.StateSnapshot, error) {
			count := snapshots.Add(1)
			if count != subscriptions.Load() {
				return session.StateSnapshot{}, errors.New("snapshot before fresh subscription")
			}
			return session.StateSnapshot{Epoch: fmt.Sprint(count), Watermark: uint64(count)}, nil
		},
	}
	peer := startPeer(t, app, 5*time.Second)
	peer.send(2, "subscribe_sessions", nil)
	initial := peer.update()
	sources[0].events <- session.Event{Epoch: "1", Sequence: 3}
	peer.send(3, "next_session_event", subscriptionParams{initial.SubscriptionID})
	errorKind(t, peer.read().Err(), types.ResyncRequired)
	peer.send(4, "next_session_event", subscriptionParams{initial.SubscriptionID})
	fresh := peer.update()
	if fresh.Snapshot == nil || fresh.Snapshot.Epoch != "2" || fresh.Snapshot.Watermark != 2 {
		t.Fatal(fresh)
	}
	select {
	case <-sources[0].done:
	default:
		t.Fatal("invalidated source was retained")
	}
	sources[1].events <- session.Event{Epoch: "2", Sequence: 3}
	peer.send(5, "next_session_event", subscriptionParams{initial.SubscriptionID})
	if update := peer.update(); update.Event == nil || update.Event.Epoch != "2" || update.Event.Sequence != 3 {
		t.Fatal(update)
	}
}

func TestUnsubscribeUnblocksDeliveryAndRejectsStaleIdentity(t *testing.T) {
	app, _ := syntheticService(t)
	peer := startPeer(t, app, 5*time.Second)
	peer.send(2, "subscribe_sessions", nil)
	initial := peer.update()
	peer.send(3, "next_session_event", subscriptionParams{initial.SubscriptionID})
	peer.send(4, "unsubscribe_sessions", subscriptionParams{initial.SubscriptionID})
	for i := 0; i < 2; i++ {
		response := peer.read()
		if response.ID() == transportID(3) {
			errorKind(t, response.Err(), types.ResyncRequired)
		} else if response.ID() != transportID(4) || response.Err() != nil {
			t.Fatal(response)
		}
	}
	peer.send(5, "next_session_event", subscriptionParams{initial.SubscriptionID})
	errorKind(t, peer.read().Err(), types.ResyncRequired)
	peer.send(6, "subscribe_sessions", nil)
	if peer.update().SubscriptionID == initial.SubscriptionID {
		t.Fatal("subscription identity reused")
	}
}

type observedSubscription struct {
	session.EventSubscription
	consumed chan struct{}
}

func (subscription observedSubscription) Next(ctx context.Context) (session.Event, error) {
	event, err := subscription.EventSubscription.Next(ctx)
	if err == nil {
		subscription.consumed <- struct{}{}
	}
	return event, err
}

func TestSlowStdoutReaderProducesVisibleOverflowAndRecovery(t *testing.T) {
	service, path := syntheticService(t)
	consumed := make(chan struct{}, 1)
	app := &fakeApp{Service: service, subscribe: func() session.EventSubscription { return observedSubscription{service.Subscribe(), consumed} }}
	peer := startPeer(t, app, 5*time.Second)
	peer.send(2, "subscribe_sessions", nil)
	initial := peer.update()
	peer.send(3, "next_session_event", subscriptionParams{initial.SubscriptionID})
	if _, err := service.Switch(t.Context(), path, "catalog"); err != nil {
		t.Fatal(err)
	}
	waitFor(t, consumed)
	// Leave the response unread while app state advances: net.Pipe's stdout
	// write blocks until this client resumes reading.
	for i := 0; i <= session.EventBufferSize; i++ {
		if _, err := service.Switch(t.Context(), path, "catalog"); err != nil {
			t.Fatal(err)
		}
	}
	if update := peer.update(); update.Event == nil || update.Event.Sequence != 1 {
		t.Fatal(update)
	}
	peer.send(4, "next_session_event", subscriptionParams{initial.SubscriptionID})
	errorKind(t, peer.read().Err(), types.ResyncRequired)
	peer.send(5, "next_session_event", subscriptionParams{initial.SubscriptionID})
	if update := peer.update(); update.Snapshot == nil || update.Snapshot.Watermark != session.EventBufferSize+2 {
		t.Fatal(update)
	}
}

type waitingSubscription struct {
	session.EventSubscription
	started chan struct{}
}

func (subscription waitingSubscription) Next(ctx context.Context) (session.Event, error) {
	close(subscription.started)
	return subscription.EventSubscription.Next(ctx)
}

func TestCanceledEventWaitResynchronizesAndHasOneConsumer(t *testing.T) {
	service, _ := syntheticService(t)
	started := make(chan struct{})
	var subscriptions atomic.Int32
	app := &fakeApp{Service: service, subscribe: func() session.EventSubscription {
		source := service.Subscribe()
		if subscriptions.Add(1) == 1 {
			return waitingSubscription{source, started}
		}
		return source
	}}
	peer := startPeer(t, app, 5*time.Second)
	peer.send(2, "subscribe_sessions", nil)
	initial := peer.update()
	peer.send(3, "next_session_event", subscriptionParams{SubscriptionID: initial.SubscriptionID})
	waitFor(t, started)
	peer.send(4, "next_session_event", subscriptionParams{SubscriptionID: initial.SubscriptionID})
	busy := peer.read()
	if busy.ID() != transportID(4) {
		t.Fatalf("concurrent poll response id = %v, want 4", busy.ID())
	}
	errorKind(t, busy.Err(), types.Busy)
	peer.cancel(3)
	canceled := peer.read()
	if canceled.ID() != transportID(3) {
		t.Fatalf("canceled poll response id = %v, want 3", canceled.ID())
	}
	errorKind(t, canceled.Err(), types.Canceled)
	var resynchronized sessionUpdate
	for attempt := 0; attempt < 10; attempt++ {
		peer.send(attempt+5, "next_session_event", subscriptionParams{SubscriptionID: initial.SubscriptionID})
		response := peer.read()
		if response.Err() != nil {
			errorKind(t, response.Err(), types.Busy)
			time.Sleep(10 * time.Millisecond)
			continue
		}
		if err := json.Unmarshal(response.Result(), &resynchronized); err != nil {
			t.Fatal(err)
		}
		break
	}
	if resynchronized.Snapshot == nil || subscriptions.Load() != 2 {
		t.Fatalf("resynchronized update=%+v subscriptions=%d", resynchronized, subscriptions.Load())
	}
}
