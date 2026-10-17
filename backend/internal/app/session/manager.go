package session

import (
	"bytes"
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"sync"
	"time"
	"unicode/utf8"
)

// Manager owns a writable session tree. Its mutex protects this instance only:
// there is no cross-instance coordination, fsync or crash recovery protocol.
type Manager struct {
	mu           sync.Mutex
	reader       *Reader
	materialized bool
	openAppend   func(string) (io.WriteCloser, error)
}

func CreateLazy(path, sessionID, cwd string, parentSession *string) *Manager {
	return &Manager{
		reader:     &Reader{path: path, header: Header{Type: "session", Version: 3, ID: sessionID, Cwd: cwd, ParentSession: copyString(parentSession)}, byID: make(map[string]Entry)},
		openAppend: func(path string) (io.WriteCloser, error) { return os.OpenFile(path, os.O_WRONLY|os.O_APPEND, 0) },
	}
}

func Create(path, sessionID, cwd string, parentSession *string) (*Manager, error) {
	manager := CreateLazy(path, sessionID, cwd, parentSession)
	if err := manager.ensureMaterialized(); err != nil {
		return nil, err
	}
	return manager, nil
}

// OpenManager reuses Open's exact header, malformed-entry and leaf rules.
func OpenManager(path string) (*Manager, error) {
	reader, err := Open(path)
	if err != nil {
		return nil, err
	}
	return managerFromReader(reader), nil
}

func managerFromReader(reader *Reader) *Manager {
	manager := CreateLazy(reader.path, reader.header.ID, reader.header.Cwd, reader.header.ParentSession)
	manager.reader, manager.materialized = reader, true
	return manager
}

func (manager *Manager) ensureMaterialized() error {
	if manager.materialized {
		return nil
	}
	if err := os.MkdirAll(filepath.Dir(manager.reader.path), 0o777); err != nil {
		return fmt.Errorf("Failed to create session directory: %w", err)
	}
	file, err := os.Create(manager.reader.path)
	if err != nil {
		return fmt.Errorf("Failed to create session file: %s: %w", manager.reader.path, err)
	}
	header := manager.reader.header
	header.Timestamp = rustTime(time.Now())
	data, err := marshalHeader(header)
	if err == nil {
		err = writeLine(file, data)
	}
	closeErr := file.Close()
	if err != nil {
		return fmt.Errorf("Failed to write session header: %w", err)
	}
	if closeErr != nil {
		return fmt.Errorf("Failed to close session file: %w", closeErr)
	}
	manager.reader.header, manager.materialized = header, true
	return nil
}

func marshalHeader(header Header) ([]byte, error) {
	for _, value := range []string{header.Type, header.ID, header.Timestamp, header.Cwd} {
		if !utf8.ValidString(value) {
			return nil, fmt.Errorf("invalid UTF-8 in session header")
		}
	}
	if header.ParentSession != nil && !utf8.ValidString(*header.ParentSession) {
		return nil, fmt.Errorf("invalid UTF-8 in parentSession")
	}
	return json.Marshal(struct {
		Type          string  `json:"type"`
		Version       uint32  `json:"version"`
		ID            string  `json:"id"`
		Timestamp     string  `json:"timestamp"`
		Cwd           string  `json:"cwd"`
		ParentSession *string `json:"parentSession,omitempty"`
	}{header.Type, header.Version, header.ID, header.Timestamp, header.Cwd, header.ParentSession})
}

func writeLine(writer io.Writer, data []byte) error {
	line := append(data, '\n')
	written, err := writer.Write(line)
	if err == nil && written != len(line) {
		return io.ErrShortWrite
	}
	return err
}

func (manager *Manager) Header() Header {
	manager.mu.Lock()
	defer manager.mu.Unlock()
	header := manager.reader.header
	header.ParentSession = copyString(header.ParentSession)
	return header
}
func (manager *Manager) Path() string {
	manager.mu.Lock()
	defer manager.mu.Unlock()
	return manager.reader.path
}
func (manager *Manager) SessionID() string { return manager.Header().ID }
func (manager *Manager) Cwd() string       { return manager.Header().Cwd }
func (manager *Manager) LeafID() *string {
	manager.mu.Lock()
	defer manager.mu.Unlock()
	return copyString(manager.reader.leafID)
}

func (manager *Manager) Entries() []Entry {
	manager.mu.Lock()
	defer manager.mu.Unlock()
	entries := manager.reader.Entries()
	for i := range entries {
		entries[i] = copyEntry(entries[i])
	}
	return entries
}

func (manager *Manager) ContextMessages() []Message {
	manager.mu.Lock()
	defer manager.mu.Unlock()
	messages := manager.reader.ContextMessages()
	for i := range messages {
		messages[i] = copyMessage(messages[i])
	}
	return messages
}

// Append accepts only the sealed persisted variants. Labels must target an
// indexed entry, including entries outside the currently active branch.
func (manager *Manager) Append(payload EntryPayload) (string, error) {
	manager.mu.Lock()
	defer manager.mu.Unlock()
	return manager.appendLocked(payload)
}

func (manager *Manager) appendLocked(payload EntryPayload) (string, error) {
	if label, ok := payload.(Label); ok {
		if _, exists := manager.reader.byID[label.TargetID]; !exists {
			return "", fmt.Errorf("Entry %s not found", label.TargetID)
		}
	}
	id := ""
	for attempt := 0; attempt <= 100; attempt++ {
		var random [16]byte
		if _, err := rand.Read(random[:]); err != nil {
			return "", fmt.Errorf("Failed to generate session entry ID: %w", err)
		}
		random[6] = random[6]&0x0f | 0x40
		random[8] = random[8]&0x3f | 0x80
		encoded := hex.EncodeToString(random[:])
		id = encoded[:8]
		if attempt == 100 {
			id = fmt.Sprintf("%s-%s-%s-%s-%s", encoded[:8], encoded[8:12], encoded[12:16], encoded[16:20], encoded[20:])
		}
		if _, exists := manager.reader.byID[id]; !exists {
			break
		}
	}
	entry := Entry{ID: id, ParentID: copyString(manager.reader.leafID), Timestamp: rustTime(time.Now()), Payload: payload}
	// Rust materializes before serialization, so a serialization failure can
	// leave a header-only file, but never advances the entry index or leaf.
	if err := manager.ensureMaterialized(); err != nil {
		return "", err
	}
	data, err := marshalEntry(entry)
	if err != nil {
		return "", fmt.Errorf("Failed to serialize session entry: %w", err)
	}
	file, err := manager.openAppend(manager.reader.path)
	if err != nil {
		return "", fmt.Errorf("Failed to open session file for append: %s: %w", manager.reader.path, err)
	}
	writeErr := writeLine(file, data)
	closeErr := file.Close()
	if writeErr != nil {
		return "", fmt.Errorf("Failed to append entry to session file: %w", writeErr)
	}
	if closeErr != nil {
		return "", fmt.Errorf("Failed to close session file: %w", closeErr)
	}
	manager.reader.byID[id], manager.reader.leafID = copyEntry(entry), &id
	return id, nil
}

func (manager *Manager) AppendMessage(message Message) (string, error) {
	return manager.Append(MessageEntry{message})
}
func (manager *Manager) AppendCompaction(summary, firstKeptEntryID string, tokensBefore uint64, details json.RawMessage, fromHook *bool) (string, error) {
	return manager.Append(Compaction{summary, firstKeptEntryID, tokensBefore, details, fromHook})
}
func (manager *Manager) AppendModelChange(provider, modelID string) (string, error) {
	return manager.Append(ModelChange{provider, modelID})
}
func (manager *Manager) AppendThinkingEffortChange(effort string) (string, error) {
	return manager.Append(ThinkingEffortChange{effort})
}
func (manager *Manager) AppendCustom(customType string, data json.RawMessage) (string, error) {
	return manager.Append(Custom{customType, data})
}
func (manager *Manager) AppendLabel(targetID string, label *string) (string, error) {
	return manager.Append(Label{targetID, label})
}
func (manager *Manager) AppendSessionInfo(name *string) (string, error) {
	return manager.Append(SessionInfo{name})
}

// CopyContextMessagesFrom copies messages only, with fresh IDs and links. Each
// successful append remains committed if a later append fails, as in Rust.
func (manager *Manager) CopyContextMessagesFrom(source *Manager) error {
	messages := source.ContextMessages()
	manager.mu.Lock()
	defer manager.mu.Unlock()
	for _, message := range messages {
		if _, err := manager.appendLocked(MessageEntry{message}); err != nil {
			return err
		}
	}
	return nil
}

func (manager *Manager) CopyContextMessagesFromPath(path string) error {
	source, err := OpenManager(path)
	if err != nil {
		return err
	}
	return manager.CopyContextMessagesFrom(source)
}

func (manager *Manager) CurrentName() *string {
	manager.mu.Lock()
	defer manager.mu.Unlock()
	var best *Entry
	for _, entry := range manager.reader.byID {
		if _, ok := entry.Payload.(SessionInfo); ok && (best == nil || entry.Timestamp > best.Timestamp) {
			current := entry
			best = &current
		}
	}
	if best == nil {
		return nil
	}
	return copyString(best.Payload.(SessionInfo).Name)
}

func (manager *Manager) LatestCustom(customType string) *Entry {
	manager.mu.Lock()
	defer manager.mu.Unlock()
	var best *Entry
	for _, entry := range manager.reader.byID {
		if custom, ok := entry.Payload.(Custom); ok && custom.Type == customType && (best == nil || entry.Timestamp > best.Timestamp) {
			current := copyEntry(entry)
			best = &current
		}
	}
	return best
}

// SetCwd preserves all bytes after the first physical header line, including
// malformed entries. Rewriting is deliberately non-atomic, matching Rust.
func (manager *Manager) SetCwd(cwd string) error {
	manager.mu.Lock()
	defer manager.mu.Unlock()
	if manager.reader.header.Cwd == cwd {
		return nil
	}
	if err := manager.ensureMaterialized(); err != nil {
		return err
	}
	data, err := os.ReadFile(manager.reader.path)
	if err != nil {
		return fmt.Errorf("Failed to read session file: %s: %w", manager.reader.path, err)
	}
	if !utf8.Valid(data) {
		return fmt.Errorf("Failed to read session file: %s: invalid UTF-8", manager.reader.path)
	}
	headerLine, rest, found := bytes.Cut(data, []byte{'\n'})
	if !found {
		return fmt.Errorf("Session file has no header line")
	}
	headerValue, ok := parseHeader(headerLine)
	if !ok {
		return fmt.Errorf("Failed to parse session header while updating cwd")
	}
	header := Header{*headerValue.Type, *headerValue.Version, *headerValue.ID, *headerValue.Timestamp, cwd, headerValue.ParentSession}
	serialized, err := marshalHeader(header)
	if err != nil {
		return fmt.Errorf("Failed to serialize session header: %w", err)
	}
	updated := append(append(serialized, '\n'), rest...)
	if err := os.WriteFile(manager.reader.path, updated, 0o666); err != nil {
		return fmt.Errorf("Failed to write session file: %s: %w", manager.reader.path, err)
	}
	manager.reader.header.Cwd = cwd
	return nil
}

func Rename(path string, name *string) error {
	manager, err := OpenManager(path)
	if err != nil {
		return err
	}
	_, err = manager.AppendSessionInfo(name)
	return err
}

func Delete(path string) error {
	if info, err := os.Lstat(path); err != nil {
		return fmt.Errorf("Failed to delete session file: %s: %w", path, err)
	} else if info.IsDir() {
		return fmt.Errorf("Failed to delete session file: %s: is a directory", path)
	}
	if err := os.Remove(path); err != nil {
		return fmt.Errorf("Failed to delete session file: %s: %w", path, err)
	}
	return nil
}

func marshalEntry(entry Entry) ([]byte, error) {
	fields := map[string]any{"id": entry.ID, "parentId": entry.ParentID, "timestamp": entry.Timestamp}
	switch payload := entry.Payload.(type) {
	case MessageEntry:
		message, err := persistedMessage(payload.Message)
		if err != nil {
			return nil, err
		}
		fields["type"], fields["message"] = "message", message
	case ThinkingEffortChange:
		fields["type"], fields["thinkingEffort"] = "thinking_effort_change", payload.Effort
	case ModelChange:
		fields["type"], fields["provider"], fields["modelId"] = "model_change", payload.Provider, payload.ModelID
	case Compaction:
		fields["type"], fields["summary"], fields["firstKeptEntryId"], fields["tokensBefore"] = "compaction", payload.Summary, payload.FirstKeptEntryID, payload.TokensBefore
		if payload.Details != nil {
			fields["details"] = payload.Details
		}
		if payload.FromHook != nil {
			fields["fromHook"] = *payload.FromHook
		}
	case Custom:
		fields["type"], fields["customType"] = "custom", payload.Type
		if payload.Data != nil {
			fields["data"] = payload.Data
		}
	case Label:
		fields["type"], fields["targetId"] = "label", payload.TargetID
		if payload.Label != nil {
			fields["label"] = *payload.Label
		}
	case SessionInfo:
		fields["type"] = "session_info"
		if payload.Name != nil {
			fields["name"] = *payload.Name
		}
	default:
		return nil, fmt.Errorf("unsupported session entry %T", entry.Payload)
	}
	if !validPersistenceText(fields) {
		return nil, fmt.Errorf("invalid UTF-8 in session entry")
	}
	data, err := json.Marshal(fields)
	if err != nil {
		return nil, err
	}
	if _, ok := parseRestoredEntry(data); !ok {
		return nil, fmt.Errorf("invalid persisted session entry")
	}
	return data, nil
}

// Go strings can contain invalid UTF-8, unlike Rust String. encoding/json
// silently replaces those bytes, so reject them at the persistence boundary.
func validPersistenceText(value any) bool {
	switch value := value.(type) {
	case string:
		return utf8.ValidString(value)
	case *string:
		return value == nil || utf8.ValidString(*value)
	case json.RawMessage:
		return utf8.Valid(value)
	case map[string]any:
		for _, field := range value {
			if !validPersistenceText(field) {
				return false
			}
		}
	case []map[string]any:
		for _, block := range value {
			if !validPersistenceText(block) {
				return false
			}
		}
	}
	return true
}

func persistedMessage(message Message) (map[string]any, error) {
	switch value := message.(type) {
	case UserMessage:
		var blocks []ContentBlock
		switch content := value.Content.(type) {
		case UserText:
			blocks = []ContentBlock{TextBlock{Text: string(content)}}
		case UserBlocks:
			blocks = []ContentBlock(content)
		default:
			return nil, fmt.Errorf("unsupported user content %T", value.Content)
		}
		content, err := persistedBlocks(blocks)
		if err != nil {
			return nil, err
		}
		fields := map[string]any{"role": "user", "content": content, "timestamp": value.Timestamp}
		if value.DisplayText != nil {
			fields["displayText"] = *value.DisplayText
		}
		return fields, nil
	case AssistantMessage:
		content, err := persistedBlocks(value.Content)
		if err != nil {
			return nil, err
		}
		reasons := [...]string{"stop", "length", "toolUse", "error", "aborted"}
		if int(value.StopReason) >= len(reasons) {
			return nil, fmt.Errorf("invalid stop reason %d", value.StopReason)
		}
		usage, cost := value.Usage, value.Usage.Cost
		fields := map[string]any{"role": "assistant", "content": content, "api": value.API, "provider": value.Provider, "model": value.Model, "timestamp": value.Timestamp, "stopReason": reasons[value.StopReason],
			"usage": map[string]any{"input": usage.Input, "output": usage.Output, "cacheRead": usage.CacheRead, "cacheWrite": usage.CacheWrite, "totalTokens": usage.TotalTokens, "cost": map[string]any{"input": cost.Input, "output": cost.Output, "cacheRead": cost.CacheRead, "cacheWrite": cost.CacheWrite, "total": cost.Total}}}
		if value.ResponseModel != nil {
			fields["responseModel"] = *value.ResponseModel
		}
		if value.ResponseID != nil {
			fields["responseId"] = *value.ResponseID
		}
		if value.ErrorMessage != nil {
			fields["errorMessage"] = *value.ErrorMessage
		}
		return fields, nil
	case ToolResultMessage:
		content, err := persistedBlocks(value.Content)
		if err != nil {
			return nil, err
		}
		fields := map[string]any{"role": "toolResult", "toolCallId": value.ToolCallID, "toolName": value.ToolName, "content": content, "isError": value.IsError, "timestamp": value.Timestamp}
		if value.Details != nil && !bytes.Equal(bytes.TrimSpace(value.Details), []byte("null")) {
			fields["details"] = value.Details
		}
		return fields, nil
	default:
		return nil, fmt.Errorf("unsupported session message %T", message)
	}
}

func persistedBlocks(blocks []ContentBlock) ([]map[string]any, error) {
	result := make([]map[string]any, 0, len(blocks))
	for _, block := range blocks {
		var fields map[string]any
		switch value := block.(type) {
		case TextBlock:
			fields = map[string]any{"type": "text", "text": value.Text}
			if value.Signature != nil {
				fields["textSignature"] = *value.Signature
			}
		case ThinkingBlock:
			fields = map[string]any{"type": "thinking", "thinking": value.Thinking}
			if value.Signature != nil {
				fields["thinkingSignature"] = *value.Signature
			}
			if value.Redacted {
				fields["redacted"] = true
			}
		case ImageBlock:
			fields = map[string]any{"type": "image", "data": value.Data, "mimeType": value.MimeType}
		case ToolCallBlock:
			if value.Arguments == nil {
				return nil, fmt.Errorf("missing tool call arguments")
			}
			fields = map[string]any{"type": "toolCall", "id": value.ID, "name": value.Name, "arguments": value.Arguments}
		default:
			return nil, fmt.Errorf("unsupported content block %T", block)
		}
		result = append(result, fields)
	}
	return result, nil
}

func copyString(value *string) *string {
	if value == nil {
		return nil
	}
	copied := *value
	return &copied
}
func copyEntry(entry Entry) Entry {
	entry.ParentID = copyString(entry.ParentID)
	switch payload := entry.Payload.(type) {
	case MessageEntry:
		payload.Message = copyMessage(payload.Message)
		entry.Payload = payload
	case Compaction:
		payload.Details = bytes.Clone(payload.Details)
		if payload.FromHook != nil {
			flag := *payload.FromHook
			payload.FromHook = &flag
		}
		entry.Payload = payload
	case Custom:
		payload.Data = bytes.Clone(payload.Data)
		entry.Payload = payload
	case Label:
		payload.Label = copyString(payload.Label)
		entry.Payload = payload
	case SessionInfo:
		payload.Name = copyString(payload.Name)
		entry.Payload = payload
	}
	return entry
}
func copyMessage(message Message) Message {
	switch value := message.(type) {
	case UserMessage:
		value.DisplayText = copyString(value.DisplayText)
		if blocks, ok := value.Content.(UserBlocks); ok {
			value.Content = UserBlocks(copyBlocks(blocks))
		}
		return value
	case AssistantMessage:
		value.Content = copyBlocks(value.Content)
		value.ResponseModel = copyString(value.ResponseModel)
		value.ResponseID = copyString(value.ResponseID)
		value.ErrorMessage = copyString(value.ErrorMessage)
		return value
	case ToolResultMessage:
		value.Content = copyBlocks(value.Content)
		value.Details = bytes.Clone(value.Details)
		return value
	default:
		return message
	}
}
func copyBlocks(blocks []ContentBlock) []ContentBlock {
	copied := make([]ContentBlock, len(blocks))
	for i, block := range blocks {
		switch value := block.(type) {
		case TextBlock:
			value.Signature = copyString(value.Signature)
			copied[i] = value
		case ThinkingBlock:
			value.Signature = copyString(value.Signature)
			copied[i] = value
		case ToolCallBlock:
			value.Arguments = bytes.Clone(value.Arguments)
			copied[i] = value
		default:
			copied[i] = block
		}
	}
	return copied
}
