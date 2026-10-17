package session

import (
	"context"
	"crypto/rand"
	"errors"
	"sync"
)

// Session snapshots and events belong to the app. The transport only orders
// their delivery; it never reconstructs product state from events.
type StateSnapshot struct {
	Epoch     string          `json:"epoch"`
	Watermark uint64          `json:"watermark"`
	Active    *LoadedSnapshot `json:"active"`
}

type Event struct {
	Epoch    string         `json:"epoch"`
	Sequence uint64         `json:"sequence"`
	Active   LoadedSnapshot `json:"active"`
}

var (
	ErrOverflow           = errors.New("session event buffer overflow; snapshot resynchronization required")
	ErrSubscriptionClosed = errors.New("session event subscription closed")
)

type EventSubscription interface {
	Next(context.Context) (Event, error)
	Close()
}

// ApplicationService is the typed session boundary consumed by transport.
// Implementations own catalog reads, loaded-session state and event revisions.
type ApplicationService interface {
	GetSessions(context.Context) ([]ListEntry, error)
	Switch(context.Context, string, string) (LoadedSnapshot, error)
	Subscribe() EventSubscription
	Snapshot(context.Context) (StateSnapshot, error)
}

// A bounded inbox is sufficient for the current Loaded-session slice. Overflow
// invalidates the whole inbox, including already queued events; it is never a
// successful delivery or a silent selective drop of an event category.
const EventBufferSize = 32

type Service struct {
	mu          sync.Mutex
	loadedMu    sync.Mutex
	catalog     Catalog
	loaded      LoadedSessions
	state       StateSnapshot
	subscribers map[*subscription]struct{}
}

func NewService(catalog Catalog) *Service {
	return &Service{catalog: catalog, loaded: LoadedSessions{Catalog: catalog},
		state: StateSnapshot{Epoch: rand.Text()}, subscribers: make(map[*subscription]struct{})}
}

func (service *Service) GetSessions(ctx context.Context) ([]ListEntry, error) {
	if err := ctx.Err(); err != nil {
		return nil, err
	}
	return service.catalog.GetSessions()
}

func (service *Service) Switch(ctx context.Context, path, catalogID string) (LoadedSnapshot, error) {
	service.loadedMu.Lock()
	defer service.loadedMu.Unlock()
	if err := ctx.Err(); err != nil {
		return LoadedSnapshot{}, err
	}
	active, err := service.loaded.Switch(path, catalogID)
	if err != nil {
		return LoadedSnapshot{}, err
	}
	service.publish(active)
	return active, nil
}

// Refresh reloads a selected tab after the runtime writes messages to its
// session manager, then publishes the new product-owned snapshot.
func (service *Service) Refresh(ctx context.Context, path, catalogID string) (LoadedSnapshot, error) {
	service.loadedMu.Lock()
	defer service.loadedMu.Unlock()
	if err := ctx.Err(); err != nil {
		return LoadedSnapshot{}, err
	}
	active, err := service.loaded.Refresh(path, catalogID)
	if err != nil {
		return LoadedSnapshot{}, err
	}
	service.publish(active)
	return active, nil
}

func (service *Service) publish(active LoadedSnapshot) {
	service.mu.Lock()
	defer service.mu.Unlock()
	service.state.Active = &active
	service.state.Watermark++
	event := Event{Epoch: service.state.Epoch, Sequence: service.state.Watermark, Active: active}
	for subscriber := range service.subscribers {
		subscriber.push(event)
	}
}

func (service *Service) Snapshot(ctx context.Context) (StateSnapshot, error) {
	service.mu.Lock()
	defer service.mu.Unlock()
	if err := ctx.Err(); err != nil {
		return StateSnapshot{}, err
	}
	return service.state, nil
}

func (service *Service) Subscribe() EventSubscription {
	service.mu.Lock()
	defer service.mu.Unlock()
	subscriber := &subscription{owner: service, events: make(chan Event, EventBufferSize), done: make(chan struct{})}
	service.subscribers[subscriber] = struct{}{}
	return subscriber
}

type subscription struct {
	owner  *Service
	mu     sync.Mutex
	events chan Event
	done   chan struct{}
	err    error
}

// Called under Service.mu. Close follows the same lock order.
func (subscription *subscription) push(event Event) {
	subscription.mu.Lock()
	defer subscription.mu.Unlock()
	if subscription.err != nil {
		return
	}
	select {
	case subscription.events <- event:
	default:
		subscription.err = ErrOverflow
		close(subscription.done)
	}
}

func (subscription *subscription) Next(ctx context.Context) (Event, error) {
	select {
	case <-ctx.Done():
		return Event{}, ctx.Err()
	case <-subscription.done:
	case event := <-subscription.events:
		subscription.mu.Lock()
		err := subscription.err
		subscription.mu.Unlock()
		if err == nil {
			return event, nil
		}
		return Event{}, err
	}
	subscription.mu.Lock()
	defer subscription.mu.Unlock()
	return Event{}, subscription.err
}

func (subscription *subscription) Close() {
	subscription.owner.mu.Lock()
	defer subscription.owner.mu.Unlock()
	delete(subscription.owner.subscribers, subscription)
	subscription.mu.Lock()
	defer subscription.mu.Unlock()
	if subscription.err == nil {
		subscription.err = ErrSubscriptionClosed
		close(subscription.done)
	}
}
