package main_test

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net"
	"net/http"
	"net/http/httptest"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
	"time"

	"go.lsp.dev/jsonrpc2"
	"rozsa/backend/internal/app/session"
	"rozsa/backend/internal/platform/configpaths"
	transporttypes "rozsa/backend/internal/transport/types"
)

type processIO struct {
	io.Reader
	io.Writer
}

func (processIO) Close() error { return nil }

func useProjectTestCWD(t *testing.T) string {
	t.Helper()
	previous, err := os.Getwd()
	if err != nil {
		t.Fatal(err)
	}
	_, sourceFile, _, ok := runtime.Caller(0)
	if !ok {
		t.Fatal("could not locate process test source")
	}
	repositoryRoot := filepath.Clean(filepath.Join(filepath.Dir(sourceFile), "../../.."))
	name := strings.NewReplacer("/", "-", " ", "-", ":", "-").Replace(t.Name())
	workingDirectory := filepath.Join(repositoryRoot, "testdata", "project-test-dirs", name)
	if err := os.Chdir(workingDirectory); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() {
		if err := os.Chdir(previous); err != nil {
			t.Error(err)
		}
	})
	return workingDirectory
}

func sharedConfigRoot(t *testing.T) string {
	t.Helper()
	_, sourceFile, _, ok := runtime.Caller(0)
	if !ok {
		t.Fatal("could not locate process test source")
	}
	repositoryRoot := filepath.Clean(filepath.Join(filepath.Dir(sourceFile), "../../.."))
	return filepath.Join(repositoryRoot, "testdata", "rozsa-config-dev")
}

func isolatedConfigRoot(t *testing.T, cwd string, sessionIDs ...string) string {
	t.Helper()
	sourceRoot := sharedConfigRoot(t)
	configRoot := t.TempDir()
	for _, relativePath := range []string{
		"settings.json",
		"models/synthetic.json",
		"skills/.builtin/rozsa-docs/SKILL.md",
	} {
		sourcePath := filepath.Join(sourceRoot, relativePath)
		targetPath := filepath.Join(configRoot, relativePath)
		data, err := os.ReadFile(sourcePath)
		if err != nil {
			t.Fatalf("read shared config fixture %s: %v", sourcePath, err)
		}
		if err := os.MkdirAll(filepath.Dir(targetPath), 0o700); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(targetPath, data, 0o600); err != nil {
			t.Fatal(err)
		}
	}
	if len(sessionIDs) == 0 {
		return configRoot
	}
	sourceSessions := filepath.Join(sourceRoot, "sessions", configpaths.ProjectKey(cwd))
	targetSessions := filepath.Join(configRoot, "sessions", configpaths.ProjectKey(cwd))
	for _, id := range sessionIDs {
		sourcePath := filepath.Join(sourceSessions, id+".jsonl")
		targetPath := filepath.Join(targetSessions, id+".jsonl")
		data, err := os.ReadFile(sourcePath)
		if err != nil {
			t.Fatalf("read shared session fixture %s: %v", sourcePath, err)
		}
		if err := os.MkdirAll(filepath.Dir(targetPath), 0o700); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(targetPath, data, 0o600); err != nil {
			t.Fatal(err)
		}
	}
	return configRoot
}

func backendExecutable(t *testing.T) string {
	t.Helper()
	path := os.Getenv("ROZSA_BACKEND_EXECUTABLE")
	if path == "" {
		t.Fatal("ROZSA_BACKEND_EXECUTABLE must point to the backend built by node desktop/scripts/backend.mjs")
	}
	absolutePath, err := filepath.Abs(path)
	if err != nil {
		t.Fatal(err)
	}
	if info, err := os.Stat(absolutePath); err != nil || info.IsDir() {
		t.Fatalf("backend executable %q is unavailable: %v", absolutePath, err)
	}
	return absolutePath
}

func startBackendProcess(t *testing.T, ctx context.Context, cwd, configRoot string) (*exec.Cmd, io.WriteCloser, io.ReadCloser, *bytes.Buffer) {
	t.Helper()
	command := exec.CommandContext(ctx, backendExecutable(t))
	command.Dir = cwd
	command.Env = append(os.Environ(), "ROZSA_CONFIG_DIR="+configRoot, "ROZSA_PROJECT_CONFIG_DIR="+filepath.Join(cwd, ".rozsa"))
	input, err := command.StdinPipe()
	if err != nil {
		t.Fatal(err)
	}
	output, err := command.StdoutPipe()
	if err != nil {
		t.Fatal(err)
	}
	var diagnostics bytes.Buffer
	command.Stderr = &diagnostics
	if err := command.Start(); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = input.Close(); _ = command.Process.Kill() })
	return command, input, output, &diagnostics
}

func TestProcessPublishesInitializationFailureWithoutMessageRequest(t *testing.T) {
	cwd := useProjectTestCWD(t)
	configRoot := isolatedConfigRoot(t, cwd)
	ctx, cancel := context.WithTimeout(t.Context(), 8*time.Second)
	defer cancel()
	command, input, output, diagnostics := startBackendProcess(t, ctx, cwd, configRoot)
	stream := jsonrpc2.NewStream(processIO{output, input})
	call := func(id int64, method string, params string) *jsonrpc2.Response {
		t.Helper()
		if _, err := stream.Write(ctx, jsonrpc2.NewCall(jsonrpc2.NewNumberID(id), method, jsonrpc2.RawMessage(params))); err != nil {
			t.Fatal(err)
		}
		message, _, err := stream.Read(ctx)
		if err != nil {
			t.Fatal(err)
		}
		response, ok := message.(*jsonrpc2.Response)
		if !ok {
			t.Fatalf("stdout contained %T", message)
		}
		if response.Err() != nil && method != "runtime_status" {
			t.Fatal(response.Err())
		}
		return response
	}
	if err := call(1, "initialize", `{"protocolVersion":1}`).Err(); err != nil {
		t.Fatal(err)
	}
	deadline := time.NewTimer(5 * time.Second)
	defer deadline.Stop()
	for {
		response := call(2, "runtime_status", `{}`)
		if statusErr := response.Err(); statusErr != nil {
			if !strings.Contains(statusErr.Error(), "configured model synthetic/missing was not found") {
				t.Fatalf("process runtime status error = %v", statusErr)
			}
			break
		}
		var status struct {
			State string `json:"state"`
		}
		if err := json.Unmarshal(response.Result(), &status); err != nil {
			t.Fatal(err)
		}
		if status.State == "ready" {
			t.Fatal("runtime became ready despite a missing configured model")
		}
		select {
		case <-deadline.C:
			t.Fatal("runtime initialization failure was not published")
		case <-time.After(10 * time.Millisecond):
		}
	}
	if err := call(3, "shutdown", `{}`).Err(); err != nil {
		t.Fatal(err)
	}
	if err := input.Close(); err != nil {
		t.Fatal(err)
	}
	if err := command.Wait(); err != nil || diagnostics.Len() != 0 {
		t.Fatalf("process shutdown: %v diagnostics=%q", err, diagnostics.String())
	}
}

func TestProductRPCShutdownDrainsTwoSessionWritesBeforeAcknowledging(t *testing.T) {
	listener, err := net.Listen("tcp", "127.0.0.1:50805")
	if err != nil {
		t.Fatalf("listen on the fixture model port: %v", err)
	}
	modelServer := httptest.NewUnstartedServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		if request.URL.Path != "/v1/chat/completions" || request.Method != http.MethodPost {
			t.Errorf("model request = %s %s", request.Method, request.URL.Path)
		}
		writer.Header().Set("Content-Type", "text/event-stream")
		if _, err := fmt.Fprint(writer, "data: {\"choices\":[{\"delta\":{\"content\":\"partial\"}}]}\n\n"); err != nil {
			t.Errorf("write model event: %v", err)
			return
		}
		writer.(http.Flusher).Flush()
		<-request.Context().Done()
	}))
	modelServer.Listener = listener
	modelServer.Start()
	defer modelServer.Close()

	cwd := useProjectTestCWD(t)
	configRoot := isolatedConfigRoot(t, cwd, "a", "b")
	ctx, cancel := context.WithTimeout(t.Context(), 12*time.Second)
	defer cancel()
	command, input, output, diagnostics := startBackendProcess(t, ctx, cwd, configRoot)
	sessionDirectory := filepath.Join(configRoot, "sessions", configpaths.ProjectKey(cwd))
	sessionPaths := map[string]string{
		"a": filepath.Join(sessionDirectory, "a.jsonl"),
		"b": filepath.Join(sessionDirectory, "b.jsonl"),
	}
	stream := jsonrpc2.NewStream(processIO{output, input})
	nextID := int64(0)
	call := func(method string, params any) *jsonrpc2.Response {
		t.Helper()
		nextID++
		data, err := json.Marshal(params)
		if err != nil {
			t.Fatal(err)
		}
		if _, err := stream.Write(ctx, jsonrpc2.NewCall(jsonrpc2.NewNumberID(nextID), method, jsonrpc2.RawMessage(data))); err != nil {
			t.Fatal(err)
		}
		var response *jsonrpc2.Response
		for response == nil {
			message, _, err := stream.Read(ctx)
			if err != nil {
				t.Fatalf("%s response read: %v; process diagnostics: %s", method, err, diagnostics.String())
			}
			switch message := message.(type) {
			case *jsonrpc2.Response:
				response = message
			case *jsonrpc2.Notification:
			default:
				t.Fatalf("response stream item: %T", message)
			}
		}
		if response.Err() != nil {
			t.Fatalf("%s: %v", method, response.Err())
		}
		return response
	}
	call("initialize", map[string]any{"protocolVersion": 1})
	deadline := time.NewTimer(5 * time.Second)
	defer deadline.Stop()
	for {
		response := call("runtime_status", map[string]any{})
		var status struct {
			State string `json:"state"`
			Error string `json:"error"`
		}
		if err := json.Unmarshal(response.Result(), &status); err != nil {
			t.Fatal(err)
		}
		if status.State == "ready" {
			break
		}
		if status.State == "failed" {
			t.Fatalf("runtime initialization failed: %s", status.Error)
		}
		select {
		case <-deadline.C:
			t.Fatal("runtime did not become ready")
		case <-time.After(10 * time.Millisecond):
		}
	}
	listed := call("get_sessions", map[string]any{})
	var catalog []session.ListEntry
	if err := json.Unmarshal(listed.Result(), &catalog); err != nil || len(catalog) != 2 {
		t.Fatalf("configured session catalog = %+v, err=%v", catalog, err)
	}
	for _, id := range []string{"a", "b"} {
		entryIndex := -1
		for index := range catalog {
			if catalog[index].ID == id && catalog[index].Path == sessionPaths[id] {
				entryIndex = index
				break
			}
		}
		if entryIndex < 0 {
			t.Fatalf("session %s not found in process catalog: %+v", id, catalog)
		}
		call("switch_session", map[string]string{"path": catalog[entryIndex].Path, "catalogId": catalog[entryIndex].ID})
		response := call("start_message", map[string]string{"text": "question " + id})
		var operation transporttypes.MessageOperation
		if err := json.Unmarshal(response.Result(), &operation); err != nil || operation.OperationID == "" {
			t.Fatalf("operation for %s = %+v, err=%v", id, operation, err)
		}
		response = call("subscribe_message_events", map[string]string{"operationId": operation.OperationID})
		var subscription transporttypes.MessageSubscription
		if err := json.Unmarshal(response.Result(), &subscription); err != nil || subscription.SubscriptionID == "" {
			t.Fatalf("subscription for %s = %+v, err=%v", id, subscription, err)
		}
		for {
			response = call("next_message_event", map[string]string{"subscriptionId": subscription.SubscriptionID})
			var event transporttypes.MessageEventResult
			if err := json.Unmarshal(response.Result(), &event); err != nil {
				t.Fatal(err)
			}
			if event.Event.Kind == transporttypes.MessageTextDelta {
				if event.Event.Delta == nil || *event.Event.Delta != "partial" {
					t.Fatalf("operation %s text delta = %v, want partial", operation.OperationID, event.Event.Delta)
				}
				break
			}
			if event.Event.Kind == transporttypes.MessageFailed || event.Event.Kind == transporttypes.MessageCanceled {
				t.Fatalf("operation %s ended before streaming text: %+v", operation.OperationID, event.Event)
			}
		}
	}
	if result := string(call("shutdown", map[string]any{}).Result()); result != "true" {
		t.Fatalf("shutdown result = %s", result)
	}
	for _, id := range []string{"a", "b"} {
		assertAbortedPartialSession(t, sessionPaths[id], id)
	}
	if err := input.Close(); err != nil {
		t.Fatal(err)
	}
	if err := command.Wait(); err != nil || diagnostics.Len() != 0 {
		t.Fatalf("process exit: %v diagnostics=%q", err, diagnostics.String())
	}
}

func TestStdioProcessProtocolAndDiagnostics(t *testing.T) {
	cwd := useProjectTestCWD(t)
	for _, malformed := range []bool{false, true} {
		t.Run(map[bool]string{false: "clean shutdown", true: "malformed input"}[malformed], func(t *testing.T) {
			configRoot := isolatedConfigRoot(t, cwd)
			ctx, cancel := context.WithTimeout(t.Context(), 8*time.Second)
			defer cancel()
			command, input, output, diagnostics := startBackendProcess(t, ctx, cwd, configRoot)
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
				if _, _, err := stream.Read(ctx); !errors.Is(err, io.EOF) {
					t.Fatalf("parser failure wrote a non-protocol stdout message or wire error response: %v", err)
				}
			} else {
				send(3, "shutdown", `{}`)
				if err := read().Err(); err != nil {
					t.Fatal(err)
				}
			}
			if err := input.Close(); err != nil {
				t.Fatal(err)
			}
			err := command.Wait()
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
