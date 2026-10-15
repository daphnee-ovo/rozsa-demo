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

func (s *Service) GetSessions(ctx context.Context) ([]ListEntry, error) {
	if err := ctx.Err(); err != nil {
		return nil, err
	}
	return s.catalog.GetSessions()
}

func (s *Service) Switch(ctx context.Context, path, catalogID string) (LoadedSnapshot, error) {
	s.loadedMu.Lock()
	defer s.loadedMu.Unlock()
	if err := ctx.Err(); err != nil {
		return LoadedSnapshot{}, err
	}
	active, err := s.loaded.Switch(path, catalogID)
	if err != nil {
		return LoadedSnapshot{}, err
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	s.state.Active = &active
	s.state.Watermark++
	event := Event{Epoch: s.state.Epoch, Sequence: s.state.Watermark, Active: active}
	for subscriber := range s.subscribers {
		subscriber.push(event)
	}
	return active, nil
}

func (s *Service) Snapshot(ctx context.Context) (StateSnapshot, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if err := ctx.Err(); err != nil {
		return StateSnapshot{}, err
	}
	return s.state, nil
}

func (s *Service) Subscribe() EventSubscription {
	s.mu.Lock()
	defer s.mu.Unlock()
	subscriber := &subscription{owner: s, events: make(chan Event, EventBufferSize), done: make(chan struct{})}
	s.subscribers[subscriber] = struct{}{}
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
func (s *subscription) push(event Event) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.err != nil {
		return
	}
	select {
	case s.events <- event:
	default:
		s.err = ErrOverflow
		close(s.done)
	}
}

func (s *subscription) Next(ctx context.Context) (Event, error) {
	select {
	case <-ctx.Done():
		return Event{}, ctx.Err()
	case <-s.done:
	case event := <-s.events:
		s.mu.Lock()
		err := s.err
		s.mu.Unlock()
		if err == nil {
			return event, nil
		}
		return Event{}, err
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	return Event{}, s.err
}

func (s *subscription) Close() {
	s.owner.mu.Lock()
	defer s.owner.mu.Unlock()
	delete(s.owner.subscribers, s)
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.err == nil {
		s.err = ErrSubscriptionClosed
		close(s.done)
	}
}
