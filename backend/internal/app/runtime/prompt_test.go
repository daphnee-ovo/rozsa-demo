package runtime_test

import (
	"context"
	"errors"
	"io"
	"os"
	"path/filepath"
	goruntime "runtime"
	"strings"
	"testing"
	"time"

	"rozsa/backend/internal/app/session"
	"rozsa/backend/internal/core/agent"
	"rozsa/backend/internal/model/types"
)

type promptProvider struct {
	started chan struct{}
	ended   chan struct{}
	block   bool
}

func (provider *promptProvider) StreamSimple(ctx context.Context, model types.Model, _ types.Context, _ types.SimpleStreamOptions, emit func(types.StreamEvent)) error {
	defer close(provider.ended)
	message := types.AssistantMessage{API: model.API, Provider: model.Provider, Model: model.ID, Timestamp: time.Now().UnixMilli(), Content: []types.ContentBlock{}}
	emit(types.Start{Partial: message})
	emit(types.TextStart{ContentIndex: 0, Partial: message})
	partial := message
	partial.Content = []types.ContentBlock{types.TextBlock{Text: "partial"}}
	emit(types.TextDelta{ContentIndex: 0, Delta: "partial", Partial: partial})
	close(provider.started)
	if provider.block {
		<-ctx.Done()
		return ctx.Err()
	}
	final := partial
	final.StopReason = types.Stop
	emit(types.Done{Reason: types.Stop, Message: final})
	return nil
}

type rejectingWriter struct{ err error }

func (writer rejectingWriter) Write([]byte) (int, error) { return 0, writer.err }

func newTestRuntime(t *testing.T, provider agent.Provider, output io.Writer) *Runtime {
	t.Helper()
	return newTestRuntimeAt(t, t.TempDir(), provider, output)
}

func useProjectTestCWD(t *testing.T) string {
	t.Helper()
	previous, err := os.Getwd()
	if err != nil {
		t.Fatal(err)
	}
	_, sourceFile, _, ok := goruntime.Caller(0)
	if !ok {
		t.Fatal("could not locate runtime test source")
	}
	repositoryRoot := filepath.Clean(filepath.Join(filepath.Dir(sourceFile), "../../../../"))
	name := projectTestDirName(t.Name())
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

func projectTestDirName(testName string) string {
	return strings.Map(func(character rune) rune {
		if character >= 'a' && character <= 'z' || character >= 'A' && character <= 'Z' || character >= '0' && character <= '9' || character == '-' || character == '_' {
			return character
		}
		return '-'
	}, testName)
}

func copyTestFixture(t *testing.T, sourcePath, targetPath string) {
	t.Helper()
	data, err := os.ReadFile(sourcePath)
	if err != nil {
		t.Fatalf("read fixture %s: %v", sourcePath, err)
	}
	if err := os.MkdirAll(filepath.Dir(targetPath), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(targetPath, data, 0o600); err != nil {
		t.Fatal(err)
	}
}

func newTestRuntimeAt(t *testing.T, sessionDir string, provider agent.Provider, output io.Writer) *Runtime {
	t.Helper()
	useProjectTestCWD(t)
	app, err := New(Config{SessionDir: sessionDir, Model: types.Model{ID: "custom-model", Provider: "custom-anthropic", API: types.APIAnthropicMessages, BaseURL: "http://127.0.0.1"}, Provider: provider, Output: output})
	if err != nil {
		t.Fatal(err)
	}
	return app
}

func TestPromptDrainsProviderAndPersistsAfterOutputWriteFailure(t *testing.T) {
	writeError := errors.New("stdout closed")
	provider := &promptProvider{started: make(chan struct{}), ended: make(chan struct{})}
	app := newTestRuntime(t, provider, rejectingWriter{err: writeError})
	err := app.Prompt(context.Background(), "hello")
	if !errors.Is(err, writeError) {
		t.Fatalf("Prompt error = %v, want writer error", err)
	}
	select {
	case <-provider.ended:
	case <-time.After(time.Second):
		t.Fatal("provider goroutine did not finish")
	}
	manager := activeSession(t, app)
	messages := manager.ContextMessages()
	if len(messages) != 2 {
		t.Fatalf("persisted messages = %d, want user and assistant: %#v", len(messages), messages)
	}
	if !strings.Contains(manager.Path(), ".jsonl") {
		t.Fatalf("session path = %q", manager.Path())
	}
}

func TestPromptUsesInjectedProviderForUnregisteredAPI(t *testing.T) {
	useProjectTestCWD(t)
	provider := &promptProvider{started: make(chan struct{}), ended: make(chan struct{})}
	customAPI := types.API("custom-api")
	app, err := New(Config{
		SessionDir: t.TempDir(),
		Model:      types.Model{ID: "custom-model", Provider: "custom-provider", API: customAPI},
		Provider:   provider,
		Output:     io.Discard,
	})
	if err != nil {
		t.Fatal(err)
	}
	if err := app.Prompt(context.Background(), "hello custom API"); err != nil {
		t.Fatal(err)
	}
	messages := activeSession(t, app).ContextMessages()
	if len(messages) != 2 {
		t.Fatalf("persisted messages = %d, want user and assistant: %#v", len(messages), messages)
	}
	assistant, ok := messages[1].(session.AssistantMessage)
	if !ok || assistant.API != string(customAPI) || assistant.Provider != "custom-provider" {
		t.Fatalf("assistant = %#v, want injected API and configured provider identity", messages[1])
	}
}

func TestNewRejectsUnregisteredAPIWithoutInjectedProvider(t *testing.T) {
	useProjectTestCWD(t)
	_, err := New(Config{
		SessionDir: t.TempDir(),
		Model:      types.Model{ID: "custom-model", Provider: "custom-provider", API: types.API("custom-api")},
	})
	if err == nil || err.Error() != `unsupported model API "custom-api"` {
		t.Fatalf("New() error = %v, want explicit unsupported API diagnostic", err)
	}
}

func TestPromptCancellationWaitsForProviderAndPersistsAbortedAssistant(t *testing.T) {
	provider := &promptProvider{started: make(chan struct{}), ended: make(chan struct{}), block: true}
	app := newTestRuntime(t, provider, io.Discard)
	ctx, cancel := context.WithCancel(context.Background())
	result := make(chan error, 1)
	go func() { result <- app.Prompt(ctx, "hello") }()
	select {
	case <-provider.started:
	case <-time.After(time.Second):
		t.Fatal("provider did not start")
	}
	cancel()
	select {
	case err := <-result:
		if !errors.Is(err, context.Canceled) {
			t.Fatalf("Prompt error = %v, want context cancellation", err)
		}
	case <-time.After(time.Second):
		t.Fatal("Prompt did not return after cancellation")
	}
	select {
	case <-provider.ended:
	case <-time.After(time.Second):
		t.Fatal("provider goroutine leaked after cancellation")
	}
	messages := activeSession(t, app).ContextMessages()
	if len(messages) != 2 {
		t.Fatalf("persisted messages = %d, want user and aborted assistant", len(messages))
	}
	assistant, ok := messages[1].(session.AssistantMessage)
	if !ok || assistant.StopReason != session.Aborted {
		t.Fatalf("last message = %#v, want aborted assistant", messages[1])
	}
}
