// Package sessions owns work independently of the selected UI tab.
package sessions

import (
	"context"
	"errors"
	"fmt"
	"sort"
	"strings"
	"sync"

	"rozsa/backend/internal/app/session"
)

type SendMode string

const (
	Queue SendMode = "queue"
	Steer SendMode = "steer"
)

var (
	ErrRunning    = errors.New("agent session is already running")
	ErrNotRunning = errors.New("agent session is not running")
	ErrClosed     = errors.New("session owner is closed")
)

type Status struct {
	Path, SessionID, CatalogID, OperationID string
	Running                                 bool
	Queued, Steering                        []string
	Error                                   string
}

type Snapshot struct {
	ActivePath string
	Sessions   []Status
}

// Hooks run outside the state lock, after prompt persistence and before stop's
// execution cancellation, respectively.
type Hooks struct {
	RejectApprovals func(string) error
	CancelQuestions func(string) error
	SessionFinished func(string) error
	SessionClosed   func(string) error
}

type Owner interface {
	Register(*session.Manager) error
	Activate(string) error
	Active() (*session.Manager, error)
	Start(context.Context, string, string) (Execution, error)
	SendRunning(string, SendMode, string) error
	RemoveQueued(string, int) (string, error)
	Snapshot() Snapshot
	Cancel(string) error
	Close(context.Context, string) error
	Shutdown(context.Context) error
}

// Execution is a reservation covering a prompt and its serial queued prompts.
type Execution interface {
	Context() context.Context
	Manager() *session.Manager
	TakeSteering() []string
	PromptFinished() error
	NextQueued() (string, bool)
	Cancel() error
	Finish(error) error
}

type owner struct {
	mu         sync.Mutex
	shutdownMu sync.Mutex
	entries    map[string]*entry
	active     string
	closed     bool
	hooks      Hooks
}

type entry struct {
	manager         *session.Manager
	execution       *execution
	queue, steering []string
	err             string
	closed          bool
}

type execution struct {
	owner      *owner
	entry      *entry
	id         string
	ctx        context.Context
	cancel     context.CancelFunc
	done       chan struct{}
	stopping   bool
	accepting  bool
	stopDone   chan struct{}
	stopErr    error
	finishOnce sync.Once
	finishErr  error
}

func New(hooks Hooks) Owner { return &owner{entries: make(map[string]*entry), hooks: hooks} }

func (owner *owner) Register(manager *session.Manager) error {
	if manager == nil {
		return errors.New("session manager is required")
	}
	owner.mu.Lock()
	defer owner.mu.Unlock()
	if owner.closed {
		return ErrClosed
	}
	path := manager.Path()
	if previous := owner.entries[path]; previous != nil {
		if previous.manager != manager {
			return errors.New("session already has a different writable owner")
		}
		if previous.closed {
			return ErrClosed
		}
		return nil
	}
	owner.entries[path] = &entry{manager: manager}
	if owner.active == "" {
		owner.active = path
	}
	return nil
}

func (owner *owner) Activate(path string) error {
	owner.mu.Lock()
	defer owner.mu.Unlock()
	entry, err := owner.entryLocked(path)
	if err != nil {
		return err
	}
	if entry.closed || owner.closed {
		return ErrClosed
	}
	owner.active = path
	return nil
}

func (owner *owner) Active() (*session.Manager, error) {
	owner.mu.Lock()
	defer owner.mu.Unlock()
	entry, err := owner.entryLocked(owner.active)
	if err != nil {
		return nil, err
	}
	if owner.closed || entry.closed {
		return nil, ErrClosed
	}
	return entry.manager, nil
}

func (owner *owner) Start(ctx context.Context, path, operationID string) (Execution, error) {
	if err := ctx.Err(); err != nil {
		return nil, err
	}
	owner.mu.Lock()
	defer owner.mu.Unlock()
	entry, err := owner.entryLocked(path)
	if err != nil {
		return nil, err
	}
	if owner.closed || entry.closed {
		return nil, ErrClosed
	}
	if entry.execution != nil {
		return nil, ErrRunning
	}
	ctx, cancel := context.WithCancel(ctx)
	execution := &execution{owner: owner, entry: entry, id: operationID, ctx: ctx, cancel: cancel, done: make(chan struct{}), accepting: true}
	entry.execution, entry.err = execution, ""
	return execution, nil
}

func (owner *owner) SendRunning(path string, mode SendMode, text string) error {
	text = strings.TrimSpace(text)
	if text == "" {
		return nil
	}
	if mode != Queue && mode != Steer {
		return fmt.Errorf("unknown running send mode: %s", mode)
	}
	owner.mu.Lock()
	defer owner.mu.Unlock()
	entry, err := owner.entryLocked(path)
	if err != nil {
		return err
	}
	if entry.execution == nil || entry.execution.stopping || !entry.execution.accepting {
		return ErrNotRunning
	}
	if mode == Queue {
		entry.queue = append(entry.queue, text)
	} else {
		entry.steering = append(entry.steering, text)
	}
	return nil
}

func (owner *owner) RemoveQueued(path string, index int) (string, error) {
	owner.mu.Lock()
	defer owner.mu.Unlock()
	entry, err := owner.entryLocked(path)
	if err != nil {
		return "", err
	}
	if index < 0 || index >= len(entry.queue) {
		return "", errors.New("queued message no longer exists")
	}
	text := entry.queue[index]
	entry.queue = append(entry.queue[:index], entry.queue[index+1:]...)
	return text, nil
}

func (owner *owner) Snapshot() Snapshot {
	owner.mu.Lock()
	defer owner.mu.Unlock()
	result := Snapshot{ActivePath: owner.active, Sessions: []Status{}}
	for path, entry := range owner.entries {
		status := Status{Path: path, SessionID: session.IDFromPath(path), CatalogID: entry.manager.SessionID(), Running: entry.execution != nil,
			Queued: append([]string{}, entry.queue...), Steering: append([]string{}, entry.steering...), Error: entry.err}
		if entry.execution != nil {
			status.OperationID = entry.execution.id
		}
		result.Sessions = append(result.Sessions, status)
	}
	sort.Slice(result.Sessions, func(i, j int) bool { return result.Sessions[i].Path < result.Sessions[j].Path })
	return result
}

func (owner *owner) Cancel(path string) error {
	owner.mu.Lock()
	entry, err := owner.entryLocked(path)
	if err != nil {
		owner.mu.Unlock()
		return err
	}
	execution := entry.execution
	if execution == nil {
		owner.mu.Unlock()
		return ErrNotRunning
	}
	owner.mu.Unlock()
	return execution.Cancel()
}

func (execution *execution) Cancel() error {
	owner := execution.owner
	owner.mu.Lock()
	entry := execution.entry
	if entry.execution != execution {
		owner.mu.Unlock()
		return ErrNotRunning
	}
	if execution.stopping {
		done := execution.stopDone
		owner.mu.Unlock()
		if done != nil {
			<-done
		}
		owner.mu.Lock()
		err := execution.stopErr
		owner.mu.Unlock()
		return err
	}
	execution.stopping = true
	execution.stopDone = make(chan struct{})
	entry.queue = nil
	owner.mu.Unlock()
	err := owner.cleanup(session.IDFromPath(entry.manager.Path()))
	owner.mu.Lock()
	entry.steering = nil
	owner.mu.Unlock()
	execution.cancel()
	if owner.hooks.SessionFinished != nil {
		err = errors.Join(err, owner.hooks.SessionFinished(session.IDFromPath(entry.manager.Path())))
	}
	owner.mu.Lock()
	execution.stopErr = err
	close(execution.stopDone)
	owner.mu.Unlock()
	return err
}

func (owner *owner) Close(ctx context.Context, path string) error {
	owner.mu.Lock()
	entry, err := owner.entryLocked(path)
	if err != nil {
		owner.mu.Unlock()
		return err
	}
	entry.closed = true
	execution := entry.execution
	owner.mu.Unlock()
	if execution != nil {
		err = owner.Cancel(path)
		select {
		case <-execution.done:
		case <-ctx.Done():
			return errors.Join(err, ctx.Err())
		}
		if writeErr := execution.Finish(nil); !CancellationOnly(writeErr) {
			err = errors.Join(err, writeErr)
		}
	} else {
		err = owner.cleanup(session.IDFromPath(entry.manager.Path()))
	}
	if owner.hooks.SessionClosed != nil {
		err = errors.Join(err, owner.hooks.SessionClosed(session.IDFromPath(entry.manager.Path())))
	}
	owner.mu.Lock()
	delete(owner.entries, path)
	if owner.active == path {
		owner.active = ""
	}
	owner.mu.Unlock()
	return err
}

func (owner *owner) Shutdown(ctx context.Context) error {
	owner.shutdownMu.Lock()
	defer owner.shutdownMu.Unlock()
	owner.mu.Lock()
	owner.closed = true
	paths := make([]string, 0, len(owner.entries))
	entries := make(map[string]*entry, len(owner.entries))
	stops := make(map[string]*execution)
	for path := range owner.entries {
		paths = append(paths, path)
		entry := owner.entries[path]
		entry.closed = true
		entries[path] = entry
		if execution := entry.execution; execution != nil && !execution.stopping {
			execution.stopping = true
			execution.stopDone = make(chan struct{})
			entry.queue = nil
			stops[path] = execution
		}
	}
	owner.mu.Unlock()
	sort.Strings(paths)
	var err error
	cleanupErrors := make(map[string]error)
	// Window-close order: reject all approvals, then cancel all questions,
	// before notifying execution cancellation. Native teardown belongs to host.
	for _, path := range paths {
		if owner.hooks.RejectApprovals != nil {
			cleanupErrors[path] = owner.hooks.RejectApprovals(session.IDFromPath(path))
		}
	}
	for _, path := range paths {
		if owner.hooks.CancelQuestions != nil {
			cleanupErrors[path] = errors.Join(cleanupErrors[path], owner.hooks.CancelQuestions(session.IDFromPath(path)))
		}
	}
	// Notify every session before waiting for any provider's final writes.
	for _, path := range paths {
		if execution := stops[path]; execution != nil {
			owner.mu.Lock()
			execution.entry.steering = nil
			owner.mu.Unlock()
			execution.cancel()
			if owner.hooks.SessionFinished != nil {
				cleanupErrors[path] = errors.Join(cleanupErrors[path], owner.hooks.SessionFinished(session.IDFromPath(path)))
			}
			owner.mu.Lock()
			execution.stopErr = cleanupErrors[path]
			close(execution.stopDone)
			owner.mu.Unlock()
		}
		err = errors.Join(err, cleanupErrors[path])
	}
	for _, path := range paths {
		owner.mu.Lock()
		execution := entries[path].execution
		owner.mu.Unlock()
		if execution != nil {
			select {
			case <-execution.done:
			case <-ctx.Done():
				return errors.Join(err, ctx.Err())
			}
			if writeErr := execution.Finish(nil); !CancellationOnly(writeErr) {
				err = errors.Join(err, writeErr)
			}
		}
		if owner.hooks.SessionClosed != nil {
			err = errors.Join(err, owner.hooks.SessionClosed(session.IDFromPath(path)))
		}
		owner.mu.Lock()
		if owner.entries[path] == entries[path] {
			delete(owner.entries, path)
		}
		if owner.active == path {
			owner.active = ""
		}
		owner.mu.Unlock()
	}
	return err
}

func (owner *owner) entryLocked(path string) (*entry, error) {
	entry := owner.entries[path]
	if entry == nil {
		return nil, errors.New("session is not registered")
	}
	return entry, nil
}

func (owner *owner) cleanup(id string) error {
	var err error
	if owner.hooks.RejectApprovals != nil {
		err = owner.hooks.RejectApprovals(id)
	}
	if owner.hooks.CancelQuestions != nil {
		err = errors.Join(err, owner.hooks.CancelQuestions(id))
	}
	return err
}

func (execution *execution) Context() context.Context  { return execution.ctx }
func (execution *execution) Manager() *session.Manager { return execution.entry.manager }

func (execution *execution) TakeSteering() []string {
	execution.owner.mu.Lock()
	defer execution.owner.mu.Unlock()
	if execution.stopping {
		return nil
	}
	messages := execution.entry.steering
	execution.entry.steering = nil
	return messages
}

func (execution *execution) PromptFinished() error {
	return execution.owner.cleanup(session.IDFromPath(execution.entry.manager.Path()))
}

func (execution *execution) NextQueued() (string, bool) {
	execution.owner.mu.Lock()
	defer execution.owner.mu.Unlock()
	if execution.stopping || execution.ctx.Err() != nil || len(execution.entry.queue) == 0 {
		if execution.ctx.Err() != nil {
			execution.entry.queue, execution.entry.steering = nil, nil
		}
		execution.accepting = false
		return "", false
	}
	text := execution.entry.queue[0]
	execution.entry.queue = execution.entry.queue[1:]
	return text, true
}

func (execution *execution) Finish(err error) error {
	execution.finishOnce.Do(func() { execution.finishErr = execution.finish(err) })
	return execution.finishErr
}

func (execution *execution) finish(err error) error {
	execution.owner.mu.Lock()
	if execution.entry.execution != execution {
		execution.owner.mu.Unlock()
		return err
	}
	stopped := execution.stopping
	stopDone := execution.stopDone
	execution.stopping = true
	execution.accepting = false
	execution.owner.mu.Unlock()
	if stopDone != nil {
		<-stopDone
		execution.owner.mu.Lock()
		err = errors.Join(err, execution.stopErr)
		execution.owner.mu.Unlock()
	}
	if !stopped && execution.owner.hooks.SessionFinished != nil {
		err = errors.Join(err, execution.owner.hooks.SessionFinished(session.IDFromPath(execution.entry.manager.Path())))
	}
	execution.owner.mu.Lock()
	defer execution.owner.mu.Unlock()
	if err != nil {
		execution.entry.err = err.Error()
	}
	execution.entry.execution = nil
	execution.cancel()
	close(execution.done)
	return err
}
