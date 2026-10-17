package runtime

import (
	"context"
	"crypto/rand"
	"errors"
	"sync"
)

type MessageEventKind string

const (
	MessageTextDelta MessageEventKind = "text_delta"
	MessageCompleted MessageEventKind = "completed"
	MessageFailed    MessageEventKind = "error"
	MessageCanceled  MessageEventKind = "canceled"
)

type MessageEvent struct {
	Sequence uint64
	Kind     MessageEventKind
	Delta    *string
	Error    *string
}

type MessageEventSubscription interface {
	Next(context.Context) (MessageEvent, error)
	Close()
}

type messageOperations struct {
	mu     sync.Mutex
	byID   map[string]*messageOperation
	order  []string
	active string
}

type messageOperation struct {
	mu          sync.Mutex
	id          string
	cancel      context.CancelFunc
	events      []MessageEvent
	changed     chan struct{}
	terminal    bool
	subscribers int
}

type messageEventSubscription struct {
	operation    *messageOperation
	index        int
	mu           sync.Mutex
	closed       bool
	closedSignal chan struct{}
}

func newMessageOperations() *messageOperations {
	return &messageOperations{byID: make(map[string]*messageOperation)}
}

func (runtime *Runtime) StartMessage(_ context.Context, text string) (string, error) {
	if text == "" {
		return "", errors.New("prompt must not be empty")
	}
	return runtime.startMessage(&text)
}

func (runtime *Runtime) ContinueMessage(_ context.Context) (string, error) {
	return runtime.startMessage(nil)
}

func (runtime *Runtime) startMessage(prompt *string) (string, error) {
	operationID := rand.Text()
	manager, err := runtime.beginRun()
	if err != nil {
		return "", err
	}
	operationContext, cancel := context.WithCancel(context.Background())
	operation := &messageOperation{id: operationID, cancel: cancel, changed: make(chan struct{})}
	runtime.messages.add(operation)
	go func() {
		err := runtime.runReserved(operationContext, prompt, manager, operation.appendText)
		operation.finish(err)
		runtime.messages.finish(operationID)
	}()
	return operationID, nil
}

func (runtime *Runtime) SubscribeMessageEvents(ctx context.Context, operationID string) (MessageEventSubscription, error) {
	if err := ctx.Err(); err != nil {
		return nil, err
	}
	operation, ok := runtime.messages.get(operationID)
	if !ok {
		return nil, errors.New("message operation is unavailable")
	}
	operation.mu.Lock()
	operation.subscribers++
	operation.mu.Unlock()
	return &messageEventSubscription{operation: operation, closedSignal: make(chan struct{})}, nil
}

func (runtime *Runtime) CancelMessage(ctx context.Context, operationID string) error {
	if err := ctx.Err(); err != nil {
		return err
	}
	operation, ok := runtime.messages.get(operationID)
	if !ok {
		return errors.New("message operation is unavailable")
	}
	operation.mu.Lock()
	defer operation.mu.Unlock()
	if operation.terminal {
		return errors.New("message operation has already completed")
	}
	operation.cancel()
	return nil
}

func (operations *messageOperations) add(operation *messageOperation) {
	operations.mu.Lock()
	defer operations.mu.Unlock()
	if operations.byID == nil {
		operations.byID = make(map[string]*messageOperation)
	}
	operations.byID[operation.id] = operation
	operations.order = append(operations.order, operation.id)
	operations.active = operation.id
	operations.pruneLocked()
}

func (operations *messageOperations) get(id string) (*messageOperation, bool) {
	operations.mu.Lock()
	defer operations.mu.Unlock()
	operation, ok := operations.byID[id]
	return operation, ok
}

func (operations *messageOperations) hasActive() bool {
	operations.mu.Lock()
	defer operations.mu.Unlock()
	return operations.active != ""
}

func (operations *messageOperations) finish(id string) {
	operations.mu.Lock()
	defer operations.mu.Unlock()
	if operations.active == id {
		operations.active = ""
	}
	operations.pruneLocked()
}

func (operations *messageOperations) pruneLocked() {
	for len(operations.order) > 64 {
		oldest := operations.order[0]
		operation := operations.byID[oldest]
		if operation == nil {
			operations.order = operations.order[1:]
			continue
		}
		operation.mu.Lock()
		removable := operation.terminal && operation.subscribers == 0
		operation.mu.Unlock()
		if !removable {
			return
		}
		delete(operations.byID, oldest)
		operations.order = operations.order[1:]
	}
}

func (operation *messageOperation) appendText(delta string) {
	operation.mu.Lock()
	defer operation.mu.Unlock()
	if operation.terminal {
		return
	}
	operation.appendLocked(MessageEvent{Kind: MessageTextDelta, Delta: &delta})
}

func (operation *messageOperation) finish(err error) {
	operation.mu.Lock()
	defer operation.mu.Unlock()
	if operation.terminal {
		return
	}
	if errors.Is(err, context.Canceled) {
		operation.appendLocked(MessageEvent{Kind: MessageCanceled})
	} else if err != nil {
		message := err.Error()
		operation.appendLocked(MessageEvent{Kind: MessageFailed, Error: &message})
	} else {
		operation.appendLocked(MessageEvent{Kind: MessageCompleted})
	}
	operation.terminal = true
	operation.cancel()
}

func (operation *messageOperation) appendLocked(event MessageEvent) {
	event.Sequence = uint64(len(operation.events) + 1)
	operation.events = append(operation.events, event)
	close(operation.changed)
	operation.changed = make(chan struct{})
}

func (subscription *messageEventSubscription) Next(ctx context.Context) (MessageEvent, error) {
	for {
		subscription.mu.Lock()
		closed := subscription.closed
		subscription.mu.Unlock()
		if closed {
			return MessageEvent{}, errors.New("message event subscription is closed")
		}
		operation := subscription.operation
		operation.mu.Lock()
		if subscription.index < len(operation.events) {
			event := operation.events[subscription.index]
			subscription.index++
			operation.mu.Unlock()
			return event, nil
		}
		if operation.terminal {
			operation.mu.Unlock()
			return MessageEvent{}, errors.New("message operation has no more events")
		}
		changed := operation.changed
		operation.mu.Unlock()
		select {
		case <-ctx.Done():
			return MessageEvent{}, ctx.Err()
		case <-changed:
		case <-subscription.closedSignal:
			return MessageEvent{}, errors.New("message event subscription is closed")
		}
	}
}

func (subscription *messageEventSubscription) Close() {
	subscription.mu.Lock()
	if subscription.closed {
		subscription.mu.Unlock()
		return
	}
	subscription.closed = true
	close(subscription.closedSignal)
	subscription.mu.Unlock()
	operation := subscription.operation
	operation.mu.Lock()
	operation.subscribers--
	operation.mu.Unlock()
}
