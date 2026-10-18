// Package runtime composes session persistence, credentials, providers and the
// agent loop behind one application owner.
package runtime

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"reflect"
	"sync"
	"time"

	"rozsa/backend/internal/app/permissions"
	"rozsa/backend/internal/app/questions"
	"rozsa/backend/internal/app/session"
	"rozsa/backend/internal/app/sessions"
	"rozsa/backend/internal/app/subagent"
	"rozsa/backend/internal/core/agent"
	"rozsa/backend/internal/core/hooks"
	modelprotocol "rozsa/backend/internal/model"
	"rozsa/backend/internal/model/credentials"
	"rozsa/backend/internal/model/types"
)

type Config struct {
	SessionDir     string
	SessionDirs    []string
	SessionService *session.Service
	Model          types.Model
	APIKey         string
	AuthPath       string
	Provider       agent.Provider
	Output         io.Writer
	LifecycleHooks sessions.Hooks
}

// Runtime composes providers and persistence with a session lifecycle owner.
type Runtime struct {
	mu            sync.Mutex
	session       *session.Manager
	service       *session.Service
	model         types.Model
	provider      agent.Provider
	questions     *questions.Owner
	approvals     *permissions.Owner
	lifecycle     sessions.Owner
	sessionDir    string
	outputMu      sync.Mutex
	output        io.Writer
	revision      uint64
	messages      *messageOperations
	conversations map[string]*conversation
	closing       bool

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
		provider, err := modelprotocol.NewProtocol(config.Model.API)
		if err != nil {
			return nil, err
		}
		resolver := credentials.NewResolver()
		key := ""
		if config.APIKey != "" {
			key, err = resolver.ResolveConfigValue(config.APIKey)
			if err != nil {
				return nil, fmt.Errorf("resolve credentials for %s: %w", config.Model.Provider, err)
			}
		} else if credentialed, ok := provider.(types.ExternalCredentials); ok && credentialed.UsesExternalCredentials() {
			// The protocol owns credentials from the SDK default chain or a
			// protocol-specific bearer token provider.
		} else {
			key, err = resolveProviderKey(context.Background(), resolver, config.Model.Provider, "", config.AuthPath)
			if err != nil {
				return nil, fmt.Errorf("resolve credentials for %s: %w", config.Model.Provider, err)
			}
			if key == "" {
				return nil, fmt.Errorf("No API key for provider: %s", config.Model.Provider)
			}
		}
		config.Provider = providerWithKey{Provider: provider, key: key}
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
	runtime := &Runtime{session: manager, service: service, sessionDir: config.SessionDir, model: config.Model, provider: config.Provider, output: config.Output, questions: questions.NewOwner(), approvals: permissions.NewOwner(), messages: newMessageOperations()}
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

func resolveProviderKey(ctx context.Context, resolver credentials.Resolver, provider, configured, authPath string) (string, error) {
	if configured != "" {
		return resolver.ResolveConfigValue(configured)
	}
	var environmentNames []string
	switch provider {
	case "anthropic":
		environmentNames = []string{"ANTHROPIC_OAUTH_TOKEN", "ANTHROPIC_API_KEY"}
	case "cloudflare-ai-gateway":
		environmentNames = []string{"CLOUDFLARE_API_KEY"}
	}
	for _, name := range environmentNames {
		value, err := resolver.ResolveEnvironmentVariable(name)
		if err != nil {
			return "", err
		}
		if value != "" {
			return value, nil
		}
	}
	return resolver.ResolveAPIKey(ctx, provider, "", "", authPath, nil)
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

type providerWithKey struct {
	Provider agent.Provider
	key      string
}

func (provider providerWithKey) StreamSimple(ctx context.Context, model types.Model, input types.Context, options types.SimpleStreamOptions, emit func(types.StreamEvent)) error {
	options.Base.APIKey = provider.key
	return provider.Provider.StreamSimple(ctx, model, input, options, emit)
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
	defer func() {
		if err := runtime.service.AdoptManager(manager); err != nil {
			runErr = errors.Join(runErr, fmt.Errorf("adopt written session: %w", err))
		}
		if _, err := runtime.service.Refresh(context.Background(), manager.Path(), manager.SessionID()); err != nil {
			runErr = errors.Join(runErr, fmt.Errorf("refresh session snapshot: %w", err))
		}
	}()

	if prompt != nil {
		user := session.UserMessage{Content: session.UserText(*prompt), Timestamp: time.Now().UnixMilli()}
		if _, err := manager.AppendMessage(user); err != nil {
			return fmt.Errorf("persist user message: %w", err)
		}
		conversation.append(user)
	}
	contextMessages := conversation.contextMessages()
	if prompt == nil && len(contextMessages) == 0 {
		return errors.New("No previous session found to continue")
	}
	input := hooks.AgentContext{Messages: toHookMessages(contextMessages)}
	config := agent.Config{Model: runtime.model, Provider: runtime.provider,
		StreamOptions: types.SimpleStreamOptions{Base: types.StreamOptions{SessionID: stringPointer(manager.SessionID()), Headers: runtime.model.Headers}},
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
	return errors.Join(ctxErr, outputErr, streamErr)
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
