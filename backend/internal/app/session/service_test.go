package session

import (
	"context"
	"errors"
	"os"
	"sync"
	"testing"
)

func TestServiceWatermarksAndBoundedOverflow(t *testing.T) {
	dir := fixtureDir(t)
	path := write(t, dir, "stem.jsonl", sessionHeader("header"))
	service := NewService(Catalog{Directories: []string{dir}})
	subscription := service.Subscribe()
	defer subscription.Close()
	initial, err := service.Snapshot(t.Context())
	if err != nil || initial.Epoch == "" || initial.Watermark != 0 || initial.Active != nil {
		t.Fatalf("%+v %v", initial, err)
	}
	if _, err := service.Switch(t.Context(), path, "wrong"); err == nil {
		t.Fatal("wrong identity accepted")
	}
	for i := 0; i < EventBufferSize+1; i++ {
		if _, err := service.Switch(t.Context(), path, "header"); err != nil {
			t.Fatal(err)
		}
	}
	if _, err := subscription.Next(t.Context()); !errors.Is(err, ErrOverflow) {
		t.Fatalf("lost events were hidden: %v", err)
	}
	subscription.Close()
	fresh := service.Subscribe()
	defer fresh.Close()
	snapshot, _ := service.Snapshot(t.Context())
	if snapshot.Watermark != EventBufferSize+1 || snapshot.Epoch != initial.Epoch || snapshot.Active.CatalogID != "header" || snapshot.Active.SessionID != "stem" {
		t.Fatal(snapshot)
	}
	service.Switch(t.Context(), path, "header")
	event, err := fresh.Next(t.Context())
	if err != nil || event.Sequence != snapshot.Watermark+1 || event.Epoch != snapshot.Epoch {
		t.Fatalf("%+v %v", event, err)
	}
}

func TestServiceConcurrentSnapshotSwitchAndClose(t *testing.T) {
	dir := fixtureDir(t)
	path := write(t, dir, "session.jsonl", sessionHeader("session"))
	service := NewService(Catalog{Directories: []string{dir}})
	var workers sync.WaitGroup
	for i := 0; i < 12; i++ {
		workers.Go(func() {
			for j := 0; j < 12; j++ {
				subscription := service.Subscribe()
				snapshot, err := service.Snapshot(t.Context())
				if err != nil || snapshot.Epoch == "" {
					t.Errorf("%+v %v", snapshot, err)
				}
				if _, err := service.Switch(t.Context(), path, "session"); err != nil {
					t.Error(err)
				}
				subscription.Close()
				// A starved subscriber may already have overflowed before Close.
				// Both failures must stay visible; neither may deliver success.
				if _, err := subscription.Next(t.Context()); !errors.Is(err, ErrSubscriptionClosed) && !errors.Is(err, ErrOverflow) {
					t.Errorf("closed receiver: %v", err)
				}
			}
		})
	}
	workers.Wait()
	state, _ := service.Snapshot(t.Context())
	if state.Watermark != 144 {
		t.Fatal(state)
	}
	ctx, cancel := context.WithCancel(t.Context())
	cancel()
	if _, err := service.Switch(ctx, path, "session"); !errors.Is(err, context.Canceled) {
		t.Fatal(err)
	}
	unchanged, _ := service.Snapshot(t.Context())
	if unchanged.Watermark != state.Watermark {
		t.Fatal("canceled work mutated state")
	}
}

func TestServiceRefreshReloadsWrittenMessagesAndPublishes(t *testing.T) {
	dir := fixtureDir(t)
	original := sessionHeader("session") + `{"type":"message","id":"user","parentId":null,"timestamp":"1","message":{"role":"user","content":"question"}}` + "\n"
	path := write(t, dir, "session.jsonl", original)
	service := NewService(Catalog{Directories: []string{dir}})
	if _, err := service.Switch(t.Context(), path, "session"); err != nil {
		t.Fatal(err)
	}
	subscription := service.Subscribe()
	defer subscription.Close()

	assistant := `{"type":"message","id":"assistant","parentId":"user","timestamp":"2","message":{"role":"assistant","model":"synthetic","content":[{"type":"text","text":"answer"}]}}` + "\n"
	if err := os.WriteFile(path, []byte(original+assistant), 0600); err != nil {
		t.Fatal(err)
	}
	refreshed, err := service.Refresh(t.Context(), path, "session")
	if err != nil || len(refreshed.Messages) != 2 || refreshed.Messages[1].EntryID != "assistant" {
		t.Fatalf("refreshed snapshot = %+v, %v", refreshed, err)
	}
	event, err := subscription.Next(t.Context())
	if err != nil || event.Sequence != 2 || len(event.Active.Messages) != 2 || event.Active.Messages[1].EntryID != "assistant" {
		t.Fatalf("refresh event = %+v, %v", event, err)
	}
	state, err := service.Snapshot(t.Context())
	if err != nil || state.Watermark != 2 || state.Active == nil || len(state.Active.Messages) != 2 {
		t.Fatalf("refreshed state = %+v, %v", state, err)
	}

	ctx, cancel := context.WithCancel(t.Context())
	cancel()
	if _, err := service.Refresh(ctx, path, "session"); !errors.Is(err, context.Canceled) {
		t.Fatalf("canceled refresh error = %v", err)
	}
	unchanged, _ := service.Snapshot(t.Context())
	if unchanged.Watermark != state.Watermark || len(unchanged.Active.Messages) != 2 {
		t.Fatalf("canceled refresh mutated state: %+v", unchanged)
	}
}
