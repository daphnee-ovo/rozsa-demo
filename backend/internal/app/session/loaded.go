package session

import (
	"fmt"
	"path/filepath"
	"strings"
)

// A display projection, not a provider context or full message schema.
type HistoryBlock struct {
	Type string `json:"type"`
	Text string `json:"text"`
	Name string `json:"name"`
}
type HistoryMessage struct {
	EntryID     string         `json:"entryId"`
	Role        string         `json:"role"`
	Content     []HistoryBlock `json:"content"`
	DisplayText *string        `json:"displayText,omitempty"`
	ToolName    string         `json:"toolName,omitempty"`
	IsError     bool           `json:"isError,omitempty"`
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
	stem := filepath.Base(path)
	// Rust file_stem treats a leading dot as part of a dotfile's name.
	if dot := strings.LastIndex(stem, "."); dot > 0 {
		stem = stem[:dot]
	}
	result := LoadedSnapshot{Path: path, CatalogID: catalogID, SessionID: stem, Cwd: reader.Header().Cwd, Messages: []HistoryMessage{}, ContextMessageCount: len(reader.ContextMessages())}
	for _, entry := range reader.Entries() {
		if message, ok := entry.Payload.(MessageEntry); ok {
			view, err := historyMessage(entry.ID, message.Message)
			if err != nil {
				return LoadedSnapshot{}, err
			}
			result.Messages = append(result.Messages, view)
		}
	}
	if loadedSessions.tabs == nil {
		loadedSessions.tabs = make(map[string]loadedTab)
	}
	loadedSessions.tabs[path] = loadedTab{reader, result}
	loadedSessions.activePath = path
	return result, nil
}
func historyMessage(id string, message Message) (HistoryMessage, error) {
	history := HistoryMessage{EntryID: id, Content: []HistoryBlock{}}
	var blocks []ContentBlock
	switch typedMessage := message.(type) {
	case UserMessage:
		history.Role, history.DisplayText = "user", typedMessage.DisplayText
		switch content := typedMessage.Content.(type) {
		case UserText:
			blocks = []ContentBlock{TextBlock{Text: string(content)}}
		case UserBlocks:
			blocks = []ContentBlock(content)
		default:
			return history, fmt.Errorf("Unsupported user content %T", content)
		}
	case AssistantMessage:
		history.Role, blocks = "assistant", typedMessage.Content
	case ToolResultMessage:
		history.Role, history.ToolName, history.IsError, blocks = "toolResult", typedMessage.ToolName, typedMessage.IsError, typedMessage.Content
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
			history.Content = append(history.Content, HistoryBlock{Type: "image"})
		case ToolCallBlock:
			history.Content = append(history.Content, HistoryBlock{Type: "toolCall", Name: contentBlock.Name})
		default:
			return history, fmt.Errorf("Unsupported history block %T", block)
		}
	}
	return history, nil
}
