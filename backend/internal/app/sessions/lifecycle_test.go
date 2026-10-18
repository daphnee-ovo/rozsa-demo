package sessions_test

import (
	"context"
	"errors"
	"path/filepath"
	"reflect"
	"strings"
	"sync"
	"testing"
	"time"

	"rozsa/backend/internal/app/session"
	"rozsa/backend/internal/app/sessions"
)

func register(t *testing.T, owner sessions.Owner, id string) *session.Manager {
	t.Helper()
	manager := session.CreateLazy(filepath.Join(t.TempDir(), id+".jsonl"), id, "/workspace", nil)
	if err := owner.Register(manager); err != nil {
		t.Fatal(err)
	}
	return manager
}

func TestOwnerIsolatesReservationsQueuesAndSnapshotCopies(t *testing.T) {
	owner := sessions.New(sessions.Hooks{})
	a, b := register(t, owner, "a"), register(t, owner, "b")
	runA, err := owner.Start(t.Context(), a.Path(), "operation-a")
	if err != nil {
		t.Fatal(err)
	}
	runB, err := owner.Start(t.Context(), b.Path(), "operation-b")
	if err != nil {
		t.Fatal(err)
	}
	defer runA.Finish(nil)
	defer runB.Finish(nil)
	if _, err := owner.Start(t.Context(), a.Path(), "duplicate"); !errors.Is(err, sessions.ErrRunning) {
		t.Fatalf("overlapping reservation: %v", err)
	}
	if err := owner.Activate(b.Path()); err != nil {
		t.Fatal(err)
	}
	if err := owner.SendRunning(a.Path(), sessions.Queue, " first "); err != nil {
		t.Fatal(err)
	}
	if err := owner.SendRunning(a.Path(), sessions.Queue, "second"); err != nil {
		t.Fatal(err)
	}
	if err := owner.SendRunning(b.Path(), sessions.Steer, "B steering"); err != nil {
		t.Fatal(err)
	}
	if removed, err := owner.RemoveQueued(a.Path(), 1); err != nil || removed != "second" {
		t.Fatalf("remove: %q %v", removed, err)
	}
	if _, err := owner.RemoveQueued(a.Path(), -1); err == nil {
		t.Fatal("negative queue index accepted")
	}
	if _, err := owner.RemoveQueued(a.Path(), 1); err == nil {
		t.Fatal("stale queue index accepted")
	}
	snapshot := owner.Snapshot()
	for _, status := range snapshot.Sessions {
		if len(status.Queued) > 0 {
			status.Queued[0] = "mutated"
		}
	}
	if text, ok := runA.NextQueued(); !ok || text != "first" {
		t.Fatalf("snapshot mutated queue: %q %v", text, ok)
	}
	if steering := runA.TakeSteering(); len(steering) != 0 {
		t.Fatalf("crossed steering: %v", steering)
	}
	if steering := runB.TakeSteering(); !reflect.DeepEqual(steering, []string{"B steering"}) {
		t.Fatalf("steering: %v", steering)
	}
	if owner.Snapshot().ActivePath != b.Path() || runA.Context().Err() != nil {
		t.Fatal("switch changed background execution")
	}
	if _, ok := runA.NextQueued(); ok {
		t.Fatal("queue not exhausted")
	}
	if err := owner.SendRunning(a.Path(), sessions.Queue, "too late"); !errors.Is(err, sessions.ErrNotRunning) {
		t.Fatalf("admitted queue after final drain: %v", err)
	}
}

func TestStopClearsQueueThenCleansInteractionsBeforeCancelAndDashboard(t *testing.T) {
	var owner sessions.Owner
	var execution sessions.Execution
	var path string
	var trace []string
	approvalError := errors.New("approval responder failed")
	owner = sessions.New(sessions.Hooks{
		RejectApprovals: func(id string) error {
			trace = append(trace, "approvals:"+id)
			for _, status := range owner.Snapshot().Sessions {
				if status.Path == path && len(status.Queued) != 0 {
					t.Error("queue cleared after approvals")
				}
				if status.Path == path && len(status.Steering) != 1 {
					t.Error("steering cleared before agent abort")
				}
			}
			if execution.Context().Err() != nil {
				t.Error("cancelled before approval cleanup")
			}
			return approvalError
		},
		CancelQuestions: func(id string) error {
			trace = append(trace, "questions:"+id)
			if execution.Context().Err() != nil {
				t.Error("cancelled before question cleanup")
			}
			return nil
		},
		SessionFinished: func(id string) error {
			trace = append(trace, "dashboard:"+id)
			if !errors.Is(execution.Context().Err(), context.Canceled) {
				t.Error("dashboard finished before cancellation")
			}
			return nil
		},
	})
	manager := register(t, owner, "a")
	path = manager.Path()
	var err error
	execution, err = owner.Start(context.Background(), path, "a")
	if err != nil {
		t.Fatal(err)
	}
	owner.SendRunning(path, sessions.Queue, "discard")
	owner.SendRunning(path, sessions.Steer, "discard steering")
	if err := owner.Cancel(path); !errors.Is(err, approvalError) {
		t.Fatalf("cleanup failure hidden: %v", err)
	}
	if !reflect.DeepEqual(trace, []string{"approvals:a", "questions:a", "dashboard:a"}) {
		t.Fatalf("stop order: %v", trace)
	}
	if _, ok := execution.NextQueued(); ok || len(execution.TakeSteering()) != 0 {
		t.Fatal("stop retained input")
	}
	execution.Finish(context.Canceled)
	newRun, err := owner.Start(context.Background(), path, "new")
	if err != nil {
		t.Fatal(err)
	}
	if err := execution.Cancel(); !errors.Is(err, sessions.ErrNotRunning) {
		t.Fatalf("stale cancellation: %v", err)
	}
	if newRun.Context().Err() != nil {
		t.Fatal("stale cancellation reached new run")
	}
	// Avoid this test's stop-order probes while releasing the new reservation.
	execution = newRun
	if err := owner.SendRunning(path, sessions.Steer, "new steering"); err != nil {
		t.Fatal(err)
	}
	newRun.Cancel()
	newRun.Finish(context.Canceled)
}

func TestCloseWaitsForWritesAndReportsTimeoutWithoutPretendingCompletion(t *testing.T) {
	closed := make(chan string, 1)
	owner := sessions.New(sessions.Hooks{SessionClosed: func(id string) error { closed <- id; return nil }})
	manager := register(t, owner, "a")
	execution, err := owner.Start(context.Background(), manager.Path(), "running")
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if err := owner.Close(ctx, manager.Path()); !errors.Is(err, context.Canceled) {
		t.Fatalf("close before drain: %v", err)
	}
	select {
	case <-closed:
		t.Fatal("reported close before writes finished")
	default:
	}
	if _, err := owner.Start(t.Context(), manager.Path(), "new"); !errors.Is(err, sessions.ErrClosed) {
		t.Fatalf("accepted work while closing: %v", err)
	}
	if _, err := manager.AppendMessage(session.UserMessage{Content: session.UserText("drained")}); err != nil {
		t.Fatal(err)
	}
	execution.Finish(context.Canceled)
	if err := owner.Close(t.Context(), manager.Path()); err != nil {
		t.Fatal(err)
	}
	if id := <-closed; id != "a" {
		t.Fatal(id)
	}
	reader, err := session.Open(manager.Path())
	if err != nil || len(reader.ContextMessages()) != 1 {
		t.Fatalf("close lost write: %v", err)
	}
}

func TestShutdownNotifiesEverySessionBeforeWaitingForAnyOne(t *testing.T) {
	owner := sessions.New(sessions.Hooks{})
	a, b := register(t, owner, "a"), register(t, owner, "b")
	runA, _ := owner.Start(context.Background(), a.Path(), "a")
	runB, _ := owner.Start(context.Background(), b.Path(), "b")
	ended := make(chan error, 1)
	go func() { ended <- owner.Shutdown(context.Background()) }()
	ctx, cancel := context.WithTimeout(t.Context(), time.Second)
	defer cancel()
	for _, execution := range []sessions.Execution{runA, runB} {
		select {
		case <-execution.Context().Done():
		case <-ctx.Done():
			t.Fatal("shutdown waited before cancelling all sessions")
		}
	}
	var writes sync.WaitGroup
	for _, execution := range []sessions.Execution{runA, runB} {
		writes.Go(func() { execution.Finish(context.Canceled) })
	}
	writes.Wait()
	if err := <-ended; err != nil {
		t.Fatal(err)
	}
	if _, err := owner.Start(t.Context(), a.Path(), "late"); err == nil {
		t.Fatal("shutdown accepted new run")
	}
	if len(owner.Snapshot().Sessions) != 0 {
		t.Fatal("shutdown retained sessions")
	}
}

func TestWindowShutdownRejectsAllApprovalsBeforeCancellingQuestions(t *testing.T) {
	var trace []string
	var executions []sessions.Execution
	owner := sessions.New(sessions.Hooks{
		RejectApprovals: func(id string) error {
			trace = append(trace, "approval:"+id)
			for _, execution := range executions {
				if execution.Context().Err() != nil {
					t.Error("execution cancelled before all approvals")
				}
			}
			return nil
		},
		CancelQuestions: func(id string) error {
			if len(trace) < 2 || !strings.HasPrefix(trace[0], "approval:") || !strings.HasPrefix(trace[1], "approval:") {
				t.Error("question cancelled before all approvals")
			}
			trace = append(trace, "question:"+id)
			for _, execution := range executions {
				if execution.Context().Err() != nil {
					t.Error("execution cancelled before all questions")
				}
			}
			return nil
		},
	})
	a, b := register(t, owner, "a"), register(t, owner, "b")
	for _, manager := range []*session.Manager{a, b} {
		execution, err := owner.Start(context.Background(), manager.Path(), manager.SessionID())
		if err != nil {
			t.Fatal(err)
		}
		executions = append(executions, execution)
	}
	for _, execution := range executions {
		go func() { <-execution.Context().Done(); execution.Finish(context.Canceled) }()
	}
	ctx, cancel := context.WithTimeout(t.Context(), time.Second)
	defer cancel()
	if err := owner.Shutdown(ctx); err != nil {
		t.Fatal(err)
	}
	if len(trace) != 4 {
		t.Fatalf("duplicate cleanup: %v", trace)
	}
}

func TestCloseReturnsWriteFailureAfterCancellation(t *testing.T) {
	owner := sessions.New(sessions.Hooks{})
	manager := register(t, owner, "a")
	execution, err := owner.Start(context.Background(), manager.Path(), "a")
	if err != nil {
		t.Fatal(err)
	}
	writeErr := errors.New("session write failed")
	go func() { <-execution.Context().Done(); execution.Finish(errors.Join(context.Canceled, writeErr)) }()
	ctx, cancel := context.WithTimeout(t.Context(), time.Second)
	defer cancel()
	if err := owner.Close(ctx, manager.Path()); !errors.Is(err, writeErr) {
		t.Fatalf("close hid write failure: %v", err)
	}
}
