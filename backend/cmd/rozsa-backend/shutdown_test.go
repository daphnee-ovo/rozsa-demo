package main

import (
	"context"
	"encoding/json"
	"errors"
	"net"
	"path/filepath"
	"sync"
	"testing"
	"time"

	"go.lsp.dev/jsonrpc2"
	"rozsa/backend/internal/app/runtime"
	"rozsa/backend/internal/app/session"
	"rozsa/backend/internal/app/sessions"
	"rozsa/backend/internal/model/types"
	"rozsa/backend/internal/transport/server"
)

type shutdownProvider struct{ started chan string }

func (provider shutdownProvider) StreamSimple(ctx context.Context, model types.Model, _ types.Context, options types.SimpleStreamOptions, emit func(types.StreamEvent)) error {
	message := types.AssistantMessage{API: model.API, Provider: model.Provider, Model: model.ID, Content: []types.ContentBlock{types.TextBlock{Text: "partial"}}}
	emit(types.Start{Partial: message})
	emit(types.TextDelta{ContentIndex: 0, Delta: "partial", Partial: message})
	provider.started <- *options.Base.SessionID
	<-ctx.Done()
	return ctx.Err()
}

func TestProductRPCShutdownDrainsTwoSessionWritesBeforeAcknowledging(t *testing.T) {
	directory := t.TempDir()
	paths := map[string]string{}
	for _, id := range []string{"a", "b"} {
		paths[id] = filepath.Join(directory, id+".jsonl")
		if _, err := session.Create(paths[id], id, directory, nil); err != nil {
			t.Fatal(err)
		}
	}
	service := session.NewService(session.Catalog{Directories: []string{directory}})
	provider := shutdownProvider{started: make(chan string, 2)}
	var trace []string
	var traceMu sync.Mutex
	record := func(action, id string) { traceMu.Lock(); defer traceMu.Unlock(); trace = append(trace, action+":"+id) }
	app, err := runtime.New(runtime.Config{SessionDir: directory, SessionService: service, Model: types.Model{ID: "test", Provider: "test", API: types.APIAnthropicMessages}, Provider: provider, LifecycleHooks: sessions.Hooks{
		RejectApprovals: func(id string) error { record("approval", id); return nil },
		CancelQuestions: func(id string) error { record("question", id); return nil },
		SessionClosed: func(id string) error {
			reader, err := session.Open(paths[id])
			if err != nil {
				return err
			}
			messages := reader.ContextMessages()
			if len(messages) != 2 {
				return errors.New("session closed before final writes")
			}
			if assistant, ok := messages[1].(session.AssistantMessage); !ok || assistant.StopReason != session.Aborted {
				return errors.New("session closed without aborted assistant")
			}
			record("closed", id)
			return nil
		},
	}})
	if err != nil {
		t.Fatal(err)
	}
	coordinator := newRuntimeCoordinator(service)
	coordinator.complete(app, nil)
	client, backend := net.Pipe()
	client.SetDeadline(time.Now().Add(6 * time.Second))
	ended := make(chan error, 1)
	go func() {
		ended <- server.ServeProductApplication(backend, backend, coordinator, productMessageApplication{coordinator: coordinator})
		backend.Close()
	}()
	t.Cleanup(func() { client.Close(); backend.Close() })
	stream := jsonrpc2.NewStream(client)
	nextID := int64(0)
	call := func(method string, params any) *jsonrpc2.Response {
		t.Helper()
		nextID++
		data, err := json.Marshal(params)
		if err != nil {
			t.Fatal(err)
		}
		if _, err := stream.Write(t.Context(), jsonrpc2.NewCall(jsonrpc2.NewNumberID(nextID), method, jsonrpc2.RawMessage(data))); err != nil {
			t.Fatal(err)
		}
		message, _, err := stream.Read(t.Context())
		if err != nil {
			t.Fatal(err)
		}
		response, ok := message.(*jsonrpc2.Response)
		if !ok {
			t.Fatalf("response: %T", message)
		}
		if response.Err() != nil {
			t.Fatal(response.Err())
		}
		return response
	}
	call("initialize", map[string]any{"protocolVersion": 1})
	for _, id := range []string{"a", "b"} {
		call("switch_session", map[string]string{"path": paths[id], "catalogId": id})
		call("start_message", map[string]string{"text": "question " + id})
		select {
		case actual := <-provider.started:
			if actual != id {
				t.Fatalf("provider owner: %s want %s", actual, id)
			}
		case <-time.After(time.Second):
			t.Fatal("provider did not start")
		}
	}
	if result := string(call("shutdown", map[string]any{}).Result()); result != "true" {
		t.Fatalf("shutdown: %s", result)
	}
	// Closed hooks check durable writes before this successful wire response.
	traceMu.Lock()
	if len(trace) < 6 || trace[0] != "approval:a" || trace[1] != "approval:b" || trace[2] != "question:a" || trace[3] != "question:b" {
		t.Errorf("window cleanup order: %v", trace)
	}
	traceMu.Unlock()
	if len(app.SessionStatus().Sessions) != 0 {
		t.Fatal("acknowledged shutdown while sessions remained owned")
	}
	client.Close()
	select {
	case err := <-ended:
		if err != nil {
			t.Fatal(err)
		}
	case <-time.After(time.Second):
		t.Fatal("server did not exit after EOF")
	}
}

func TestCoordinatorClosesRuntimeThatFinishesInitializationAfterShutdown(t *testing.T) {
	service := session.NewService(session.Catalog{Directories: []string{t.TempDir()}})
	coordinator := newRuntimeCoordinator(service)
	ctx, cancel := context.WithCancel(t.Context())
	cancel()
	if err := coordinator.Shutdown(ctx); !errors.Is(err, context.Canceled) {
		t.Fatalf("initialization wait: %v", err)
	}
	app, err := runtime.New(runtime.Config{SessionDir: t.TempDir(), SessionService: service, Model: types.Model{ID: "test", Provider: "test", API: types.APIAnthropicMessages}, Provider: coordinatorTestProvider{}})
	if err != nil {
		t.Fatal(err)
	}
	coordinator.complete(app, nil)
	if _, err := app.StartMessage(t.Context(), "late"); !errors.Is(err, sessions.ErrClosed) {
		t.Fatalf("late runtime escaped close: %v", err)
	}
	if _, err := coordinator.await(t.Context()); err == nil {
		t.Fatal("closing coordinator exposed runtime")
	}
	if err := coordinator.Shutdown(t.Context()); err != nil {
		t.Fatal(err)
	}
}
