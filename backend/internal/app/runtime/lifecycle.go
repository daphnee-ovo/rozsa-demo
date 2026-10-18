package runtime

import (
	"context"
	"crypto/rand"
	"errors"
	"fmt"
	"path/filepath"

	"rozsa/backend/internal/app/permissions"
	"rozsa/backend/internal/app/session"
	"rozsa/backend/internal/app/sessions"
)

// ApprovalRequests is the shared pending-request boundary consumed by tool and
// transport slices; runtime cancellation cleans this same owner.
type ApprovalRequests interface {
	Add(permissions.Identity, permissions.ApprovalInfo) error
	Get(permissions.Identity) (permissions.Request, bool)
	Queue(string) []permissions.Request
	BeginResponse(permissions.Identity, permissions.Response) error
	FinishResponse(permissions.Identity, bool) error
	Mark(permissions.Identity, permissions.RequestStatus) error
}

func (runtime *Runtime) ApprovalOwner() ApprovalRequests { return runtime.approvals }

func (runtime *Runtime) SessionStatus() sessions.Snapshot { return runtime.lifecycle.Snapshot() }

func (runtime *Runtime) SendRunningMessage(ctx context.Context, path string, mode sessions.SendMode, text string) error {
	if err := ctx.Err(); err != nil {
		return err
	}
	return runtime.lifecycle.SendRunning(path, mode, text)
}

func (runtime *Runtime) RemoveQueuedMessage(ctx context.Context, path string, index int) (string, error) {
	if err := ctx.Err(); err != nil {
		return "", err
	}
	return runtime.lifecycle.RemoveQueued(path, index)
}

// Shutdown rejects new work, cancels all sessions, and waits for their writes.
// UI subscription closure and reload deliberately do not call this method.
func (runtime *Runtime) Shutdown(ctx context.Context) error {
	runtime.mu.Lock()
	runtime.closing = true
	runtime.mu.Unlock()
	return errors.Join(runtime.shutdownSubagents(ctx), runtime.lifecycle.Shutdown(ctx))
}

// ForkSession copies display history through the selected user message, with
// fresh JSONL entry identities and an explicit source path, as fork_session does.
// A failed write leaves the successfully written prefix on disk and reports it.
func (runtime *Runtime) ForkSession(ctx context.Context, messageIndex int) (session.LoadedSnapshot, error) {
	if err := ctx.Err(); err != nil {
		return session.LoadedSnapshot{}, err
	}
	runtime.mu.Lock()
	defer runtime.mu.Unlock()
	if runtime.closing {
		return session.LoadedSnapshot{}, sessions.ErrClosed
	}
	source, err := runtime.lifecycle.Active()
	if err != nil {
		return session.LoadedSnapshot{}, err
	}
	messages := runtime.conversationLocked(source).displayMessages()
	if messageIndex < 0 || messageIndex >= len(messages) {
		return session.LoadedSnapshot{}, errors.New("Fork point no longer exists")
	}
	if _, ok := messages[messageIndex].(session.UserMessage); !ok {
		return session.LoadedSnapshot{}, errors.New("A fork point must be a user message")
	}
	id := rand.Text()
	path := filepath.Join(runtime.sessionDir, id+".jsonl")
	sourcePath := source.Path()
	manager, err := session.Create(path, id, source.Cwd(), &sourcePath)
	if err != nil {
		return session.LoadedSnapshot{}, err
	}
	for _, message := range messages[:messageIndex+1] {
		if _, err := manager.AppendMessage(message); err != nil {
			return session.LoadedSnapshot{}, fmt.Errorf("fork partially written to %s: %w", path, err)
		}
	}
	if err := runtime.service.AdoptManager(manager); err != nil {
		return session.LoadedSnapshot{}, err
	}
	if err := runtime.lifecycle.Register(manager); err != nil {
		return session.LoadedSnapshot{}, err
	}
	snapshot, err := runtime.service.Switch(context.WithoutCancel(ctx), path, id)
	if err != nil {
		return session.LoadedSnapshot{}, err
	}
	if err := runtime.lifecycle.Activate(path); err != nil {
		return session.LoadedSnapshot{}, err
	}
	runtime.session = manager
	return snapshot, nil
}
