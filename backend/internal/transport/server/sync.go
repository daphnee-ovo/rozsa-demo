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

func (syncer *sessionSync) subscribe() *sessionCursor {
	syncer.mu.Lock()
	defer syncer.mu.Unlock()
	if syncer.current != nil {
		syncer.current.close()
	}
	syncer.serial++
	cursor := &sessionCursor{app: syncer.app, id: fmt.Sprint(syncer.serial), gate: make(chan struct{}, 1)}
	// Subscribe before snapshot, in wire order. Racing events remain buffered.
	cursor.source = syncer.app.Subscribe()
	syncer.current = cursor
	return cursor
}

func (syncer *sessionSync) next(ctx context.Context, id string) (sessionUpdate, error) {
	syncer.mu.Lock()
	cursor := syncer.current
	syncer.mu.Unlock()
	if cursor == nil || cursor.id != id {
		return sessionUpdate{}, types.Error(types.ResyncRequired, "Unknown or replaced subscription")
	}
	return cursor.next(ctx)
}

func (syncer *sessionSync) unsubscribe(id string) (any, error) {
	syncer.mu.Lock()
	defer syncer.mu.Unlock()
	if syncer.current == nil || syncer.current.id != id {
		return nil, types.Error(types.ResyncRequired, "Unknown or replaced subscription")
	}
	syncer.current.close()
	syncer.current = nil
	return true, nil
}

func (syncer *sessionSync) close() {
	syncer.mu.Lock()
	defer syncer.mu.Unlock()
	if syncer.current != nil {
		syncer.current.close()
		syncer.current = nil
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

func (cursor *sessionCursor) close() {
	cursor.mu.Lock()
	defer cursor.mu.Unlock()
	cursor.closed = true
	if cursor.source != nil {
		cursor.source.Close()
	}
}

func (cursor *sessionCursor) snapshot(ctx context.Context) (sessionUpdate, error) {
	select {
	case cursor.gate <- struct{}{}:
		defer func() { <-cursor.gate }()
	default:
		return sessionUpdate{}, types.Error(types.Busy, "Subscription has an outstanding delivery")
	}
	return cursor.takeSnapshot(ctx, false)
}

func (cursor *sessionCursor) takeSnapshot(ctx context.Context, resubscribe bool) (sessionUpdate, error) {
	cursor.mu.Lock()
	if cursor.closed {
		cursor.mu.Unlock()
		return sessionUpdate{}, types.Error(types.ResyncRequired, "Subscription closed")
	}
	if resubscribe {
		cursor.source.Close()
		cursor.source = cursor.app.Subscribe()
	}
	cursor.mu.Unlock()
	snapshot, err := cursor.app.Snapshot(ctx)
	if err != nil {
		cursor.needsSnapshot = true
		return sessionUpdate{}, requestError(ctx, types.Error(types.Session, err.Error()))
	}
	cursor.mu.Lock()
	closed := cursor.closed
	cursor.mu.Unlock()
	if closed {
		return sessionUpdate{}, types.Error(types.ResyncRequired, "Subscription replaced during snapshot")
	}
	if ctx.Err() != nil {
		cursor.needsSnapshot = true
		return sessionUpdate{}, requestError(ctx, ctx.Err())
	}
	if snapshot.Epoch == "" {
		cursor.needsSnapshot = true
		return sessionUpdate{}, types.Error(types.ResyncRequired, "Snapshot has no epoch")
	}
	cursor.epoch, cursor.watermark, cursor.needsSnapshot = snapshot.Epoch, snapshot.Watermark, false
	return sessionUpdate{SubscriptionID: cursor.id, Snapshot: &snapshot}, nil
}

func (cursor *sessionCursor) next(ctx context.Context) (sessionUpdate, error) {
	select {
	case cursor.gate <- struct{}{}:
		defer func() { <-cursor.gate }()
	default:
		return sessionUpdate{}, types.Error(types.Busy, "Subscription has an outstanding delivery")
	}
	if cursor.needsSnapshot {
		return cursor.takeSnapshot(ctx, true)
	}
	if cursor.epoch == "" {
		return sessionUpdate{}, types.Error(types.Busy, "Initial snapshot is not ready")
	}
	for {
		cursor.mu.Lock()
		source, closed := cursor.source, cursor.closed
		cursor.mu.Unlock()
		if closed {
			return sessionUpdate{}, types.Error(types.ResyncRequired, "Subscription closed")
		}
		event, err := source.Next(ctx)
		if err != nil {
			cursor.needsSnapshot = true
			return sessionUpdate{}, requestError(ctx, types.Error(types.ResyncRequired, err.Error()))
		}
		if ctx.Err() != nil {
			cursor.needsSnapshot = true
			return sessionUpdate{}, requestError(ctx, ctx.Err())
		}
		if event.Epoch != cursor.epoch || event.Sequence <= cursor.watermark {
			continue
		}
		if event.Sequence != cursor.watermark+1 {
			cursor.needsSnapshot = true
			return sessionUpdate{}, types.Error(types.ResyncRequired, "Session event sequence gap")
		}
		cursor.watermark = event.Sequence
		return sessionUpdate{SubscriptionID: cursor.id, Event: &event}, nil
	}
}
