package server

import (
	"context"
	"fmt"
	"sync"

	"rozsa/backend/internal/transport/types"
)

type messageSync struct {
	app           MessageApplication
	mu            sync.Mutex
	serial        uint64
	subscriptions map[string]*messageCursor
}

type messageCursor struct {
	operationID string
	source      MessageEventSubscription
	gate        chan struct{}
	mu          sync.Mutex
	closed      bool
	last        uint64
}

func (syncer *messageSync) handle(ctx context.Context, method types.Method, start types.StartMessageParams, operation types.MessageOperationParams, subscription types.MessageSubscriptionParams) (any, error) {
	if syncer.app == nil {
		return nil, types.Error(types.MethodNotFound, "Message operations are unavailable")
	}
	switch method {
	case types.StartMessage:
		id, err := syncer.app.StartMessage(ctx, start.Text)
		return messageOperationResult(id, err)
	case types.ContinueMessage:
		id, err := syncer.app.ContinueMessage(ctx)
		return messageOperationResult(id, err)
	case types.SubscribeMessage:
		source, err := syncer.app.SubscribeMessageEvents(ctx, operation.OperationID)
		if err != nil {
			return nil, types.Error(types.Message, err.Error())
		}
		if source == nil {
			return nil, types.Error(types.Message, "Application returned an empty message subscription")
		}
		syncer.mu.Lock()
		syncer.serial++
		id := fmt.Sprintf("message-%d", syncer.serial)
		if syncer.subscriptions == nil {
			syncer.subscriptions = make(map[string]*messageCursor)
		}
		syncer.subscriptions[id] = &messageCursor{operationID: operation.OperationID, source: source, gate: make(chan struct{}, 1)}
		syncer.mu.Unlock()
		return types.MessageSubscription{SubscriptionID: id, OperationID: operation.OperationID}, nil
	case types.NextMessageEvent:
		return syncer.next(ctx, subscription.SubscriptionID)
	case types.CancelMessage:
		if err := syncer.app.CancelMessage(ctx, operation.OperationID); err != nil {
			return nil, types.Error(types.Message, err.Error())
		}
		return true, nil
	default:
		return nil, types.Error(types.MethodNotFound, "Method not found")
	}
}

func messageOperationResult(id string, err error) (any, error) {
	if err != nil {
		return nil, types.Error(types.Message, err.Error())
	}
	if id == "" {
		return nil, types.Error(types.Message, "Application returned an empty operation ID")
	}
	return types.MessageOperation{OperationID: id}, nil
}

func (syncer *messageSync) next(ctx context.Context, id string) (types.MessageEventResult, error) {
	syncer.mu.Lock()
	cursor := syncer.subscriptions[id]
	syncer.mu.Unlock()
	if cursor == nil {
		return types.MessageEventResult{}, types.Error(types.ResyncRequired, "Unknown message subscription")
	}
	select {
	case cursor.gate <- struct{}{}:
		defer func() { <-cursor.gate }()
	default:
		return types.MessageEventResult{}, types.Error(types.Busy, "Message subscription has an outstanding receive")
	}
	cursor.mu.Lock()
	closed := cursor.closed
	cursor.mu.Unlock()
	if closed {
		return types.MessageEventResult{}, types.Error(types.ResyncRequired, "Message subscription is closed")
	}
	event, err := cursor.source.Next(ctx)
	if err != nil {
		return types.MessageEventResult{}, messageReceiveError(ctx, err)
	}
	if !validMessageEvent(event) {
		return types.MessageEventResult{}, types.Error(types.Message, "Application returned an invalid message event")
	}
	if event.Sequence != cursor.last+1 {
		return types.MessageEventResult{}, types.Error(types.ResyncRequired, fmt.Sprintf("Message event sequence gap: expected %d, got %d", cursor.last+1, event.Sequence))
	}
	cursor.last = event.Sequence
	return types.MessageEventResult{SubscriptionID: id, OperationID: cursor.operationID, Event: event}, nil
}

func validMessageEvent(event types.MessageEvent) bool {
	switch event.Kind {
	case types.MessageTextDelta:
		return event.Sequence > 0 && event.Delta != nil && event.Error == nil
	case types.MessageCompleted, types.MessageCanceled:
		return event.Sequence > 0 && event.Delta == nil && event.Error == nil
	case types.MessageFailed:
		return event.Sequence > 0 && event.Delta == nil && event.Error != nil
	default:
		return false
	}
}

func messageReceiveError(ctx context.Context, err error) error {
	if ctx.Err() != nil {
		return requestError(ctx, err)
	}
	return types.Error(types.Message, err.Error())
}

func (syncer *messageSync) unsubscribe(id string) (any, error) {
	syncer.mu.Lock()
	cursor := syncer.subscriptions[id]
	if cursor != nil {
		delete(syncer.subscriptions, id)
	}
	syncer.mu.Unlock()
	if cursor == nil {
		return nil, types.Error(types.ResyncRequired, "Unknown message subscription")
	}
	cursor.close()
	return true, nil
}

func (cursor *messageCursor) close() {
	cursor.mu.Lock()
	defer cursor.mu.Unlock()
	if !cursor.closed {
		cursor.closed = true
		cursor.source.Close()
	}
}

func (syncer *messageSync) close() {
	syncer.mu.Lock()
	cursors := make([]*messageCursor, 0, len(syncer.subscriptions))
	for _, cursor := range syncer.subscriptions {
		cursors = append(cursors, cursor)
	}
	syncer.subscriptions = make(map[string]*messageCursor)
	syncer.mu.Unlock()
	for _, cursor := range cursors {
		cursor.close()
	}
}
