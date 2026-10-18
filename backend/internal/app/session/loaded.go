package session

import (
	"encoding/json"
	"fmt"
	"path/filepath"
	"strings"
)

// A display projection, not a provider context or full message schema.
type HistoryBlock struct {
	Type      string          `json:"type"`
	Text      string          `json:"text,omitempty"`
	Name      string          `json:"name,omitempty"`
	ID        string          `json:"id,omitempty"`
	Arguments json.RawMessage `json:"arguments,omitempty"`
	Data      string          `json:"data,omitempty"`
	MIMEType  string          `json:"mimeType,omitempty"`
}

func (block HistoryBlock) MarshalJSON() ([]byte, error) {
	type wireBlock struct {
		Type      string          `json:"type"`
		Text      *string         `json:"text,omitempty"`
		Name      *string         `json:"name,omitempty"`
		ID        string          `json:"id,omitempty"`
		Arguments json.RawMessage `json:"arguments,omitempty"`
		Data      string          `json:"data,omitempty"`
		MIMEType  string          `json:"mimeType,omitempty"`
	}
	wire := wireBlock{Type: block.Type, ID: block.ID, Arguments: block.Arguments, Data: block.Data, MIMEType: block.MIMEType}
	if block.Type == "text" || block.Type == "thinking" {
		wire.Text = &block.Text
	}
	if block.Type == "toolCall" {
		wire.Name = &block.Name
	}
	return json.Marshal(wire)
}

type HistoryMessage struct {
	EntryID      string          `json:"entryId"`
	Role         string          `json:"role"`
	Content      []HistoryBlock  `json:"content"`
	DisplayText  *string         `json:"displayText,omitempty"`
	ToolCallID   string          `json:"toolCallId,omitempty"`
	ToolName     string          `json:"toolName,omitempty"`
	Details      json.RawMessage `json:"details,omitempty"`
	ErrorMessage *string         `json:"errorMessage,omitempty"`
	StopReason   string          `json:"stopReason,omitempty"`
	Timestamp    int64           `json:"timestamp,omitempty"`
	IsError      bool            `json:"isError,omitempty"`
}
type LoadedSnapshot struct {
	Path                string           `json:"path"`
	CatalogID           string           `json:"catalogId"`
	SessionID           string           `json:"sessionId"`
	Cwd                 string           `json:"cwd"`
	Messages            []HistoryMessage `json:"messages"`
	ContextMessageCount int              `json:"contextMessageCount"`
}
type loadedTab struct {
	reader   *Reader
	snapshot LoadedSnapshot
	manager  *Manager
}

// Mirrors switch_session's path-keyed Loaded tabs. No agent is started; the
// effective model context stays in Go's Reader, separate from display history.
type LoadedSessions struct {
	Catalog    Catalog
	tabs       map[string]loadedTab
	activePath string
}

func (loadedSessions *LoadedSessions) Switch(path, catalogID string) (LoadedSnapshot, error) {
	if tab, ok := loadedSessions.tabs[path]; ok {
		if tab.snapshot.CatalogID != catalogID {
			return LoadedSnapshot{}, fmt.Errorf("Session identity changed")
		}
		loadedSessions.activePath = path
		return tab.snapshot, nil
	}
	entries, err := loadedSessions.Catalog.GetSessions()
	if err != nil {
		return LoadedSnapshot{}, err
	}
	allowed := false
	for _, entry := range entries {
		if entry.Path == path && entry.ID == catalogID {
			allowed = true
			break
		}
	}
	if !allowed {
		return LoadedSnapshot{}, fmt.Errorf("Session is not in the configured catalog")
	}
	reader, err := Open(path)
	if err != nil {
		return LoadedSnapshot{}, err
	}
	if reader.Header().ID != catalogID {
		return LoadedSnapshot{}, fmt.Errorf("Session identity changed")
	}
	result, err := loadedSnapshot(path, catalogID, reader)
	if err != nil {
		return LoadedSnapshot{}, err
	}
	if loadedSessions.tabs == nil {
		loadedSessions.tabs = make(map[string]loadedTab)
	}
	loadedSessions.tabs[path] = loadedTab{reader: reader, snapshot: result}
	loadedSessions.activePath = path
	return result, nil
}

func (loadedSessions *LoadedSessions) AdoptManager(manager *Manager) error {
	manager.mu.Lock()
	defer manager.mu.Unlock()
	reader := manager.reader
	path := reader.path
	materialized := manager.materialized
	if !materialized {
		return nil
	}
	snapshot, err := loadedSnapshot(path, reader.header.ID, reader)
	if err != nil {
		return err
	}
	if loadedSessions.tabs == nil {
		loadedSessions.tabs = make(map[string]loadedTab)
	}
	loadedSessions.tabs[path] = loadedTab{reader: reader, snapshot: snapshot, manager: manager}
	return nil
}

// IDFromPath is the GUI session identity; the persisted header ID remains the
// catalog identity and can differ from the filename.
func IDFromPath(path string) string {
	stem := filepath.Base(path)
	// Rust file_stem treats a leading dot as part of a dotfile's name.
	if dot := strings.LastIndex(stem, "."); dot > 0 {
		stem = stem[:dot]
	}
	return stem
}

func loadedSnapshot(path, catalogID string, reader *Reader) (LoadedSnapshot, error) {
	result := LoadedSnapshot{Path: path, CatalogID: catalogID, SessionID: IDFromPath(path), Cwd: reader.Header().Cwd, Messages: []HistoryMessage{}, ContextMessageCount: len(reader.ContextMessages())}
	for _, entry := range reader.Entries() {
		if message, ok := entry.Payload.(MessageEntry); ok {
			view, err := historyMessage(entry.ID, message.Message)
			if err != nil {
				return LoadedSnapshot{}, err
			}
			result.Messages = append(result.Messages, view)
		}
	}
	return result, nil
}

// Refresh rereads an already-open tab after its writable session manager has
// appended entries. Failed reads preserve the previously usable cached tab.
func (loadedSessions *LoadedSessions) Refresh(path, catalogID string) (LoadedSnapshot, error) {
	selected := loadedSessions.activePath
	previous, existed := loadedSessions.tabs[path]
	delete(loadedSessions.tabs, path)
	snapshot, err := loadedSessions.Switch(path, catalogID)
	if err != nil && existed {
		loadedSessions.tabs[path] = previous
	}
	if err == nil && existed {
		tab := loadedSessions.tabs[path]
		tab.manager = previous.manager
		loadedSessions.tabs[path] = tab
	}
	if selected != "" || err != nil {
		loadedSessions.activePath = selected
	}
	return snapshot, err
}

func historyMessage(id string, message Message) (HistoryMessage, error) {
	history := HistoryMessage{EntryID: id, Content: []HistoryBlock{}}
	var blocks []ContentBlock
	switch typedMessage := message.(type) {
	case UserMessage:
		history.Role, history.DisplayText, history.Timestamp = "user", typedMessage.DisplayText, typedMessage.Timestamp
		switch content := typedMessage.Content.(type) {
		case UserText:
			blocks = []ContentBlock{TextBlock{Text: string(content)}}
		case UserBlocks:
			blocks = []ContentBlock(content)
		default:
			return history, fmt.Errorf("Unsupported user content %T", content)
		}
	case AssistantMessage:
		history.Role, history.ErrorMessage, history.StopReason, history.Timestamp, blocks = "assistant", typedMessage.ErrorMessage, stopReasonName(typedMessage.StopReason), typedMessage.Timestamp, typedMessage.Content
	case ToolResultMessage:
		history.Role, history.ToolCallID, history.ToolName, history.Details, history.IsError, history.Timestamp, blocks = "toolResult", typedMessage.ToolCallID, typedMessage.ToolName, typedMessage.Details, typedMessage.IsError, typedMessage.Timestamp, typedMessage.Content
	default:
		return history, fmt.Errorf("Unsupported history message %T", message)
	}
	for _, block := range blocks {
		switch contentBlock := block.(type) {
		case TextBlock:
			history.Content = append(history.Content, HistoryBlock{Type: "text", Text: contentBlock.Text})
		case ThinkingBlock:
			history.Content = append(history.Content, HistoryBlock{Type: "thinking", Text: contentBlock.Thinking})
		case ImageBlock:
			history.Content = append(history.Content, HistoryBlock{Type: "image", Data: contentBlock.Data, MIMEType: contentBlock.MimeType})
		case ToolCallBlock:
			history.Content = append(history.Content, HistoryBlock{Type: "toolCall", ID: contentBlock.ID, Name: contentBlock.Name, Arguments: contentBlock.Arguments})
		default:
			return history, fmt.Errorf("Unsupported history block %T", block)
		}
	}
	return history, nil
}

func stopReasonName(reason StopReason) string {
	switch reason {
	case Stop:
		return "stop"
	case Length:
		return "length"
	case ToolUse:
		return "toolUse"
	case Error:
		return "error"
	case Aborted:
		return "aborted"
	default:
		return "unknown"
	}
}
