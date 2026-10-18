package main

import (
	"bytes"
	"context"
	"flag"
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"os"
	"os/exec"
	"path/filepath"
	"testing"
	"time"

	"go.lsp.dev/jsonrpc2"
	"rozsa/backend/internal/app/runtime"
	"rozsa/backend/internal/app/session"
	"rozsa/backend/internal/model/types"
)

type coordinatorTestProvider struct{}

func (coordinatorTestProvider) StreamSimple(context.Context, types.Model, types.Context, types.SimpleStreamOptions, func(types.StreamEvent)) error {
	return nil
}

func TestRuntimeCoordinatorServesSessionsAndAdoptsSelectionBeforeReady(t *testing.T) {
	directory := t.TempDir()
	selectedPath := filepath.Join(directory, "selected.jsonl")
	otherPath := filepath.Join(directory, "other.jsonl")
	for id, path := range map[string]string{"selected": selectedPath, "other": otherPath} {
		content := fmt.Sprintf("{\"type\":\"session\",\"version\":3,\"id\":%q,\"timestamp\":\"2026-10-18T00:00:00Z\",\"cwd\":%q}\n", id, directory)
		if err := os.WriteFile(path, []byte(content), 0o600); err != nil {
			t.Fatal(err)
		}
	}
	sessions := session.NewService(session.Catalog{Directories: []string{directory}})
	coordinator := newRuntimeCoordinator(sessions)
	entries, err := coordinator.GetSessions(t.Context())
	if err != nil || len(entries) != 2 {
		t.Fatalf("session catalog before runtime readiness = %d entries, err=%v", len(entries), err)
	}
	selected, err := coordinator.Switch(t.Context(), selectedPath, "selected")
	if err != nil || selected.Path != selectedPath {
		t.Fatalf("switch before runtime readiness = %+v err=%v", selected, err)
	}
	if status := coordinator.RuntimeStatus(); status.State != "initializing" {
		t.Fatalf("status before completion = %+v", status)
	}
	app, err := runtime.New(runtime.Config{
		SessionDir: directory, SessionDirs: []string{directory}, SessionService: sessions,
		Model: types.Model{ID: "test", Provider: "test", API: types.APIAnthropicMessages}, Provider: coordinatorTestProvider{},
	})
	if err != nil {
		t.Fatal(err)
	}
	coordinator.complete(app, nil)
	if status := coordinator.RuntimeStatus(); status.State != "ready" {
		t.Fatalf("status after completion = %+v", status)
	}
	active, err := app.Snapshot(t.Context())
	if err != nil || active.Active == nil || active.Active.Path != selectedPath {
		t.Fatalf("runtime active session after initialization = %+v err=%v, want %s", active.Active, err, selectedPath)
	}
	if _, err := coordinator.Switch(t.Context(), otherPath, "other"); err != nil {
		t.Fatal(err)
	}
	shared, err := coordinator.Snapshot(t.Context())
	if err != nil || shared.Active == nil || shared.Active.Path != otherPath {
		t.Fatalf("runtime session switch was not published through the early session service: %+v err=%v", shared.Active, err)
	}
}

func TestRuntimeCoordinatorPublishesInitializationFailureWithoutMessageRequest(t *testing.T) {
	coordinator := newRuntimeCoordinator(session.NewService(session.Catalog{Directories: []string{t.TempDir()}}))
	coordinator.complete(nil, fmt.Errorf("missing model credentials"))
	status := coordinator.RuntimeStatus()
	if status.State != "failed" || status.Error != "missing model credentials" {
		t.Fatalf("failed runtime status = %+v", status)
	}
	if _, err := coordinator.await(t.Context()); err == nil || err.Error() != status.Error {
		t.Fatalf("runtime wait error = %v, want initialization diagnostic %q", err, status.Error)
	}
}

// Execute the actual main in an owned child, including its stderr diagnostics
// and exit status. No external build/runner or user session directory is used.
func TestBackendHelperProcess(t *testing.T) {
	if os.Getenv("ROZSA_TEST_BACKEND_PROCESS") != "1" {
		return
	}
	os.Args = []string{os.Args[0]}
	flag.CommandLine = flag.NewFlagSet(os.Args[0], flag.ExitOnError)
	main()
	os.Exit(0)
}

type processIO struct {
	io.Reader
	io.Writer
}

func (processIO) Close() error { return nil }

func TestStdioProcessProtocolAndDiagnostics(t *testing.T) {
	configRoot := t.TempDir()
	global := filepath.Join(configRoot, "global")
	project := filepath.Join(configRoot, "project")
	if err := os.MkdirAll(filepath.Join(global, "models"), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(project, 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(global, "settings.json"), []byte(`{"defaultProvider":"test","defaultModel":"test"}`), 0o600); err != nil {
		t.Fatal(err)
	}
	modelConfig := `{"providers":{"test":{"api":"openai-completions","apiKey":"test-key","baseUrl":"http://127.0.0.1:1/v1","models":[{"id":"test","contextWindow":4096,"maxTokens":256}]}}}`
	if err := os.WriteFile(filepath.Join(global, "models", "test.json"), []byte(modelConfig), 0o600); err != nil {
		t.Fatal(err)
	}
	builtinServer := httptest.NewServer(http.HandlerFunc(func(response http.ResponseWriter, _ *http.Request) { _, _ = response.Write([]byte("[]")) }))
	t.Cleanup(builtinServer.Close)
	testExecutable, err := os.Executable()
	if err != nil {
		t.Fatal(err)
	}
	builtinConfig := filepath.Clean(filepath.Join(filepath.Dir(testExecutable), "..", "backend", "config.yaml"))
	if err := os.MkdirAll(filepath.Dir(builtinConfig), 0o700); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = os.Remove(builtinConfig) })
	if err := os.WriteFile(builtinConfig, []byte("BUILTIN_SKILLS_URL: "+builtinServer.URL+"\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	for _, malformed := range []bool{false, true} {
		t.Run(map[bool]string{false: "clean shutdown", true: "malformed input"}[malformed], func(t *testing.T) {
			executable, err := os.Executable()
			if err != nil {
				t.Fatal(err)
			}
			ctx, cancel := context.WithTimeout(t.Context(), 8*time.Second)
			defer cancel()
			cmd := exec.CommandContext(ctx, executable, "-test.run=^TestBackendHelperProcess$")
			cmd.Env = append(os.Environ(), "ROZSA_TEST_BACKEND_PROCESS=1", "ROZSA_CONFIG_DIR="+global, "ROZSA_PROJECT_CONFIG_DIR="+project)
			input, err := cmd.StdinPipe()
			if err != nil {
				t.Fatal(err)
			}
			output, err := cmd.StdoutPipe()
			if err != nil {
				t.Fatal(err)
			}
			var diagnostics bytes.Buffer
			cmd.Stderr = &diagnostics
			if err := cmd.Start(); err != nil {
				t.Fatal(err)
			}
			stream := jsonrpc2.NewStream(processIO{output, input})
			send := func(id int64, method string, params string) {
				t.Helper()
				if _, err := stream.Write(ctx, jsonrpc2.NewCall(jsonrpc2.NewNumberID(id), method, jsonrpc2.RawMessage(params))); err != nil {
					t.Fatal(err)
				}
			}
			read := func() *jsonrpc2.Response {
				t.Helper()
				message, _, err := stream.Read(ctx)
				if err != nil {
					t.Fatal(err)
				}
				response, ok := message.(*jsonrpc2.Response)
				if !ok {
					t.Fatalf("stdout contained %T", message)
				}
				return response
			}
			send(1, "initialize", `{"protocolVersion":1}`)
			if err := read().Err(); err != nil {
				t.Fatal(err)
			}
			send(2, "get_sessions", `{}`)
			if response := read(); response.Err() != nil || string(response.Result()) != "[]" {
				t.Fatalf("get_sessions result = %s, error=%v; want empty configured catalog", response.Result(), response.Err())
			}
			if malformed {
				frames := stream.(interface {
					WriteFrame(context.Context, []byte) (int64, error)
				})
				if _, err := frames.WriteFrame(ctx, []byte(`{`)); err != nil {
					t.Fatal(err)
				}
				if _, _, err := stream.Read(ctx); err != io.EOF {
					t.Fatalf("parser failure wrote a non-protocol stdout message or wire error response: %v", err)
				}
			} else {
				send(3, "shutdown", `{}`)
				if err := read().Err(); err != nil {
					t.Fatal(err)
				}
			}
			input.Close()
			err = cmd.Wait()
			if malformed {
				if err == nil || diagnostics.Len() == 0 {
					t.Fatalf("want exit failure and stderr diagnostic: %v %q", err, diagnostics.String())
				}
			} else if err != nil || diagnostics.Len() != 0 {
				t.Fatalf("clean shutdown: %v %q", err, diagnostics.String())
			}
		})
	}
}
