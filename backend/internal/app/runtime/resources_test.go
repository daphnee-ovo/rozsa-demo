package runtime_test

import (
	"context"
	"encoding/json"
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

	"rozsa/backend/internal/app/session"
	"rozsa/backend/internal/app/settings"
	"rozsa/backend/internal/app/skills"
	"rozsa/backend/internal/core/agent"
	"rozsa/backend/internal/model/types"
	"rozsa/backend/internal/platform/configpaths"
)

type promptCaptureProvider struct {
	mu       sync.Mutex
	contexts []types.Context
}

func (provider *promptCaptureProvider) StreamSimple(_ context.Context, model types.Model, input types.Context, _ types.SimpleStreamOptions, emit func(types.StreamEvent)) error {
	provider.mu.Lock()
	input.Messages = append([]types.Message(nil), input.Messages...)
	provider.contexts = append(provider.contexts, input)
	provider.mu.Unlock()

	message := types.AssistantMessage{API: model.API, Provider: model.Provider, Model: model.ID, Timestamp: time.Now().UnixMilli(), Content: []types.ContentBlock{types.TextBlock{Text: "ok"}}, StopReason: types.Stop}
	emit(types.Start{Partial: message})
	emit(types.TextStart{ContentIndex: 0, Partial: message})
	emit(types.TextDelta{ContentIndex: 0, Delta: "ok", Partial: message})
	emit(types.Done{Reason: types.Stop, Message: message})
	return nil
}

func (provider *promptCaptureProvider) captured() []types.Context {
	provider.mu.Lock()
	defer provider.mu.Unlock()
	return append([]types.Context(nil), provider.contexts...)
}

func TestProductionRuntimeBuildsLayeredPromptAndReloadsPerSession(t *testing.T) {
	root := useProjectTestCWD(t)
	globalRoot := filepath.Join(root, "global")
	home := filepath.Join(root, "home")
	cwdA := filepath.Join(root, "project-a", "work")

	rootsResolver := func(cwd string) (configpaths.Roots, error) {
		return configpaths.Resolve(cwd, &globalRoot, nil, &home, "darwin")
	}
	rootsA, err := rootsResolver(cwdA)
	if err != nil {
		t.Fatal(err)
	}
	settingsPaths := rootsA.SettingsPaths()
	projectSettingsPath := settingsPaths[1]
	globalSettingsPath := filepath.Join(t.TempDir(), "settings.json")
	copyTestFixture(t, settingsPaths[0], globalSettingsPath)
	settingsManager, err := settings.Load(globalSettingsPath, &projectSettingsPath, nil)
	if err != nil {
		t.Fatal(err)
	}
	builtinServer := newRuntimeBuiltinServer(t)
	skillApplication := skills.NewApplicationWithBuiltinURL(settingsManager, rootsResolver, builtinServer.URL+"/contents/builtin-skills", builtinServer.Client(), nil)

	sessionRoot := t.TempDir()
	writeDirectory, otherDirectory := filepath.Join(sessionRoot, "sessions-a"), filepath.Join(sessionRoot, "sessions-b")
	firstSessionPath := filepath.Join(writeDirectory, "a.jsonl")
	secondSessionPath := filepath.Join(otherDirectory, "b.jsonl")
	copyTestFixture(t, filepath.Join(root, "sessions-a", "a.jsonl"), firstSessionPath)
	copyTestFixture(t, filepath.Join(root, "sessions-b", "b.jsonl"), secondSessionPath)
	provider := &promptCaptureProvider{}
	app, err := New(Config{
		SessionDir: writeDirectory, SessionDirs: []string{writeDirectory, otherDirectory},
		SessionService: session.NewService(session.Catalog{Directories: []string{writeDirectory, otherDirectory}}),
		Model:          types.Model{ID: "test-model", Provider: "test-provider", API: types.APIAnthropicMessages, ContextWindow: 128000, MaxTokens: 4096},
		Provider:       provider, Output: io.Discard, ResolvedSettings: settingsManager.Resolved,
		Skills: skillApplication, ResolveRoots: rootsResolver,
	})
	if err != nil {
		t.Fatal(err)
	}
	if got := app.SessionStatus().ActivePath; got != firstSessionPath {
		t.Fatalf("active session path = %q, want %q", got, firstSessionPath)
	}

	if err := app.Prompt(context.Background(), "Use /skill:shared"); err != nil {
		t.Fatal(err)
	}
	firstContext := provider.captured()[0]
	firstPrompt := promptText(t, firstContext)
	for _, expected := range []string{"project A resource", "project A ancestor", "project A cwd", "project shared", "## Skills"} {
		if !strings.Contains(firstPrompt, expected) {
			t.Errorf("first request system prompt missing %q: %s", expected, firstPrompt)
		}
	}
	for _, absent := range []string{"global resource", "agents shared", "global shared body", "project shared body", "global only"} {
		if strings.Contains(firstPrompt, absent) {
			t.Errorf("first request system prompt unexpectedly contains %q: %s", absent, firstPrompt)
		}
	}
	for _, absent := range []string{"Guidelines:", "{{GUIDELINES}}", "Rózsa docs (read only when", "{{README_PATH}}"} {
		if strings.Contains(firstPrompt, absent) {
			t.Errorf("default system prompt unexpectedly contains moved documentation guidance %q", absent)
		}
	}
	if !strings.Contains(firstPrompt, "rozsa-docs: Use when the user asks about Rózsa itself") {
		t.Fatalf("built-in skill is not discoverable in the normal system prompt: %s", firstPrompt)
	}
	firstUser := lastUserMessage(t, firstContext)
	userText, ok := firstUser.Content.(types.UserText)
	if !ok || !strings.Contains(string(userText), "<content>\nproject shared body\n</content>") || !strings.Contains(string(userText), filepath.Join(cwdA, ".rozsa", "skills", "shared")) {
		t.Fatalf("slash skill body was not injected from the highest-priority source: %#v", firstUser.Content)
	}
	active := activeSession(t, app)
	persisted := active.ContextMessages()
	if len(persisted) == 0 {
		t.Fatal("runtime did not persist the user message")
	}
	persistedUser, ok := persisted[0].(session.UserMessage)
	if !ok || persistedUser.DisplayText == nil || *persistedUser.DisplayText != "Use /skill:shared" {
		t.Fatalf("persisted skill prompt display text = %#v, want original slash command", persisted[0])
	}
	reopened, err := session.OpenManager(active.Path())
	if err != nil {
		t.Fatal(err)
	}
	reopenedMessages := reopened.ContextMessages()
	reopenedUser, ok := reopenedMessages[0].(session.UserMessage)
	if !ok || reopenedUser.DisplayText == nil || *reopenedUser.DisplayText != "Use /skill:shared" {
		t.Fatalf("reopened JSONL display text = %#v, want original slash command", reopenedMessages[0])
	}
	reopenedContent, ok := reopenedUser.Content.(session.UserBlocks)
	if !ok || len(reopenedContent) == 0 {
		t.Fatalf("reopened JSONL lost expanded skill content: %#v", reopenedUser.Content)
	}
	reopenedText, ok := reopenedContent[0].(session.TextBlock)
	if !ok || !strings.Contains(reopenedText.Text, "project shared body") {
		t.Fatalf("reopened JSONL skill text = %#v", reopenedContent[0])
	}

	if err := settingsManager.Set(settings.Global, "skills.global-only", true); err != nil {
		t.Fatal(err)
	}
	if err := app.Prompt(context.Background(), "check changed skills"); err != nil {
		t.Fatal(err)
	}
	secondPrompt := promptText(t, provider.captured()[1])
	if !strings.Contains(secondPrompt, "global only") {
		t.Fatalf("next runtime turn did not apply changed skill settings: %s", secondPrompt)
	}

	if _, err := app.Switch(context.Background(), secondSessionPath, "session-b"); err != nil {
		t.Fatal(err)
	}
	if err := app.Prompt(context.Background(), "Use /skill:b-only"); err != nil {
		t.Fatal(err)
	}
	thirdContext := provider.captured()[2]
	thirdPrompt := promptText(t, thirdContext)
	for _, expected := range []string{"project B resource", "project B cwd", "project B only"} {
		if !strings.Contains(thirdPrompt, expected) {
			t.Errorf("session B system prompt missing %q: %s", expected, thirdPrompt)
		}
	}
	for _, absent := range []string{"project A resource", "project A cwd", "project shared"} {
		if strings.Contains(thirdPrompt, absent) {
			t.Errorf("session B system prompt leaked %q: %s", absent, thirdPrompt)
		}
	}
	thirdUser := lastUserMessage(t, thirdContext)
	thirdText, ok := thirdUser.Content.(types.UserText)
	if !ok || !strings.Contains(string(thirdText), "project B body") {
		t.Fatalf("session B skill was not injected: %#v", thirdUser.Content)
	}

	if err := app.Prompt(context.Background(), "/skill:rozsa-docs"); err != nil {
		t.Fatal(err)
	}
	fourthContext := provider.captured()[3]
	fourthPrompt := promptText(t, fourthContext)
	if strings.Contains(fourthPrompt, "Rózsa docs (read only when") || strings.Contains(fourthPrompt, "{{README_PATH}}") {
		t.Fatal("skill body was copied into the system prompt instead of loaded on demand")
	}
	fourthUser := lastUserMessage(t, fourthContext)
	fourthText, ok := fourthUser.Content.(types.UserText)
	if !ok || !strings.Contains(string(fourthText), "Rózsa docs (read only when") || !strings.Contains(string(fourthText), "{{README_PATH}}") {
		t.Fatalf("on-demand rozsa-docs body was not injected into the user context: %#v", fourthUser.Content)
	}
}

func TestProductionRuntimeReturnsRootResolutionFailureBeforeProviderCall(t *testing.T) {
	root := useProjectTestCWD(t)
	settingsManager, err := settings.Load(filepath.Join(root, "settings.json"), nil, nil)
	if err != nil {
		t.Fatal(err)
	}
	resolveRoots := func(string) (configpaths.Roots, error) { return configpaths.Roots{}, os.ErrPermission }
	provider := &promptCaptureProvider{}
	app, err := New(Config{
		SessionDir: filepath.Join(root, "sessions"),
		Model:      types.Model{ID: "test-model", Provider: "test-provider", API: types.APIAnthropicMessages, ContextWindow: 128000, MaxTokens: 4096},
		Provider:   provider, Output: io.Discard, ResolvedSettings: settingsManager.Resolved,
		Skills: skills.NewApplication(settingsManager, resolveRoots, nil), ResolveRoots: resolveRoots,
	})
	if err != nil {
		t.Fatal(err)
	}
	if err := app.Prompt(context.Background(), "hello"); !errors.Is(err, os.ErrPermission) {
		t.Fatalf("Prompt error = %v, want root resolution failure", err)
	}
	if len(provider.captured()) != 0 {
		t.Fatal("provider was called after prompt root resolution failed")
	}
	if _, err := os.Stat(app.SessionStatus().ActivePath); !errors.Is(err, os.ErrNotExist) {
		t.Fatalf("session file exists despite root resolution failure: %v", err)
	}
}

func newRuntimeBuiltinServer(t *testing.T) *httptest.Server {
	t.Helper()
	var server *httptest.Server
	server = httptest.NewServer(http.HandlerFunc(func(response http.ResponseWriter, request *http.Request) {
		var body any
		switch request.URL.Path {
		case "/contents/builtin-skills":
			body = []map[string]string{{"name": "rozsa-docs", "type": "dir"}}
		case "/contents/builtin-skills/rozsa-docs":
			body = []map[string]string{{"name": "SKILL.md", "type": "file", "download_url": server.URL + "/raw/rozsa-docs/SKILL.md"}}
		case "/raw/rozsa-docs/SKILL.md":
			_, _ = response.Write([]byte("---\nname: rozsa-docs\ndescription: Use when the user asks about Rózsa itself\n---\nRózsa docs (read only when needed): {{README_PATH}}"))
			return
		default:
			http.NotFound(response, request)
			return
		}
		encoded, err := json.Marshal(body)
		if err != nil {
			t.Errorf("encode built-in listing: %v", err)
			return
		}
		_, _ = response.Write(encoded)
	}))
	t.Cleanup(server.Close)
	return server
}

func promptText(t *testing.T, input types.Context) string {
	t.Helper()
	if input.SystemPrompt == nil {
		t.Fatal("production runtime did not provide a system prompt")
	}
	return *input.SystemPrompt
}

func lastUserMessage(t *testing.T, input types.Context) types.UserMessage {
	t.Helper()
	for index := len(input.Messages) - 1; index >= 0; index-- {
		if user, ok := input.Messages[index].(types.UserMessage); ok {
			return user
		}
	}
	t.Fatal("provider request has no user message")
	return types.UserMessage{}
}

var _ agent.Provider = (*promptCaptureProvider)(nil)
