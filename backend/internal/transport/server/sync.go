package server

import (
	"context"
	"fmt"
	"sync"

	"rozsa/backend/internal/app/session"
	"rozsa/backend/internal/transport/types"
)

type subscriptionParams struct {
	SubscriptionID string `json:"subscriptionId"`
}

// Pull delivery bounds memory without an independent notification writer.
// First receive the watermarked snapshot, then poll for events. There is one
// subscription per connection/renderer, not a general product event bus.
type sessionUpdate struct {
	SubscriptionID string                 `json:"subscriptionId"`
	Snapshot       *session.StateSnapshot `json:"snapshot,omitempty"`
	Event          *session.Event         `json:"event,omitempty"`
}

type sessionSync struct {
	app     sessionApp
	mu      sync.Mutex
	serial  uint64
	current *sessionCursor
}

func (s *sessionSync) subscribe() *sessionCursor {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.current != nil {
		s.current.close()
	}
	s.serial++
	cursor := &sessionCursor{app: s.app, id: fmt.Sprint(s.serial), gate: make(chan struct{}, 1)}
	// Subscribe before snapshot, in wire order. Racing events remain buffered.
	cursor.source = s.app.Subscribe()
	s.current = cursor
	return cursor
}

func (s *sessionSync) next(ctx context.Context, id string) (sessionUpdate, error) {
	s.mu.Lock()
	cursor := s.current
	s.mu.Unlock()
	if cursor == nil || cursor.id != id {
		return sessionUpdate{}, types.Error(types.ResyncRequired, "Unknown or replaced subscription")
	}
	return cursor.next(ctx)
}

func (s *sessionSync) unsubscribe(id string) (any, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.current == nil || s.current.id != id {
		return nil, types.Error(types.ResyncRequired, "Unknown or replaced subscription")
	}
	s.current.close()
	s.current = nil
	return true, nil
}

func (s *sessionSync) close() {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.current != nil {
		s.current.close()
		s.current = nil
	}
}

type sessionCursor struct {
	app  sessionApp
	id   string
	gate chan struct{}
	// Separate from the gate so close never waits for a blocked Next. Delivery
	// fields (epoch/watermark/needsSnapshot) belong to the gate holder.
	mu            sync.Mutex
	source        session.EventSubscription
	closed        bool
	epoch         string
	watermark     uint64
	needsSnapshot bool
}

func (s *sessionCursor) close() {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.closed = true
	if s.source != nil {
		s.source.Close()
	}
}

func (s *sessionCursor) snapshot(ctx context.Context) (sessionUpdate, error) {
	select {
	case s.gate <- struct{}{}:
		defer func() { <-s.gate }()
	default:
		return sessionUpdate{}, types.Error(types.Busy, "Subscription has an outstanding delivery")
	}
	return s.takeSnapshot(ctx, false)
}

func (s *sessionCursor) takeSnapshot(ctx context.Context, resubscribe bool) (sessionUpdate, error) {
	s.mu.Lock()
	if s.closed {
		s.mu.Unlock()
		return sessionUpdate{}, types.Error(types.ResyncRequired, "Subscription closed")
	}
	if resubscribe {
		s.source.Close()
		s.source = s.app.Subscribe()
	}
	s.mu.Unlock()
	snapshot, err := s.app.Snapshot(ctx)
	if err != nil {
		s.needsSnapshot = true
		return sessionUpdate{}, requestError(ctx, types.Error(types.Session, err.Error()))
	}
	s.mu.Lock()
	closed := s.closed
	s.mu.Unlock()
	if closed {
		return sessionUpdate{}, types.Error(types.ResyncRequired, "Subscription replaced during snapshot")
	}
	if ctx.Err() != nil {
		s.needsSnapshot = true
		return sessionUpdate{}, requestError(ctx, ctx.Err())
	}
	if snapshot.Epoch == "" {
		s.needsSnapshot = true
		return sessionUpdate{}, types.Error(types.ResyncRequired, "Snapshot has no epoch")
	}
	s.epoch, s.watermark, s.needsSnapshot = snapshot.Epoch, snapshot.Watermark, false
	return sessionUpdate{SubscriptionID: s.id, Snapshot: &snapshot}, nil
}

func (s *sessionCursor) next(ctx context.Context) (sessionUpdate, error) {
	select {
	case s.gate <- struct{}{}:
		defer func() { <-s.gate }()
	default:
		return sessionUpdate{}, types.Error(types.Busy, "Subscription has an outstanding delivery")
	}
	if s.needsSnapshot {
		return s.takeSnapshot(ctx, true)
	}
	if s.epoch == "" {
		return sessionUpdate{}, types.Error(types.Busy, "Initial snapshot is not ready")
	}
	for {
		s.mu.Lock()
		source, closed := s.source, s.closed
		s.mu.Unlock()
		if closed {
			return sessionUpdate{}, types.Error(types.ResyncRequired, "Subscription closed")
		}
		event, err := source.Next(ctx)
		if err != nil {
			s.needsSnapshot = true
			return sessionUpdate{}, requestError(ctx, types.Error(types.ResyncRequired, err.Error()))
		}
		if ctx.Err() != nil {
			s.needsSnapshot = true
			return sessionUpdate{}, requestError(ctx, ctx.Err())
		}
		if event.Epoch != s.epoch || event.Sequence <= s.watermark {
			continue
		}
		if event.Sequence != s.watermark+1 {
			s.needsSnapshot = true
			return sessionUpdate{}, types.Error(types.ResyncRequired, "Session event sequence gap")
		}
		s.watermark = event.Sequence
		return sessionUpdate{SubscriptionID: s.id, Event: &event}, nil
	}
}
