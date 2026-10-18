package runtime

import (
	"context"
	"crypto/rand"
	"errors"
	"sync"

	"rozsa/backend/internal/app/session"
	"rozsa/backend/internal/app/sessions"
)

type MessageEventKind string

const (
	MessageTextDelta MessageEventKind = "text_delta"
	MessageCompleted MessageEventKind = "completed"
	MessageFailed    MessageEventKind = "error"
	MessageCanceled  MessageEventKind = "canceled"
)

type MessageEvent struct {
	SessionID   string
	SessionPath string
	Sequence    uint64
	Kind        MessageEventKind
	Delta       *string
	Error       *string
}

type MessageEventSubscription interface {
	Next(context.Context) (MessageEvent, error)
	Close()
}

type messageOperations struct {
	mu    sync.Mutex
	byID  map[string]*messageOperation
	order []string
}

type messageOperation struct {
	mu          sync.Mutex
	id          string
	execution   sessions.Execution
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

func (runtime *Runtime) StartMessage(ctx context.Context, text string) (string, error) {
	if err := ctx.Err(); err != nil {
		return "", err
	}
	if text == "" {
		return "", errors.New("prompt must not be empty")
	}
	return runtime.startMessage(&text)
}

func (runtime *Runtime) ContinueMessage(ctx context.Context) (string, error) {
	if err := ctx.Err(); err != nil {
		return "", err
	}
	return runtime.startMessage(nil)
}

func (runtime *Runtime) startMessage(prompt *string) (string, error) {
	operationID := rand.Text()
	execution, err := runtime.beginRun(context.Background(), operationID)
	if err != nil {
		return "", err
	}
	operation := &messageOperation{id: operationID, execution: execution, changed: make(chan struct{})}
	runtime.messages.add(operation)
	go func() {
		err := runtime.runExecution(prompt, execution, operation.appendText)
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
	return operation.execution.Cancel()
}

func (operations *messageOperations) add(operation *messageOperation) {
	operations.mu.Lock()
	defer operations.mu.Unlock()
	if operations.byID == nil {
		operations.byID = make(map[string]*messageOperation)
	}
	operations.byID[operation.id] = operation
	operations.order = append(operations.order, operation.id)
	operations.pruneLocked()
}

func (operations *messageOperations) get(id string) (*messageOperation, bool) {
	operations.mu.Lock()
	defer operations.mu.Unlock()
	operation, ok := operations.byID[id]
	return operation, ok
}

func (operations *messageOperations) finish(id string) {
	operations.mu.Lock()
	defer operations.mu.Unlock()
	operations.pruneLocked()
}

func (operations *messageOperations) pruneLocked() {
	for index := 0; len(operations.order) > 64 && index < len(operations.order); {
		oldest := operations.order[index]
		operation := operations.byID[oldest]
		if operation == nil {
			operations.order = append(operations.order[:index], operations.order[index+1:]...)
			continue
		}
		operation.mu.Lock()
		removable := operation.terminal && operation.subscribers == 0
		operation.mu.Unlock()
		if !removable {
			index++
			continue
		}
		delete(operations.byID, oldest)
		operations.order = append(operations.order[:index], operations.order[index+1:]...)
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
	if sessions.CancellationOnly(err) {
		operation.appendLocked(MessageEvent{Kind: MessageCanceled})
	} else if err != nil {
		message := err.Error()
		operation.appendLocked(MessageEvent{Kind: MessageFailed, Error: &message})
	} else {
		operation.appendLocked(MessageEvent{Kind: MessageCompleted})
	}
	operation.terminal = true
}

func (operation *messageOperation) appendLocked(event MessageEvent) {
	event.SessionID = session.IDFromPath(operation.execution.Manager().Path())
	event.SessionPath = operation.execution.Manager().Path()
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
