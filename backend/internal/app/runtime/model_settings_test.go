package runtime

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"rozsa/backend/internal/app/modelregistry"
	"rozsa/backend/internal/app/session"
	"rozsa/backend/internal/core/agent"
	"rozsa/backend/internal/core/hooks"
	"rozsa/backend/internal/model/credentials"
	"rozsa/backend/internal/model/types"
)

type modelSettingsProvider struct{}

type credentialCapture struct {
	key     string
	account string
}

func (capture *credentialCapture) StreamSimple(_ context.Context, _ types.Model, _ types.Context, options types.SimpleStreamOptions, _ func(types.StreamEvent)) error {
	capture.key, capture.account = options.Base.APIKey, options.Base.Headers["x-rozsa-account-id"]
	return nil
}

func TestExistingProviderObservesCredentialRotationAndLogout(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	t.Setenv("ANTHROPIC_API_KEY", "")
	t.Setenv("ANTHROPIC_OAUTH_TOKEN", "")
	t.Setenv("ROZSA_CONFIG_DIR", t.TempDir())
	path := filepath.Join(t.TempDir(), "auth.json")
	current := credentials.OAuthCredential{Access: "old", Refresh: "refresh", Expires: time.Now().Add(time.Hour).UnixMilli()}
	if err := credentials.StoreOAuth(path, "anthropic", current); err != nil {
		t.Fatal(err)
	}
	model := types.Model{ID: "test", Provider: "anthropic", API: types.APIAnthropicMessages}
	app, err := New(Config{SessionDir: t.TempDir(), Model: model, AuthPath: path})
	if err != nil {
		t.Fatal(err)
	}
	defer app.Shutdown(t.Context())
	wrapper := app.provider.(providerWithKey)
	capture := &credentialCapture{}
	wrapper.Provider = capture
	current.Access = "rotated"
	if err := credentials.StoreOAuth(path, "anthropic", current); err != nil {
		t.Fatal(err)
	}
	if err := wrapper.StreamSimple(t.Context(), model, types.Context{}, types.SimpleStreamOptions{}, func(types.StreamEvent) {}); err != nil {
		t.Fatal(err)
	}
	if capture.key != "rotated" {
		t.Fatalf("provider reused stale token: %s", capture.key)
	}
	if _, err := credentials.RemoveStored(path, "anthropic"); err != nil {
		t.Fatal(err)
	}
	if err := wrapper.StreamSimple(t.Context(), model, types.Context{}, types.SimpleStreamOptions{}, func(types.StreamEvent) {}); err == nil {
		t.Fatal("logged-out provider reused a token")
	}
}

func (modelSettingsProvider) StreamSimple(_ context.Context, _ types.Model, _ types.Context, _ types.SimpleStreamOptions, _ func(types.StreamEvent)) error {
	return nil
}

var _ agent.Provider = modelSettingsProvider{}

type namingProvider struct{}

func (namingProvider) StreamSimple(_ context.Context, _ types.Model, input types.Context, options types.SimpleStreamOptions, emit func(types.StreamEvent)) error {
	if len(input.Messages) != 1 || len(input.Tools) != 0 || options.Reasoning == nil || *options.Reasoning != types.ThinkingLow {
		return errors.New("session naming request was not isolated")
	}
	emit(types.TextDelta{Delta: "Generated title"})
	return nil
}

func TestSetModelPreparesProviderAndLeavesCurrentModelOnFailure(t *testing.T) {
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
	provider, ok := app.provider.(providerWithKey)
	if !ok || provider.key != "test-api-key" {
		t.Fatalf("selected model provider credentials: %#v", app.provider)
	}
	if app.thinkingEffort != types.ThinkingOff {
		t.Fatalf("unsupported thinking effort was not normalized: %s", app.thinkingEffort)
	}
	if effort, err := app.SetThinkingEffort(types.ThinkingMedium); err != nil || effort != types.ThinkingOff {
		t.Fatalf("unsupported active thinking update: effort=%s err=%v", effort, err)
	}
}

func TestBlockImagesRemovesImageBlocksFromModelInput(t *testing.T) {
	text := types.TextBlock{Text: "keep this"}
	imageBlock := types.ImageBlock{Data: "image-data", MimeType: "image/png"}
	input := []hooks.Message{{Kind: "standard", Standard: types.UserMessage{Content: types.UserBlocks{text, imageBlock}}}}
	filtered := withoutImages(input)
	user, ok := filtered[0].Standard.(types.UserMessage)
	if !ok {
		t.Fatalf("filtered message type: %T", filtered[0].Standard)
	}
	blocks, ok := user.Content.(types.UserBlocks)
	if !ok || len(blocks) != 1 || blocks[0] != text {
		t.Fatalf("image remained in model input: %#v", user.Content)
	}
	original := input[0].Standard.(types.UserMessage).Content.(types.UserBlocks)
	if len(original) != 2 {
		t.Fatal("filter mutated persisted conversation context")
	}
}

func TestGenerateSessionNameUsesSmallModelAndPreservesManualRename(t *testing.T) {
	root := t.TempDir()
	modelsDir := filepath.Join(root, "models")
	if err := os.MkdirAll(modelsDir, 0o700); err != nil {
		t.Fatal(err)
	}
	modelConfig := `{"providers":{"synthetic":{"api":"anthropic-messages","apiKey":"key","baseUrl":"https://example.invalid","models":[{"id":"title-model","name":"Title model"}]}}}`
	if err := os.WriteFile(filepath.Join(modelsDir, "models.json"), []byte(modelConfig), 0o600); err != nil {
		t.Fatal(err)
	}
	registry, diagnostics, err := modelregistry.Load([]byte("{}"), modelsDir)
	if err != nil || len(diagnostics) != 0 {
		t.Fatalf("load naming model: diagnostics=%#v err=%v", diagnostics, err)
	}
	model, ok := registry.Find("synthetic", "title-model")
	if !ok {
		t.Fatal("naming model missing")
	}
	app, err := New(Config{SessionDir: filepath.Join(root, "sessions"), Model: model, ModelRegistry: registry, Provider: namingProvider{}})
	if err != nil {
		t.Fatal(err)
	}
	defer app.Shutdown(t.Context())

	if err := app.generateSessionName(t.Context(), app.session, "A very long coding task that requires a title", map[string]any{"smallModel": "title-model"}); err != nil {
		t.Fatal(err)
	}
	if name := app.session.CurrentName(); name == nil || *name != "Generated title" {
		t.Fatalf("generated name: %v", name)
	}

	if _, err := app.session.AppendSessionInfo(stringPointer("manual rename")); err != nil {
		t.Fatal(err)
	}
	if err := app.generateSessionName(t.Context(), app.session, "Another long task that should never replace the manual title", map[string]any{"smallModel": "title-model"}); err != nil {
		t.Fatal(err)
	}
	if name := app.session.CurrentName(); name == nil || *name != "manual rename" {
		t.Fatalf("manual name was replaced: %v", name)
	}
}

func TestAutoCompactionUsesConfiguredRatiosAndRestoresSummaryContext(t *testing.T) {
	model := types.Model{ID: "model", Provider: "provider", API: types.APIAnthropicMessages, ContextWindow: 100, MaxTokens: 64}
	app, err := New(Config{SessionDir: t.TempDir(), Model: model, Provider: namingProvider{}})
	if err != nil {
		t.Fatal(err)
	}
	defer app.Shutdown(t.Context())
	for index := 0; index < 6; index++ {
		if _, err := app.session.AppendMessage(session.UserMessage{Content: session.UserText(strings.Repeat("earlier detail ", 8))}); err != nil {
			t.Fatal(err)
		}
	}
	if _, err := app.session.AppendMessage(session.AssistantMessage{Provider: "provider", Model: "model", Usage: session.Usage{Input: 90, Output: 2, TotalTokens: 92}}); err != nil {
		t.Fatal(err)
	}
	conversation := app.conversation(app.session)
	settings := map[string]any{"compaction": map[string]any{"enabled": true, "triggerRatio": 0.5, "targetRatio": 0.2}}
	if err := app.compactIfNeeded(t.Context(), app.session, conversation, model, namingProvider{}, settings); err != nil {
		t.Fatal(err)
	}
	var compacted *session.Compaction
	for _, entry := range app.session.Entries() {
		if value, ok := entry.Payload.(session.Compaction); ok {
			compacted = &value
		}
	}
	if compacted == nil || compacted.Summary != "Generated title" || compacted.TokensBefore < 90 {
		t.Fatalf("compaction entry: %#v", compacted)
	}
	if current := conversation.contextMessages(); len(current) == 0 {
		t.Fatal("compaction removed the summary from live context")
	}
	reopened, err := session.OpenManager(app.session.Path())
	if err != nil {
		t.Fatal(err)
	}
	restored := newConversation(reopened).contextMessages()
	if len(restored) == 0 {
		t.Fatal("reopened context is empty")
	}
	first, ok := restored[0].(session.UserMessage)
	text, textOK := first.Content.(session.UserText)
	if !ok || !textOK || text != "Generated title" {
		t.Fatalf("summary missing after reopen: %#v", restored[0])
	}
}
