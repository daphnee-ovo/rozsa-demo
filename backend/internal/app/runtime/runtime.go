// Package runtime composes session persistence, credentials, providers and the
// agent loop behind one application owner.
package runtime

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"log"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"sync"
	"time"

	"rozsa/backend/internal/app/compaction"
	"rozsa/backend/internal/app/modelcatalog"
	"rozsa/backend/internal/app/modelregistry"
	"rozsa/backend/internal/app/permissions"
	"rozsa/backend/internal/app/questions"
	"rozsa/backend/internal/app/resources"
	"rozsa/backend/internal/app/session"
	"rozsa/backend/internal/app/sessions"
	"rozsa/backend/internal/app/skills"
	"rozsa/backend/internal/app/subagent"
	"rozsa/backend/internal/core/agent"
	"rozsa/backend/internal/core/hooks"
	"rozsa/backend/internal/model/types"
	"rozsa/backend/internal/platform/configpaths"
)

type Config struct {
	SessionDir       string
	SessionDirs      []string
	SessionService   *session.Service
	ModelRegistry    modelregistry.Reader
	Credentials      modelcatalog.CredentialSource
	Model            types.Model
	ThinkingEffort   types.ThinkingEffort
	ResolvedSettings func() map[string]any
	Skills           skills.Application
	ResolveRoots     func(string) (configpaths.Roots, error)
	APIKey           string
	AuthPath         string
	Provider         agent.Provider
	Output           io.Writer
	LifecycleHooks   sessions.Hooks
}

// Runtime composes providers and persistence with a session lifecycle owner.
type Runtime struct {
	mu               sync.Mutex
	session          *session.Manager
	service          *session.Service
	modelRegistry    modelregistry.Reader
	credentialSource modelcatalog.CredentialSource
	model            types.Model
	thinkingEffort   types.ThinkingEffort
	resolvedSettings func() map[string]any
	skills           skills.Application
	resolveRoots     func(string) (configpaths.Roots, error)
	provider         agent.Provider
	authPath         string
	questions        *questions.Owner
	approvals        *permissions.Owner
	lifecycle        sessions.Owner
	sessionDir       string
	outputMu         sync.Mutex
	output           io.Writer
	revision         uint64
	messages         *messageOperations
	conversations    map[string]*conversation
	closing          bool

	subagents      map[string]subagent.Manager
	subagentEvents map[string][]subagent.Event
}

func New(config Config) (*Runtime, error) {
	if config.SessionDir == "" {
		return nil, errors.New("session directory is required")
	}
	if config.Model.ID == "" || config.Model.Provider == "" || config.Model.API == "" {
		return nil, errors.New("runtime requires a configured model, provider and API")
	}
	if config.Provider == nil {
		if config.Credentials == nil {
			config.Credentials = modelcatalog.NewAuth(config.AuthPath, nil)
		}
		provider, err := prepareModelProviderWithSource(config.Model, config.APIKey, config.Credentials, config.ModelRegistry)
		if err != nil {
			return nil, err
		}
		config.Provider = provider
	}
	if (config.Skills == nil) != (config.ResolveRoots == nil) {
		return nil, errors.New("runtime prompt resources require both skills and config root services")
	}
	if err := os.MkdirAll(config.SessionDir, 0o700); err != nil {
		return nil, fmt.Errorf("create session directory: %w", err)
	}
	manager, err := openLatest(config.SessionDir)
	if err != nil {
		return nil, err
	}
	if config.Output == nil {
		config.Output = io.Discard
	}
	catalogDirectories := config.SessionDirs
	if len(catalogDirectories) == 0 {
		catalogDirectories = []string{config.SessionDir}
	}
	service := config.SessionService
	if service == nil {
		service = session.NewService(session.Catalog{Directories: catalogDirectories})
	}
	if err := service.AdoptManager(manager); err != nil {
		return nil, fmt.Errorf("prepare active session snapshot: %w", err)
	}
	effort := config.ThinkingEffort
	if effort == "" || !config.Model.SupportsThinking(effort) {
		effort = types.ThinkingOff
	}
	runtime := &Runtime{session: manager, service: service, sessionDir: config.SessionDir, model: config.Model, modelRegistry: config.ModelRegistry, credentialSource: config.Credentials, thinkingEffort: effort, resolvedSettings: config.ResolvedSettings, skills: config.Skills, resolveRoots: config.ResolveRoots, provider: config.Provider, authPath: config.AuthPath, output: config.Output, questions: questions.NewOwner(), approvals: permissions.NewOwner(), messages: newMessageOperations()}
	lifecycleHooks := config.LifecycleHooks
	rejectApprovals, cancelQuestions := lifecycleHooks.RejectApprovals, lifecycleHooks.CancelQuestions
	lifecycleHooks.RejectApprovals = func(id string) error {
		runtime.approvals.CloseSession(id)
		if rejectApprovals != nil {
			return rejectApprovals(id)
		}
		return nil
	}
	lifecycleHooks.CancelQuestions = func(id string) error {
		runtime.questions.CancelSession(id)
		if cancelQuestions != nil {
			return cancelQuestions(id)
		}
		return nil
	}
	runtime.lifecycle = sessions.New(lifecycleHooks)
	if err := runtime.lifecycle.Register(manager); err != nil {
		return nil, err
	}
	return runtime, nil
}

// SetModel validates and prepares the provider before replacing the model used
// by subsequent runtime turns. A running turn retains its current snapshot.
func (runtime *Runtime) SetModel(model types.Model, configuredAPIKey string, effort types.ThinkingEffort) error {
	if model.ID == "" || model.Provider == "" || model.API == "" {
		return errors.New("runtime requires a configured model, provider and API")
	}
	if !validThinkingEffort(effort) {
		return fmt.Errorf("invalid thinking effort: %s", effort)
	}
	if !model.SupportsThinking(effort) {
		effort = types.ThinkingOff
	}
	source := runtime.credentialSource
	if source == nil {
		source = modelcatalog.NewAuth(runtime.authPath, nil)
	}
	wrappedProvider, err := prepareModelProviderWithSource(model, configuredAPIKey, source, runtime.modelRegistry)
	if err != nil {
		return err
	}

	runtime.mu.Lock()
	defer runtime.mu.Unlock()
	if runtime.closing {
		return sessions.ErrClosed
	}
	runtime.model = model
	runtime.thinkingEffort = effort
	runtime.provider = wrappedProvider
	return nil
}

func (runtime *Runtime) SetThinkingEffort(effort types.ThinkingEffort) (types.ThinkingEffort, error) {
	if !validThinkingEffort(effort) {
		return "", fmt.Errorf("invalid thinking effort: %s", effort)
	}
	runtime.mu.Lock()
	defer runtime.mu.Unlock()
	if runtime.closing {
		return "", sessions.ErrClosed
	}
	if !runtime.model.SupportsThinking(effort) {
		effort = types.ThinkingOff
	}
	runtime.thinkingEffort = effort
	return effort, nil
}

func (runtime *Runtime) Model() types.Model {
	runtime.mu.Lock()
	defer runtime.mu.Unlock()
	return runtime.model
}

func validThinkingEffort(effort types.ThinkingEffort) bool {
	switch effort {
	case types.ThinkingOff, types.ThinkingLow, types.ThinkingMedium, types.ThinkingHigh, types.ThinkingXHigh, types.ThinkingMax:
		return true
	default:
		return false
	}
}

func (runtime *Runtime) GetSessions(ctx context.Context) ([]session.ListEntry, error) {
	return runtime.service.GetSessions(ctx)
}

func (runtime *Runtime) Switch(ctx context.Context, path, catalogID string) (session.LoadedSnapshot, error) {
	runtime.mu.Lock()
	defer runtime.mu.Unlock()
	if runtime.closing {
		return session.LoadedSnapshot{}, sessions.ErrClosed
	}
	snapshot, err := runtime.service.Switch(ctx, path, catalogID)
	if err != nil {
		return session.LoadedSnapshot{}, err
	}
	manager, err := runtime.service.WritableManager(snapshot.Path, snapshot.CatalogID)
	if err != nil {
		return session.LoadedSnapshot{}, fmt.Errorf("resolve selected session for writing: %w", err)
	}
	if err := runtime.lifecycle.Register(manager); err != nil {
		return session.LoadedSnapshot{}, err
	}
	if err := runtime.lifecycle.Activate(manager.Path()); err != nil {
		return session.LoadedSnapshot{}, err
	}
	runtime.session = manager
	return snapshot, nil
}

func (runtime *Runtime) Subscribe() session.EventSubscription { return runtime.service.Subscribe() }

// QuestionOwner exposes the runtime-owned question queue to the app transport.
// Agent tool executions use this same owner, so responses resolve the request
// created by the production tool call.
func (runtime *Runtime) QuestionOwner() *questions.Owner { return runtime.questions }

func (runtime *Runtime) Snapshot(ctx context.Context) (session.StateSnapshot, error) {
	return runtime.service.Snapshot(ctx)
}

func openLatest(dir string) (*session.Manager, error) {
	entries, err := os.ReadDir(dir)
	if err != nil {
		return nil, fmt.Errorf("read session directory: %w", err)
	}
	var latest string
	var latestTime time.Time
	for _, entry := range entries {
		if entry.IsDir() || filepath.Ext(entry.Name()) != ".jsonl" {
			continue
		}
		info, err := entry.Info()
		if err != nil {
			return nil, fmt.Errorf("stat session %s: %w", entry.Name(), err)
		}
		if latest == "" || info.ModTime().After(latestTime) {
			latest, latestTime = filepath.Join(dir, entry.Name()), info.ModTime()
		}
	}
	if latest != "" {
		return session.OpenManager(latest)
	}
	var raw [16]byte
	if _, err := rand.Read(raw[:]); err != nil {
		return nil, fmt.Errorf("generate session id: %w", err)
	}
	cwd, err := os.Getwd()
	if err != nil {
		return nil, fmt.Errorf("get working directory: %w", err)
	}
	id := hex.EncodeToString(raw[:])
	return session.CreateLazy(filepath.Join(dir, id+".jsonl"), id, cwd, nil), nil
}

func (runtime *Runtime) Prompt(ctx context.Context, text string) error {
	if text == "" {
		return errors.New("prompt must not be empty")
	}
	return runtime.run(ctx, &text)
}

func (runtime *Runtime) Continue(ctx context.Context) error { return runtime.run(ctx, nil) }

var _ session.ApplicationService = (*Runtime)(nil)

func (runtime *Runtime) run(ctx context.Context, prompt *string) error {
	execution, err := runtime.beginRun(ctx, "")
	if err != nil {
		return err
	}
	return runtime.runExecution(prompt, execution, nil)
}

func (runtime *Runtime) beginRun(ctx context.Context, operationID string) (sessions.Execution, error) {
	runtime.mu.Lock()
	defer runtime.mu.Unlock()
	if runtime.closing {
		return nil, sessions.ErrClosed
	}
	manager, err := runtime.lifecycle.Active()
	if err != nil {
		return nil, err
	}
	return runtime.lifecycle.Start(ctx, manager.Path(), operationID)
}

func (runtime *Runtime) runExecution(prompt *string, execution sessions.Execution, emitText func(string)) (runErr error) {
	defer func() { runErr = execution.Finish(runErr) }()
	for {
		err := runtime.runReserved(execution.Context(), prompt, execution, emitText)
		runErr = errors.Join(runErr, err, execution.PromptFinished())
		text, ok := execution.NextQueued()
		if !ok {
			return runErr
		}
		prompt = &text
	}
}

func (runtime *Runtime) runReserved(ctx context.Context, prompt *string, execution sessions.Execution, emitText func(string)) (runErr error) {
	manager := execution.Manager()
	conversation := runtime.conversation(manager)
	settings := map[string]any{}
	if runtime.resolvedSettings != nil {
		settings = runtime.resolvedSettings()
	}
	nameCandidate := prompt != nil && boolSetting(settings["autoSessionNaming"]) && manager.CurrentName() == nil && !hasUserMessage(conversation.contextMessages())
	var systemPrompt *string
	var loadedSkills skills.LoadResult
	if runtime.skills != nil {
		builtPrompt, result, err := runtime.buildSystemPrompt(manager.Cwd(), settings)
		if err != nil {
			return fmt.Errorf("build agent prompt for session %s: %w", manager.SessionID(), err)
		}
		systemPrompt = &builtPrompt
		loadedSkills = result
	}
	defer func() {
		if err := runtime.service.AdoptManager(manager); err != nil {
			runErr = errors.Join(runErr, fmt.Errorf("adopt written session: %w", err))
		}
		if _, err := runtime.service.Refresh(context.Background(), manager.Path(), manager.SessionID()); err != nil {
			runErr = errors.Join(runErr, fmt.Errorf("refresh session snapshot: %w", err))
		}
	}()

	if prompt != nil {
		content := *prompt
		var displayText *string
		if runtime.skills != nil {
			if expanded, original, changed := skills.ExpandCommands(content, loadedSkills.Skills); changed {
				content = expanded
				displayText = &original
			}
		}
		user := session.UserMessage{Content: session.UserText(content), DisplayText: displayText, Timestamp: time.Now().UnixMilli()}
		if _, err := manager.AppendMessage(user); err != nil {
			return fmt.Errorf("persist user message: %w", err)
		}
		conversation.append(user)
	}
	contextMessages := conversation.contextMessages()
	if prompt == nil && len(contextMessages) == 0 {
		return errors.New("No previous session found to continue")
	}
	input := hooks.AgentContext{SystemPrompt: systemPrompt, Messages: toHookMessages(contextMessages)}
	if enabled, _ := settings["blockImages"].(bool); enabled {
		input.Messages = withoutImages(input.Messages)
	}
	runtime.mu.Lock()
	model, provider, effort := runtime.model, runtime.provider, runtime.thinkingEffort
	runtime.mu.Unlock()
	config := agent.Config{Model: model, Provider: provider,
		StreamOptions: types.SimpleStreamOptions{Base: types.StreamOptions{SessionID: stringPointer(manager.SessionID()), Headers: model.Headers}},
		ConvertToLLM:  func(messages []hooks.Message) []types.Message { return toModelMessages(messages) },
		Tools:         runtime.applicationTools(session.IDFromPath(manager.Path())),
		GetSteeringMessages: func() []hooks.Message {
			var messages []hooks.Message
			for _, text := range execution.TakeSteering() {
				display := "[steer] " + text
				messages = append(messages, hooks.Message{Kind: "standard", Standard: types.UserMessage{Content: types.UserText(text), DisplayText: &display, Timestamp: time.Now().UnixMilli()}})
			}
			return messages
		}}
	if retry := objectSetting(settings["retry"]); retry != nil {
		if timeout, ok := settingInteger(retry["timeoutMs"]); ok && timeout > 0 {
			config.StreamOptions.Base.Timeout = time.Duration(timeout) * time.Millisecond
		}
		if maximum, ok := settingInteger(retry["maxRetries"]); ok && maximum >= 0 && maximum <= int64(^uint32(0)) {
			value := uint32(maximum)
			config.StreamOptions.Base.MaxRetries = &value
		}
		if delay, ok := settingInteger(retry["maxRetryDelayMs"]); ok && delay > 0 {
			config.StreamOptions.Base.MaxRetryDelay = time.Duration(delay) * time.Millisecond
		}
	}
	if effort != types.ThinkingOff {
		config.StreamOptions.Reasoning = &effort
	}
	// The manager already includes a newly persisted prompt. Continue runs it
	// from restored context without appending the same user turn twice.
	result := agent.Continue(ctx, input, config)
	var completed []hooks.Message
	var outputErr error
	for event := range result.Events {
		switch event := event.(type) {
		case agent.MessageUpdate:
			if delta, ok := event.StreamEvent.(types.TextDelta); ok {
				if emitText != nil {
					emitText(delta.Delta)
				}
				if outputErr == nil {
					runtime.outputMu.Lock()
					_, err := io.WriteString(runtime.output, delta.Delta)
					runtime.outputMu.Unlock()
					if err != nil {
						outputErr = fmt.Errorf("write CLI output: %w", err)
					}
				}
			}
		case agent.AgentEnd:
			completed = event.Messages
		case agent.MessageEnd:
			if message, ok := toSessionMessage(event.Message); ok {
				conversation.appendDisplay(message)
			}
		}
	}
	ctxErr := ctx.Err()
	var streamErr error
	var previousAssistant *session.AssistantMessage
	for _, message := range completed {
		persisted, ok := toSessionMessage(message)
		if !ok {
			continue
		}
		if ctxErr != nil {
			if assistant, ok := persisted.(session.AssistantMessage); ok && (assistant.StopReason == session.Error || assistant.StopReason == session.Aborted) {
				assistant.StopReason = session.Aborted
				assistant.ErrorMessage = nil
				persisted = assistant
			}
		}
		if assistant, ok := persisted.(session.AssistantMessage); ok {
			if previousAssistant != nil && reflect.DeepEqual(*previousAssistant, assistant) {
				continue
			}
			copy := assistant
			previousAssistant = &copy
		} else {
			previousAssistant = nil
		}
		conversation.appendContext(persisted)
		// As in AgentSession::persist_new_messages, loop-produced users are
		// retained live; the command layer already persisted prompt users.
		if _, user := persisted.(session.UserMessage); !user {
			if _, err := manager.AppendMessage(persisted); err != nil {
				return fmt.Errorf("persist agent message: %w", err)
			}
		}
		if assistant, ok := persisted.(session.AssistantMessage); ok && assistant.StopReason == session.Error {
			if assistant.ErrorMessage != nil {
				streamErr = errors.New(*assistant.ErrorMessage)
			} else {
				streamErr = errors.New("provider stream failed")
			}
		}
	}
	if len(completed) == 0 && prompt != nil && ctxErr == nil {
		return errors.New("agent completed without a persistable response")
	}
	runtime.mu.Lock()
	runtime.revision++
	runtime.mu.Unlock()
	if streamErr != nil {
		streamErr = fmt.Errorf("agent stream: %w", streamErr)
	}
	operationErr := errors.Join(ctxErr, outputErr, streamErr)
	if operationErr == nil {
		if err := runtime.compactIfNeeded(ctx, manager, conversation, model, provider, settings); err != nil {
			operationErr = fmt.Errorf("automatic compaction: %w", err)
		}
	}
	if operationErr == nil && nameCandidate {
		if err := runtime.generateSessionName(ctx, manager, *prompt, settings); err != nil {
			fmt.Fprintf(os.Stderr, "session name generation failed: %v\n", err)
		}
	}
	return operationErr
}

func (runtime *Runtime) buildSystemPrompt(cwd string, settings map[string]any) (string, skills.LoadResult, error) {
	roots, err := runtime.resolveRoots(cwd)
	if err != nil {
		return "", skills.LoadResult{}, err
	}
	resourceDirectories := roots.ResourceDirs()
	loadedResources := resources.Load(cwd, resourceDirectories[:])
	for _, diagnostic := range loadedResources.Diagnostics {
		log.Printf("resource load diagnostic path=%s message=%s", diagnostic.Path, diagnostic.Message)
	}
	loadedSkills, err := runtime.skills.Enabled(cwd, settings)
	if err != nil {
		return "", skills.LoadResult{}, err
	}
	for _, diagnostic := range loadedSkills.Diagnostics {
		log.Printf("skill load diagnostic path=%s message=%s", diagnostic.Path, diagnostic.Message)
	}
	systemPrompt := resources.NewSystemPrompt(resources.DefaultSystemPrompt()).Format(loadedResources.Resources)
	if skillFragment := loadedSkills.PromptFragment(); skillFragment != "" {
		systemPrompt += "\n\n" + skillFragment
	}
	return systemPrompt, loadedSkills, nil
}

func (runtime *Runtime) compactIfNeeded(ctx context.Context, manager *session.Manager, conversation *conversation, model types.Model, provider agent.Provider, settings map[string]any) error {
	compactionSettings := objectSetting(settings["compaction"])
	if !boolSetting(compactionSettings["enabled"]) {
		return nil
	}
	triggerRatio, triggerOK := settingFloat(compactionSettings["triggerRatio"])
	targetRatio, targetOK := settingFloat(compactionSettings["targetRatio"])
	if !triggerOK || !targetOK {
		return errors.New("compaction ratios are unavailable")
	}
	if model.ContextWindow <= 0 {
		return errors.New("model context window must be greater than zero")
	}
	limits, err := (compaction.Settings{Enabled: true, TriggerRatio: triggerRatio, TargetRatio: targetRatio}).ResolveTokenLimits(uint64(model.ContextWindow))
	if err != nil {
		return err
	}
	entries := manager.Entries()
	start := 0
	previousSummary := ""
	for index := len(entries) - 1; index >= 0; index-- {
		compact, ok := entries[index].Payload.(session.Compaction)
		if !ok {
			continue
		}
		previousSummary = compact.Summary
		for keptIndex, entry := range entries {
			if entry.ID == compact.FirstKeptEntryID {
				start = keptIndex
				break
			}
		}
		break
	}
	if start >= len(entries) {
		return nil
	}
	activeEntries := entries[start:]
	contextTokens := latestContextTokens(manager.ContextMessages())
	engine := compaction.Engine{Trigger: compaction.Trigger{ThresholdTokens: limits.ThresholdTokens, TargetTokens: limits.TargetTokens}}
	plan := engine.PrepareWithContext(activeEntries, contextTokens)
	if plan == nil {
		return nil
	}
	result, err := engine.Execute(ctx, manager, activeEntries, *plan, func(ctx context.Context, content string) (string, error) {
		if previousSummary != "" {
			content = "Previous summary:\n" + previousSummary + "\n\nEarlier messages:\n" + content
		}
		return runtime.summarize(ctx, model, provider, content, settings)
	})
	if err != nil {
		return err
	}
	kept := manager.ContextMessages()
	display := "Previous conversation summary"
	contextMessages := append([]session.Message{session.UserMessage{Content: session.UserText(result.Summary), DisplayText: &display}}, kept...)
	conversation.replaceContext(contextMessages)
	return nil
}

func latestContextTokens(messages []session.Message) uint64 {
	for index := len(messages) - 1; index >= 0; index-- {
		assistant, ok := messages[index].(session.AssistantMessage)
		if !ok {
			continue
		}
		if assistant.Usage.TotalTokens > assistant.Usage.Output {
			promptTokens := assistant.Usage.TotalTokens - assistant.Usage.Output
			return promptTokens
		}
		return assistant.Usage.Input + assistant.Usage.CacheRead + assistant.Usage.CacheWrite
	}
	return 0
}

func (runtime *Runtime) summarize(ctx context.Context, model types.Model, provider agent.Provider, content string, settings map[string]any) (string, error) {
	systemPrompt := "Summarize the earlier conversation for a coding assistant continuing the same task. Preserve user goals, constraints, decisions, paths, and unfinished work. Be concise and do not invent details. Return only the summary."
	maxTokens := model.MaxTokens
	if maxTokens <= 0 || maxTokens > 2048 {
		maxTokens = 2048
	}
	effort := types.ThinkingLow
	options := types.SimpleStreamOptions{Base: types.StreamOptions{MaxTokens: &maxTokens}, Reasoning: &effort}
	if retry := objectSetting(settings["retry"]); retry != nil {
		if timeout, ok := settingInteger(retry["timeoutMs"]); ok && timeout > 0 {
			options.Base.Timeout = time.Duration(timeout) * time.Millisecond
		}
	}
	var result strings.Builder
	var streamErr error
	err := provider.StreamSimple(ctx, model, types.Context{SystemPrompt: &systemPrompt, Messages: []types.Message{types.UserMessage{Content: types.UserText(content)}}}, options, func(event types.StreamEvent) {
		switch event := event.(type) {
		case types.TextDelta:
			result.WriteString(event.Delta)
		case types.StreamError:
			if event.Error.ErrorMessage != nil {
				streamErr = errors.New(*event.Error.ErrorMessage)
			} else {
				streamErr = errors.New("provider returned a compaction error")
			}
		case types.Done:
			if result.Len() == 0 {
				for _, block := range event.Message.Content {
					if text, ok := block.(types.TextBlock); ok {
						result.WriteString(text.Text)
					}
				}
			}
		}
	})
	if err != nil {
		return "", err
	}
	if streamErr != nil {
		return "", streamErr
	}
	return strings.TrimSpace(result.String()), nil
}

func hasUserMessage(messages []session.Message) bool {
	for _, message := range messages {
		if _, ok := message.(session.UserMessage); ok {
			return true
		}
	}
	return false
}

func (runtime *Runtime) generateSessionName(ctx context.Context, manager *session.Manager, input string, settings map[string]any) error {
	title := directSessionName(input)
	if title == "" {
		smallModelID, _ := settings["smallModel"].(string)
		if smallModelID == "" || runtime.modelRegistry == nil {
			return nil
		}
		var model types.Model
		for _, candidate := range runtime.modelRegistry.All() {
			if candidate.ID == smallModelID && runtime.modelRegistry.IsConfigured(candidate.Provider, candidate.ID) {
				model = candidate
				break
			}
		}
		if model.ID == "" {
			return fmt.Errorf("configured small model %q was not found", smallModelID)
		}
		apiKey, _ := runtime.modelRegistry.APIKey(model.Provider)
		runtime.mu.Lock()
		currentModel, provider := runtime.model, runtime.provider
		runtime.mu.Unlock()
		if currentModel.ID != model.ID || currentModel.Provider != model.Provider || currentModel.API != model.API {
			nextProvider, err := prepareModelProvider(model, apiKey, runtime.authPath)
			if err != nil {
				return err
			}
			provider = nextProvider
		}
		systemPrompt := "Create a concise session title for the user's coding task. Use the same language as the user. Return only the title. Use at most 8 words, or at most 24 characters for languages without spaces. Do not explain, reason, add labels, quotes, or ending punctuation."
		maxTokens := 32
		low := types.ThinkingLow
		var raw strings.Builder
		var streamErr error
		err := provider.StreamSimple(ctx, model, types.Context{SystemPrompt: &systemPrompt, Messages: []types.Message{types.UserMessage{Content: types.UserText(strings.TrimSpace(input))}}}, types.SimpleStreamOptions{Base: types.StreamOptions{MaxTokens: &maxTokens}, Reasoning: &low}, func(event types.StreamEvent) {
			switch event := event.(type) {
			case types.TextDelta:
				raw.WriteString(event.Delta)
			case types.StreamError:
				if event.Error.ErrorMessage != nil {
					streamErr = errors.New(*event.Error.ErrorMessage)
				} else {
					streamErr = errors.New("provider returned a naming error")
				}
			case types.Done:
				if raw.Len() == 0 {
					for _, block := range event.Message.Content {
						if text, ok := block.(types.TextBlock); ok {
							raw.WriteString(text.Text)
						}
					}
				}
			}
		})
		if err != nil {
			return fmt.Errorf("session naming request: %w", err)
		}
		if streamErr != nil {
			return fmt.Errorf("session naming request: %w", streamErr)
		}
		title = normalizeSessionName(raw.String())
	}
	if title == "" || manager.CurrentName() != nil {
		return nil
	}
	_, err := manager.AppendSessionInfoIfUnset(title)
	return err
}

func directSessionName(input string) string {
	normalized := strings.Join(strings.Fields(input), " ")
	if normalized == "" || len(strings.Fields(normalized)) >= 8 {
		return ""
	}
	maxCharacters := 60
	for _, character := range normalized {
		if character >= 0x2E80 && character <= 0x9FFF || character >= 0xF900 && character <= 0xFAFF || character >= 0x20000 && character <= 0x2FA1F {
			maxCharacters = 24
			break
		}
	}
	if len([]rune(normalized)) <= maxCharacters {
		return normalized
	}
	return ""
}

func normalizeSessionName(input string) string {
	for _, line := range strings.Split(input, "\n") {
		line = strings.TrimSpace(line)
		if line == "" {
			continue
		}
		line = strings.TrimSpace(strings.TrimPrefix(strings.TrimPrefix(line, "Title:"), "title:"))
		line = strings.Trim(line, "\"'` ")
		line = strings.Join(strings.Fields(line), " ")
		if line == "" {
			return ""
		}
		if len([]rune(line)) > 60 {
			line = string([]rune(line)[:57]) + "..."
		}
		return line
	}
	return ""
}

func toHookMessages(messages []session.Message) []hooks.Message {
	result := make([]hooks.Message, 0, len(messages))
	for _, message := range messages {
		if converted, ok := toModelMessage(message); ok {
			result = append(result, hooks.Message{Kind: "standard", Standard: converted})
		}
	}
	return result
}

func withoutImages(messages []hooks.Message) []hooks.Message {
	filtered := make([]hooks.Message, len(messages))
	for i, message := range messages {
		filtered[i] = message
		if message.Kind != "standard" {
			continue
		}
		switch value := message.Standard.(type) {
		case types.UserMessage:
			blocks, ok := value.Content.(types.UserBlocks)
			if !ok {
				continue
			}
			value.Content = types.UserBlocks(withoutImageBlocks(blocks))
			filtered[i].Standard = value
		case types.AssistantMessage:
			value.Content = withoutImageBlocks(value.Content)
			filtered[i].Standard = value
		case types.ToolResultMessage:
			value.Content = withoutImageBlocks(value.Content)
			filtered[i].Standard = value
		}
	}
	return filtered
}

func withoutImageBlocks(blocks []types.ContentBlock) []types.ContentBlock {
	filtered := make([]types.ContentBlock, 0, len(blocks))
	for _, block := range blocks {
		if _, image := block.(types.ImageBlock); !image {
			filtered = append(filtered, block)
		}
	}
	return filtered
}

func objectSetting(value any) map[string]any {
	object, _ := value.(map[string]any)
	return object
}

func boolSetting(value any) bool {
	enabled, _ := value.(bool)
	return enabled
}

func settingInteger(value any) (int64, bool) {
	switch value := value.(type) {
	case json.Number:
		parsed, err := value.Int64()
		return parsed, err == nil
	case int:
		return int64(value), true
	case int64:
		return value, true
	case float64:
		if value == float64(int64(value)) {
			return int64(value), true
		}
	}
	return 0, false
}

func settingFloat(value any) (float64, bool) {
	switch value := value.(type) {
	case json.Number:
		parsed, err := value.Float64()
		return parsed, err == nil
	case float64:
		return value, true
	case int:
		return float64(value), true
	case int64:
		return float64(value), true
	}
	return 0, false
}

func toModelMessage(message session.Message) (types.Message, bool) {
	switch value := message.(type) {
	case session.UserMessage:
		var content types.UserContent
		switch value := value.Content.(type) {
		case session.UserText:
			content = types.UserText(value)
		case session.UserBlocks:
			blocks := make([]types.ContentBlock, 0, len(value))
			for _, block := range value {
				if converted, ok := toModelBlock(block); ok {
					blocks = append(blocks, converted)
				}
			}
			content = types.UserBlocks(blocks)
		default:
			return nil, false
		}
		return types.UserMessage{Content: content, DisplayText: value.DisplayText, Timestamp: value.Timestamp}, true
	case session.AssistantMessage:
		blocks := make([]types.ContentBlock, 0, len(value.Content))
		for _, block := range value.Content {
			if converted, ok := toModelBlock(block); ok {
				blocks = append(blocks, converted)
			}
		}
		return types.AssistantMessage{Content: blocks, API: types.API(value.API), Provider: value.Provider, Model: value.Model, ResponseModel: value.ResponseModel, ResponseID: value.ResponseID, Usage: types.Usage{Input: value.Usage.Input, Output: value.Usage.Output, CacheRead: value.Usage.CacheRead, CacheWrite: value.Usage.CacheWrite, TotalTokens: value.Usage.TotalTokens}, StopReason: types.StopReason(value.StopReason), ErrorMessage: value.ErrorMessage, Timestamp: value.Timestamp}, true
	case session.ToolResultMessage:
		blocks := make([]types.ContentBlock, 0, len(value.Content))
		for _, block := range value.Content {
			if converted, ok := toModelBlock(block); ok {
				blocks = append(blocks, converted)
			}
		}
		return types.ToolResultMessage{ToolCallID: value.ToolCallID, ToolName: value.ToolName, Content: blocks, Details: value.Details, IsError: value.IsError, Timestamp: value.Timestamp}, true
	default:
		return nil, false
	}
}

func toModelBlock(block session.ContentBlock) (types.ContentBlock, bool) {
	switch value := block.(type) {
	case session.TextBlock:
		return types.TextBlock{Text: value.Text, Signature: value.Signature}, true
	case session.ThinkingBlock:
		return types.ThinkingBlock{Thinking: value.Thinking, Signature: value.Signature, Redacted: value.Redacted}, true
	case session.ImageBlock:
		return types.ImageBlock{Data: value.Data, MimeType: value.MimeType}, true
	case session.ToolCallBlock:
		return types.ToolCallBlock{ID: value.ID, Name: value.Name, Arguments: value.Arguments}, true
	default:
		return nil, false
	}
}

func toModelMessages(messages []hooks.Message) []types.Message {
	result := make([]types.Message, 0, len(messages))
	for _, message := range messages {
		if message.Kind == "standard" {
			result = append(result, message.Standard)
		}
	}
	return result
}

func toSessionMessage(message hooks.Message) (session.Message, bool) {
	if message.Kind != "standard" {
		return nil, false
	}
	switch value := message.Standard.(type) {
	case types.UserMessage:
		var content session.UserContent
		switch value := value.Content.(type) {
		case types.UserText:
			content = session.UserText(value)
		case types.UserBlocks:
			blocks := make([]session.ContentBlock, 0, len(value))
			for _, block := range value {
				if converted, ok := toSessionBlock(block); ok {
					blocks = append(blocks, converted)
				}
			}
			content = session.UserBlocks(blocks)
		default:
			return nil, false
		}
		return session.UserMessage{Content: content, DisplayText: value.DisplayText, Timestamp: value.Timestamp}, true
	case types.AssistantMessage:
		blocks := make([]session.ContentBlock, 0, len(value.Content))
		for _, block := range value.Content {
			if converted, ok := toSessionBlock(block); ok {
				blocks = append(blocks, converted)
			}
		}
		return session.AssistantMessage{Content: blocks, API: string(value.API), Provider: value.Provider, Model: value.Model, ResponseModel: value.ResponseModel, ResponseID: value.ResponseID, Usage: session.Usage{Input: value.Usage.Input, Output: value.Usage.Output, CacheRead: value.Usage.CacheRead, CacheWrite: value.Usage.CacheWrite, TotalTokens: value.Usage.TotalTokens}, StopReason: session.StopReason(value.StopReason), ErrorMessage: value.ErrorMessage, Timestamp: value.Timestamp}, true
	case types.ToolResultMessage:
		blocks := make([]session.ContentBlock, 0, len(value.Content))
		for _, block := range value.Content {
			if converted, ok := toSessionBlock(block); ok {
				blocks = append(blocks, converted)
			}
		}
		return session.ToolResultMessage{ToolCallID: value.ToolCallID, ToolName: value.ToolName, Content: blocks, Details: value.Details, IsError: value.IsError, Timestamp: value.Timestamp}, true
	default:
		return nil, false
	}
}

func toSessionBlock(block types.ContentBlock) (session.ContentBlock, bool) {
	switch value := block.(type) {
	case types.TextBlock:
		return session.TextBlock{Text: value.Text, Signature: value.Signature}, true
	case types.ThinkingBlock:
		return session.ThinkingBlock{Thinking: value.Thinking, Signature: value.Signature, Redacted: value.Redacted}, true
	case types.ImageBlock:
		return session.ImageBlock{Data: value.Data, MimeType: value.MimeType}, true
	case types.ToolCallBlock:
		return session.ToolCallBlock{ID: value.ID, Name: value.Name, Arguments: value.Arguments}, true
	default:
		return nil, false
	}
}

func stringPointer(value string) *string { return &value }
