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
	"sync"
	"time"

	"rozsa/backend/internal/app/session"
	"rozsa/backend/internal/core/agent"
	"rozsa/backend/internal/core/hooks"
	"rozsa/backend/internal/model/anthropic"
	"rozsa/backend/internal/model/credentials"
	"rozsa/backend/internal/model/types"
)

type Config struct {
	SessionDir string
	Model      types.Model
	APIKey     string
	AuthPath   string
	Provider   agent.Provider
	Output     io.Writer
}

// Runtime owns the active session and serializes prompts against it.
type Runtime struct {
	mu       sync.Mutex
	session  *session.Manager
	service  *session.Service
	model    types.Model
	provider agent.Provider
	output   io.Writer
	pending  []hooks.Message
	queue    []string
	revision uint64
	running  bool
}

func New(config Config) (*Runtime, error) {
	if config.SessionDir == "" {
		return nil, errors.New("session directory is required")
	}
	if config.Model.ID == "" || config.Model.Provider == "" || config.Model.API == "" {
		return nil, errors.New("runtime requires a configured model, provider and API")
	}
	if config.Model.API != types.APIAnthropicMessages {
		return nil, fmt.Errorf("unsupported model API %q", config.Model.API)
	}
	if config.Provider == nil {
		resolver := credentials.NewResolver()
		key, err := resolveProviderKey(context.Background(), resolver, config.Model.Provider, config.APIKey, config.AuthPath)
		if err != nil {
			return nil, fmt.Errorf("resolve credentials for %s: %w", config.Model.Provider, err)
		}
		if key == "" {
			return nil, fmt.Errorf("No API key for provider: %s", config.Model.Provider)
		}
		config.Provider = providerWithKey{Provider: anthropic.NewProvider(nil), key: key}
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
	return &Runtime{session: manager, service: session.NewService(session.Catalog{Directories: []string{config.SessionDir}}), model: config.Model, provider: config.Provider, output: config.Output}, nil
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
	return runtime.service.Switch(ctx, path, catalogID)
}

func (runtime *Runtime) Subscribe() session.EventSubscription { return runtime.service.Subscribe() }

func (runtime *Runtime) Snapshot(ctx context.Context) (session.StateSnapshot, error) {
	return runtime.service.Snapshot(ctx)
}

type providerWithKey struct {
	*anthropic.Provider
	key string
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
	runtime.mu.Lock()
	if runtime.running {
		runtime.mu.Unlock()
		return errors.New("agent session is already running")
	}
	runtime.running = true
	manager := runtime.session
	runtime.mu.Unlock()
	defer func() { runtime.mu.Lock(); runtime.running = false; runtime.mu.Unlock() }()

	if prompt != nil {
		user := types.UserMessage{Content: types.UserText(*prompt), Timestamp: time.Now().UnixMilli()}
		if _, err := manager.AppendMessage(session.UserMessage{Content: session.UserText(*prompt), Timestamp: user.Timestamp}); err != nil {
			return fmt.Errorf("persist user message: %w", err)
		}
	}
	contextMessages := manager.ContextMessages()
	if prompt == nil && len(contextMessages) == 0 {
		return errors.New("No previous session found to continue")
	}
	input := hooks.AgentContext{Messages: toHookMessages(contextMessages)}
	config := agent.Config{Model: runtime.model, Provider: runtime.provider,
		StreamOptions: types.SimpleStreamOptions{Base: types.StreamOptions{SessionID: stringPointer(manager.SessionID()), Headers: runtime.model.Headers}},
		ConvertToLLM:  func(messages []hooks.Message) []types.Message { return toModelMessages(messages) }}
	// The manager already includes a newly persisted prompt. Continue runs it
	// from restored context without appending the same user turn twice.
	result := agent.Continue(ctx, input, config)
	var completed []hooks.Message
	for event := range result.Events {
		switch event := event.(type) {
		case agent.MessageUpdate:
			if delta, ok := event.StreamEvent.(types.TextDelta); ok {
				if _, err := io.WriteString(runtime.output, delta.Delta); err != nil {
					return fmt.Errorf("write CLI output: %w", err)
				}
			}
		case agent.AgentEnd:
			completed = event.Messages
		}
	}
	if err := ctx.Err(); err != nil {
		return err
	}
	var streamErr error
	for _, message := range completed {
		persisted, ok := toSessionMessage(message)
		if !ok {
			continue
		}
		if _, err := manager.AppendMessage(persisted); err != nil {
			return fmt.Errorf("persist agent message: %w", err)
		}
		if assistant, ok := persisted.(session.AssistantMessage); ok && assistant.StopReason == session.Error {
			if assistant.ErrorMessage != nil {
				streamErr = errors.New(*assistant.ErrorMessage)
			} else {
				streamErr = errors.New("provider stream failed")
			}
		}
	}
	if len(completed) == 0 && prompt != nil {
		return errors.New("agent completed without a persistable response")
	}
	runtime.mu.Lock()
	runtime.revision++
	runtime.mu.Unlock()
	if streamErr != nil {
		return fmt.Errorf("agent stream: %w", streamErr)
	}
	return nil
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
