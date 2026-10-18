// Package subagent owns independent agent loops and their transcripts.
package subagent

import (
	"context"
	"crypto/rand"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"sync"
	"time"

	"rozsa/backend/internal/app/session"
	"rozsa/backend/internal/app/sessions"
	"rozsa/backend/internal/core/agent"
	"rozsa/backend/internal/core/hooks"
	"rozsa/backend/internal/core/tools"
	"rozsa/backend/internal/model/types"
)

type Status string

const (
	Idle    Status = "idle"
	Running Status = "running"
	Aborted Status = "aborted"
	Error   Status = "error"
)

type Info struct {
	ID             string               `json:"id"`
	Name           string               `json:"name"`
	Status         Status               `json:"status"`
	ModelID        string               `json:"model_id"`
	ModelProvider  string               `json:"model_provider"`
	ThinkingEffort types.ThinkingEffort `json:"thinking_effort"`
	CreatedAt      int64                `json:"created_at"`
	LastActivityAt int64                `json:"last_activity_at"`
	LastError      *string              `json:"last_error"`
	MessageCount   int                  `json:"message_count"`
	SessionFile    *string              `json:"session_file"`
	SessionID      string               `json:"session_id"`
}
type Snapshot struct {
	Info     Info
	Messages []hooks.Message
}
type Event struct {
	OwnerSessionID, SubagentID, SessionID, ToolCallID string
	Sequence                                          uint64
	Info                                              Info
	Agent                                             agent.Event
}

type SpawnConfig struct {
	Name, SystemPrompt string
	Model              *types.Model
	ThinkingEffort     *types.ThinkingEffort
	Scope              Scope
}
type Config struct {
	SessionDir, MainSessionUUID, MainSessionFile, Cwd string
	Model                                             types.Model
	ThinkingEffort                                    types.ThinkingEffort
	Provider                                          agent.Provider
	Tools                                             func(string) []tools.Tool
	DisabledTools                                     map[string]bool
	PermissionHook                                    func(string) hooks.PreToolUse
	ToSessionMessage                                  func(hooks.Message) (session.Message, bool)
	LifecycleHooks                                    sessions.Hooks
	Emit                                              func(Event)
}

type Manager interface {
	Spawn(context.Context, SpawnConfig, string) (Info, error)
	Send(context.Context, string, string, bool, string) error
	Wait(context.Context, string) error
	Abort(context.Context, string) error
	List() []Info
	Snapshot(string) (Snapshot, error)
	Shutdown(context.Context) error
}
type manager struct {
	mu       sync.Mutex
	eventMu  sync.Mutex
	config   Config
	owner    sessions.Owner
	entries  map[string]*entry
	nextID   uint64
	sequence uint64
	closed   bool
}
type entry struct {
	info      Info
	config    SpawnConfig
	model     types.Model
	tools     []tools.Tool
	messages  []hooks.Message
	writer    *session.Manager
	execution sessions.Execution
	done      chan struct{}
}

func New(config Config) (Manager, error) {
	if config.Provider == nil || config.ToSessionMessage == nil || config.MainSessionUUID == "" {
		return nil, errors.New("subagent requires provider, message converter and owner identity")
	}
	if filepath.Base(config.MainSessionUUID) != config.MainSessionUUID || config.MainSessionUUID == "." || config.MainSessionUUID == ".." {
		return nil, errors.New("subagent owner identity must be a single path component")
	}
	if config.ThinkingEffort == "" {
		config.ThinkingEffort = types.ThinkingOff
	}
	disabled := make(map[string]bool, len(config.DisabledTools))
	for name, value := range config.DisabledTools {
		disabled[name] = value
	}
	config.DisabledTools = disabled
	return &manager{config: config, owner: sessions.New(config.LifecycleHooks), entries: make(map[string]*entry), nextID: 1}, nil
}

func (manager *manager) Spawn(ctx context.Context, config SpawnConfig, callID string) (Info, error) {
	if err := ctx.Err(); err != nil {
		return Info{}, err
	}
	manager.mu.Lock()
	var spawned *entry
	defer func() {
		manager.mu.Unlock()
		if spawned != nil {
			manager.emit(spawned, callID, nil)
		}
	}()
	if manager.closed {
		return Info{}, sessions.ErrClosed
	}
	active := 0
	for _, entry := range manager.entries {
		if entry.info.Status == Running {
			active++
		}
	}
	if active >= 10 {
		return Info{}, errors.New("too many active subagents (max 10)")
	}
	var id, path string
	for {
		id = fmt.Sprintf("subagent-%d", manager.nextID)
		manager.nextID++
		path = filepath.Join(manager.config.SessionDir, manager.config.MainSessionUUID, id+".jsonl")
		if manager.config.SessionDir == "" {
			break
		}
		if _, err := os.Lstat(path); os.IsNotExist(err) {
			break
		} else if err != nil {
			return Info{}, err
		}
	}
	if config.Name == "" {
		config.Name = id
	}
	model := manager.config.Model
	if config.Model != nil {
		model = *config.Model
		if model.API != manager.config.Model.API || model.Provider != manager.config.Model.Provider {
			return Info{}, errors.New("subagent model override requires the configured protocol and provider")
		}
	}
	config.Scope = cloneScope(config.Scope)
	effort := manager.config.ThinkingEffort
	if config.ThinkingEffort != nil {
		effort = *config.ThinkingEffort
	}
	var source *string
	if manager.config.MainSessionFile != "" {
		value := manager.config.MainSessionFile
		source = &value
	}
	writer := session.CreateLazy(path, rand.Text(), manager.config.Cwd, source)
	var sessionFile *string
	if manager.config.SessionDir != "" {
		var err error
		writer, err = session.Create(path, writer.SessionID(), manager.config.Cwd, source)
		if err != nil {
			return Info{}, fmt.Errorf("failed to create subagent session: %w", err)
		}
		sessionFile = &path
	}
	if err := manager.owner.Register(writer); err != nil {
		return Info{}, err
	}
	now := time.Now().UnixMilli()
	info := Info{ID: id, Name: config.Name, Status: Idle, ModelID: model.ID, ModelProvider: model.Provider, ThinkingEffort: effort, CreatedAt: now, LastActivityAt: now, SessionFile: sessionFile, SessionID: session.IDFromPath(path)}
	var available []tools.Tool
	if manager.config.Tools != nil {
		for _, tool := range manager.config.Tools(info.SessionID) {
			if tool.Name() != "subagent" && !manager.config.DisabledTools[tool.Name()] && config.Scope.Allows(tool.Name()) {
				available = append(available, tool)
			}
		}
	}
	spawned = &entry{info: info, config: config, model: model, tools: available, writer: writer}
	manager.entries[id] = spawned
	return cloneInfo(info), nil
}

func (manager *manager) Send(ctx context.Context, id, text string, wait bool, callID string) error {
	if err := ctx.Err(); err != nil {
		return err
	}
	manager.mu.Lock()
	entry, err := manager.entryLocked(id)
	if err != nil {
		manager.mu.Unlock()
		return err
	}
	if manager.closed {
		manager.mu.Unlock()
		return sessions.ErrClosed
	}
	if entry.info.Status == Aborted {
		manager.mu.Unlock()
		return fmt.Errorf("subagent '%s' was aborted — create a new one", id)
	}
	if entry.execution != nil {
		manager.mu.Unlock()
		return fmt.Errorf("subagent '%s' is already running", id)
	}
	// Background work survives the tool request returning; explicit abort and
	// shutdown own cancellation through the same lifecycle as normal sessions.
	execution, err := manager.owner.Start(context.Background(), entry.writer.Path(), callID)
	if err != nil {
		manager.mu.Unlock()
		return err
	}
	user := hooks.Message{Kind: "standard", Standard: types.UserMessage{Content: types.UserText(text), Timestamp: time.Now().UnixMilli()}}
	if err := manager.persist(entry, user); err != nil {
		manager.mu.Unlock()
		return execution.Finish(err)
	}
	entry.messages = append(entry.messages, user)
	entry.execution, entry.done = execution, make(chan struct{})
	entry.info.Status, entry.info.LastError = Running, nil
	entry.info.MessageCount, entry.info.LastActivityAt = len(entry.messages), time.Now().UnixMilli()
	manager.mu.Unlock()
	go manager.run(entry, execution, callID)
	if wait {
		return manager.Wait(ctx, id)
	}
	return nil
}

func (manager *manager) Wait(ctx context.Context, id string) error {
	manager.mu.Lock()
	entry, err := manager.entryLocked(id)
	if err != nil {
		manager.mu.Unlock()
		return err
	}
	done := entry.done
	running := entry.execution != nil
	manager.mu.Unlock()
	if !running {
		return ctx.Err()
	}
	select {
	case <-done:
		return nil
	case <-ctx.Done():
		return ctx.Err()
	}
}

func (manager *manager) Abort(ctx context.Context, id string) error {
	if err := ctx.Err(); err != nil {
		return err
	}
	manager.mu.Lock()
	entry, err := manager.entryLocked(id)
	if err != nil {
		manager.mu.Unlock()
		return err
	}
	entry.info.Status, entry.info.LastActivityAt = Aborted, time.Now().UnixMilli()
	execution := entry.execution
	manager.mu.Unlock()
	if execution == nil {
		return nil
	}
	return errors.Join(execution.Cancel(), manager.Wait(ctx, id))
}

func (manager *manager) List() []Info {
	manager.mu.Lock()
	defer manager.mu.Unlock()
	result := make([]Info, 0, len(manager.entries))
	for _, entry := range manager.entries {
		result = append(result, cloneInfo(entry.info))
	}
	sort.Slice(result, func(i, j int) bool {
		return result[i].CreatedAt < result[j].CreatedAt || result[i].CreatedAt == result[j].CreatedAt && result[i].ID < result[j].ID
	})
	return result
}
func (manager *manager) Snapshot(id string) (Snapshot, error) {
	manager.mu.Lock()
	defer manager.mu.Unlock()
	entry, err := manager.entryLocked(id)
	if err != nil {
		return Snapshot{}, err
	}
	return Snapshot{Info: cloneInfo(entry.info), Messages: append([]hooks.Message(nil), entry.messages...)}, nil
}
func (manager *manager) Shutdown(ctx context.Context) error {
	manager.mu.Lock()
	manager.closed = true
	for _, entry := range manager.entries {
		if entry.execution != nil {
			entry.info.Status = Aborted
		}
	}
	manager.mu.Unlock()
	err := manager.owner.Shutdown(ctx)
	manager.mu.Lock()
	var pending []chan struct{}
	for _, entry := range manager.entries {
		if entry.execution != nil {
			pending = append(pending, entry.done)
		}
	}
	manager.mu.Unlock()
	for _, done := range pending {
		select {
		case <-done:
		case <-ctx.Done():
			return errors.Join(err, ctx.Err())
		}
	}
	return err
}
func (manager *manager) entryLocked(id string) (*entry, error) {
	entry := manager.entries[id]
	if entry == nil {
		return nil, fmt.Errorf("subagent '%s' not found", id)
	}
	return entry, nil
}
func cloneInfo(info Info) Info {
	if info.LastError != nil {
		value := *info.LastError
		info.LastError = &value
	}
	if info.SessionFile != nil {
		value := *info.SessionFile
		info.SessionFile = &value
	}
	return info
}
