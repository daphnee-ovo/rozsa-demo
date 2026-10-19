package runtime

import (
	"context"

	"rozsa/backend/internal/app/permissions"
)

// ApprovalService is the app boundary for reading and resolving requests.
// Transport adapters depend on this interface instead of the runtime owner.
type ApprovalService interface {
	PendingApprovals(string) []permissions.Request
	RespondApproval(context.Context, permissions.Identity, permissions.Response) error
}

func (runtime *Runtime) ApprovalOwner() ApprovalService { return runtime }

func (runtime *Runtime) PendingApprovals(sessionID string) []permissions.Request {
	return runtime.approvals.Queue(sessionID)
}

func (runtime *Runtime) closeApprovalSession(sessionID string) error {
	state := runtime.permissionRuntime
	state.mu.Lock()
	defer state.mu.Unlock()
	runtime.approvals.CloseSession(sessionID)
	for identity, waiter := range state.waiters {
		if identity.SessionID != sessionID {
			continue
		}
		delete(state.waiters, identity)
		select {
		case waiter.response <- permissions.Response{Kind: permissions.Deny}:
		default:
		}
	}
	return nil
}
