package runtime_test

import (
	"context"
	"errors"
	"io"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"

	"rozsa/backend/internal/app/modelregistry"
	"rozsa/backend/internal/app/session"
	"rozsa/backend/internal/core/agent"
	"rozsa/backend/internal/model/credentials"
	"rozsa/backend/internal/model/types"
)

type modelSettingsProvider struct{}

func TestExistingProviderObservesCredentialRotationAndLogout(t *testing.T) {
	useProjectTestCWD(t)
	t.Setenv("HOME", t.TempDir())
	t.Setenv("ANTHROPIC_API_KEY", "")
	t.Setenv("ANTHROPIC_OAUTH_TOKEN", "")
	t.Setenv("ROZSA_CONFIG_DIR", t.TempDir())
	path := filepath.Join(t.TempDir(), "auth.json")
	current := credentials.OAuthCredential{Access: "sk-ant-oat-old", Refresh: "refresh", Expires: time.Now().Add(time.Hour).UnixMilli()}
	if err := credentials.StoreOAuth(path, "anthropic", current); err != nil {
		t.Fatal(err)
	}
	capture := &credentialCapture{}
	server := httptest.NewServer(http.HandlerFunc(func(response http.ResponseWriter, request *http.Request) {
		capture.add(request.Header.Get("authorization"))
		response.Header().Set("Content-Type", "text/event-stream")
		_, _ = io.WriteString(response, "event: message_start\ndata: {\"message\":{\"id\":\"message\",\"usage\":{\"input_tokens\":1,\"output_tokens\":0}}}\n\nevent: content_block_start\ndata: {\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\nevent: content_block_delta\ndata: {\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"ok\"}}\n\nevent: content_block_stop\ndata: {\"index\":0}\n\nevent: message_delta\ndata: {\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":1}}\n\nevent: message_stop\ndata: {}\n\n")
	}))
	defer server.Close()
	model := types.Model{ID: "test", Provider: "anthropic", API: types.APIAnthropicMessages, BaseURL: server.URL, MaxTokens: 32}
	app, err := New(Config{SessionDir: t.TempDir(), Model: model, AuthPath: path, ResolvedSettings: func() map[string]any { return map[string]any{"autoSessionNaming": false} }})
	if err != nil {
		t.Fatal(err)
	}
	defer app.Shutdown(t.Context())
	if err := app.Prompt(t.Context(), "first"); err != nil {
		t.Fatal(err)
	}
	current.Access = "sk-ant-oat-rotated"
	if err := credentials.StoreOAuth(path, "anthropic", current); err != nil {
		t.Fatal(err)
	}
	if err := app.Prompt(t.Context(), "second"); err != nil {
		t.Fatal(err)
	}
	if _, err := credentials.RemoveStored(path, "anthropic"); err != nil {
		t.Fatal(err)
	}
	if err := app.Prompt(t.Context(), "after logout"); err == nil {
		t.Fatal("logged-out provider sent a request")
	}
	if got := capture.values(); len(got) != 2 || got[0] != "Bearer sk-ant-oat-old" || got[1] != "Bearer sk-ant-oat-rotated" {
		t.Fatalf("request credentials = %v", got)
	}
}

type credentialCapture struct {
	mu      sync.Mutex
	apiKeys []string
}

func (capture *credentialCapture) add(value string) {
	capture.mu.Lock()
	capture.apiKeys = append(capture.apiKeys, value)
	capture.mu.Unlock()
}

func (capture *credentialCapture) values() []string {
	capture.mu.Lock()
	defer capture.mu.Unlock()
	return append([]string(nil), capture.apiKeys...)
}

func (modelSettingsProvider) StreamSimple(_ context.Context, _ types.Model, _ types.Context, _ types.SimpleStreamOptions, _ func(types.StreamEvent)) error {
	return nil
}

var _ agent.Provider = modelSettingsProvider{}

type namingProvider struct {
	mu       sync.Mutex
	contexts []types.Context
}

func (provider *namingProvider) StreamSimple(_ context.Context, model types.Model, input types.Context, options types.SimpleStreamOptions, emit func(types.StreamEvent)) error {
	provider.mu.Lock()
	provider.contexts = append(provider.contexts, input)
	provider.mu.Unlock()
	if input.SystemPrompt != nil && strings.HasPrefix(*input.SystemPrompt, "Create a concise session title") {
		if len(input.Messages) != 1 || len(input.Tools) != 0 || options.Reasoning == nil || *options.Reasoning != types.ThinkingLow {
			return errors.New("session naming request was not isolated")
		}
		emit(types.TextDelta{Delta: "Generated title"})
		return nil
	}
	if input.SystemPrompt != nil && strings.HasPrefix(*input.SystemPrompt, "Summarize the earlier conversation") {
		emit(types.TextDelta{Delta: "Generated summary"})
		return nil
	}
	emit(types.Done{Reason: types.Stop, Message: types.AssistantMessage{API: model.API, Provider: model.Provider, Model: model.ID, StopReason: types.Stop, Usage: types.Usage{Input: 90, Output: 2, TotalTokens: 92}, Content: []types.ContentBlock{types.TextBlock{Text: "answer"}}}})
	return nil
}

func (provider *namingProvider) capturedContexts() []types.Context {
	provider.mu.Lock()
	defer provider.mu.Unlock()
	return append([]types.Context(nil), provider.contexts...)
}

func TestSetModelPreparesProviderAndLeavesCurrentModelOnFailure(t *testing.T) {
	useProjectTestCWD(t)
	initial := types.Model{ID: "before", Provider: "custom", API: types.APIAnthropicMessages}
	app, err := New(Config{SessionDir: t.TempDir(), Model: initial, Provider: modelSettingsProvider{}})
	if err != nil {
		t.Fatal(err)
	}
	defer app.Shutdown(t.Context())

	if err := app.SetModel(types.Model{ID: "unsupported", Provider: "custom", API: "unsupported"}, "key", types.ThinkingOff); err == nil {
		t.Fatal("unsupported API was accepted")
	}
	if got := app.Model(); got.ID != initial.ID || got.Provider != initial.Provider || got.API != initial.API {
		t.Fatalf("failed selection changed model: %#v", got)
	}

	selected := types.Model{ID: "after", Provider: "custom", API: types.APIAnthropicMessages, BaseURL: "https://example.invalid"}
	if err := app.SetModel(selected, "test-api-key", types.ThinkingHigh); err != nil {
		t.Fatalf("select model: %v", err)
	}
	if got := app.Model(); got.ID != selected.ID || got.Provider != selected.Provider || got.API != selected.API {
		t.Fatalf("selected model was not applied: %#v", got)
	}
	if effort, err := app.SetThinkingEffort(types.ThinkingMedium); err != nil || effort != types.ThinkingOff {
		t.Fatalf("unsupported active thinking update: effort=%s err=%v", effort, err)
	}
}

type blockImagesProvider struct{ input types.Context }

func (provider *blockImagesProvider) StreamSimple(_ context.Context, model types.Model, input types.Context, _ types.SimpleStreamOptions, emit func(types.StreamEvent)) error {
	provider.input = input
	emit(types.Done{Reason: types.Stop, Message: types.AssistantMessage{API: model.API, Provider: model.Provider, Model: model.ID, StopReason: types.Stop, Content: []types.ContentBlock{types.TextBlock{Text: "complete"}}}})
	return nil
}

func TestBlockImagesRemovesImageBlocksFromProviderInputAndKeepsSessionImage(t *testing.T) {
	useProjectTestCWD(t)
	workingDirectory, err := os.Getwd()
	if err != nil {
		t.Fatal(err)
	}
	path := filepath.Join(t.TempDir(), "images.jsonl")
	copyTestFixture(t, filepath.Join(workingDirectory, "sessions", "images.jsonl"), path)
	directory := filepath.Dir(path)
	text := types.TextBlock{Text: "keep this"}
	imageBlock := types.ImageBlock{Data: "image-data", MimeType: "image/png"}
	provider := &blockImagesProvider{}
	app, err := New(Config{SessionDir: directory, Model: types.Model{ID: "model", Provider: "provider", API: types.APIAnthropicMessages, Input: []string{"text", "image"}}, Provider: provider, ResolvedSettings: func() map[string]any { return map[string]any{"blockImages": true, "autoSessionNaming": false} }})
	if err != nil {
		t.Fatal(err)
	}
	if err := app.Continue(t.Context()); err != nil {
		t.Fatal(err)
	}
	if len(provider.input.Messages) == 0 {
		t.Fatalf("Continue sent no messages to the provider; persisted session = %#v", activeSession(t, app).ContextMessages())
	}
	user, ok := provider.input.Messages[0].(types.UserMessage)
	if !ok {
		t.Fatalf("provider message type: %T", provider.input.Messages[0])
	}
	blocks, ok := user.Content.(types.UserBlocks)
	if !ok || len(blocks) != 1 || blocks[0] != text {
		t.Fatalf("image remained in model input: %#v", user.Content)
	}
	persisted := activeSession(t, app).ContextMessages()[0].(session.UserMessage).Content.(session.UserBlocks)
	if len(persisted) != 2 || persisted[1] != (session.ImageBlock{Data: imageBlock.Data, MimeType: imageBlock.MimeType}) {
		t.Fatalf("provider filter changed persisted image: %#v", persisted)
	}
}

func TestGenerateSessionNameUsesSmallModelAndPreservesManualRename(t *testing.T) {
	workingDirectory := useProjectTestCWD(t)
	repositoryRoot := filepath.Clean(filepath.Join(workingDirectory, "../../.."))
	modelsDirectory := filepath.Join(repositoryRoot, "testdata", "rozsa-config-dev", "models")
	registry, diagnostics, err := modelregistry.Load([]byte("{}"), modelsDirectory)
	if err != nil || len(diagnostics) != 0 {
		t.Fatalf("load naming model: diagnostics=%#v err=%v", diagnostics, err)
	}
	model, ok := registry.Find("synthetic", "title-model")
	if !ok {
		t.Fatal("naming model missing")
	}
	settings := func() map[string]any { return map[string]any{"smallModel": "title-model", "autoSessionNaming": true} }
	provider := &namingProvider{}
	app, err := New(Config{SessionDir: t.TempDir(), Model: model, ModelRegistry: registry, Provider: provider, ResolvedSettings: settings})
	if err != nil {
		t.Fatal(err)
	}
	defer app.Shutdown(t.Context())

	if err := app.Prompt(t.Context(), "A very long coding task that requires a title"); err != nil {
		t.Fatal(err)
	}
	if name := activeSession(t, app).CurrentName(); name == nil || *name != "Generated title" {
		t.Fatalf("generated name: %v", name)
	}
	if err := app.Shutdown(t.Context()); err != nil {
		t.Fatal(err)
	}
	manualPath := filepath.Join(t.TempDir(), "manual.jsonl")
	copyTestFixture(t, filepath.Join(workingDirectory, "sessions", "manual", "manual.jsonl"), manualPath)
	manualDir := filepath.Dir(manualPath)
	manualName := "manual rename"
	manualApp, err := New(Config{SessionDir: manualDir, Model: model, ModelRegistry: registry, Provider: &namingProvider{}, ResolvedSettings: settings})
	if err != nil {
		t.Fatal(err)
	}
	defer manualApp.Shutdown(t.Context())
	if err := manualApp.Prompt(t.Context(), "Another long task that should never replace the manual title"); err != nil {
		t.Fatal(err)
	}
	if name := activeSession(t, manualApp).CurrentName(); name == nil || *name != manualName {
		t.Fatalf("manual name was replaced: %v", name)
	}
}

func TestAutoCompactionUsesConfiguredRatiosAndRestoresSummaryContext(t *testing.T) {
	workingDirectory := useProjectTestCWD(t)
	model := types.Model{ID: "model", Provider: "provider", API: types.APIAnthropicMessages, ContextWindow: 100, MaxTokens: 64}
	path := filepath.Join(t.TempDir(), "compaction.jsonl")
	copyTestFixture(t, filepath.Join(workingDirectory, "sessions", "compaction.jsonl"), path)
	directory := filepath.Dir(path)
	provider := &namingProvider{}
	resolvedSettings := func() map[string]any {
		return map[string]any{"autoSessionNaming": false, "compaction": map[string]any{"enabled": true, "triggerRatio": 0.5, "targetRatio": 0.2}}
	}
	app, err := New(Config{SessionDir: directory, Model: model, Provider: provider, ResolvedSettings: resolvedSettings})
	if err != nil {
		t.Fatal(err)
	}
	appClosed := false
	t.Cleanup(func() {
		if appClosed {
			return
		}
		ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
		defer cancel()
		if err := app.Shutdown(ctx); err != nil {
			t.Errorf("shutdown initial runtime: %v", err)
		}
	})
	if err := app.Continue(t.Context()); err != nil {
		t.Fatal(err)
	}
	active := activeSession(t, app)
	var compacted *session.Compaction
	for _, entry := range active.Entries() {
		if value, ok := entry.Payload.(session.Compaction); ok {
			compacted = &value
		}
	}
	if compacted == nil || compacted.Summary != "Generated summary" || compacted.TokensBefore < 90 {
		t.Fatalf("compaction entry: %#v", compacted)
	}
	if err := app.Shutdown(t.Context()); err != nil {
		t.Fatal(err)
	}
	appClosed = true

	resumedApp, err := New(Config{SessionDir: directory, Model: model, Provider: provider, ResolvedSettings: resolvedSettings})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() {
		ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
		defer cancel()
		if err := resumedApp.Shutdown(ctx); err != nil {
			t.Errorf("shutdown resumed runtime: %v", err)
		}
	})
	if err := resumedApp.Prompt(t.Context(), "continue after compaction"); err != nil {
		t.Fatal(err)
	}
	var promptContexts []types.Context
	for _, input := range provider.capturedContexts() {
		if input.SystemPrompt != nil && strings.HasPrefix(*input.SystemPrompt, "Summarize the earlier conversation") {
			continue
		}
		promptContexts = append(promptContexts, input)
	}
	if len(promptContexts) != 2 {
		t.Fatalf("main provider calls = %d, want pre- and post-compaction turns", len(promptContexts))
	}
	resumed, ok := promptContexts[1].Messages[0].(types.UserMessage)
	text, textOK := resumed.Content.(types.UserText)
	if !ok || !textOK || text != "Generated summary" {
		t.Fatalf("summary missing from resumed provider context: %#v", promptContexts[1].Messages)
	}
}
