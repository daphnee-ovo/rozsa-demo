package session

import (
	"bufio"
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"slices"
	"unicode/utf8"
)

// Header preserves the persisted header; Open does not enforce version == 3.
type Header struct {
	Type               string
	Version            uint32
	ID, Timestamp, Cwd string
	ParentSession      *string
}

type Entry struct {
	ID        string
	ParentID  *string
	Timestamp string
	Payload   EntryPayload
}

// Sealed variants keep session metadata separate from provider messages.
type EntryPayload interface{ sessionEntry() }
type MessageEntry struct{ Message Message }
type ThinkingEffortChange struct{ Effort string }
type ModelChange struct{ Provider, ModelID string }
type Compaction struct {
	Summary, FirstKeptEntryID string
	TokensBefore              uint64
	Details                   json.RawMessage
	FromHook                  *bool
}
type Custom struct {
	Type string
	Data json.RawMessage
}
type Label struct {
	TargetID string
	Label    *string
}
type SessionInfo struct{ Name *string }

func (MessageEntry) sessionEntry()         {}
func (ThinkingEffortChange) sessionEntry() {}
func (ModelChange) sessionEntry()          {}
func (Compaction) sessionEntry()           {}
func (Custom) sessionEntry()               {}
func (Label) sessionEntry()                {}
func (SessionInfo) sessionEntry()          {}

type Message interface{ sessionMessage() }
type UserMessage struct {
	Content     UserContent
	DisplayText *string
	Timestamp   int64
}
type AssistantMessage struct {
	Content                   []ContentBlock
	API, Provider, Model      string // API/provider permit custom identifiers in Rust.
	ResponseModel, ResponseID *string
	Usage                     Usage
	StopReason                StopReason
	ErrorMessage              *string
	Timestamp                 int64
}
type ToolResultMessage struct {
	ToolCallID, ToolName string
	Content              []ContentBlock
	Details              json.RawMessage
	IsError              bool
	Timestamp            int64
}

func (UserMessage) sessionMessage()       {}
func (AssistantMessage) sessionMessage()  {}
func (ToolResultMessage) sessionMessage() {}

type StopReason uint8

const (
	Stop StopReason = iota
	Length
	ToolUse
	Error
	Aborted
)

type Usage struct {
	Input, Output, CacheRead, CacheWrite, TotalTokens uint64
	Cost                                              UsageCost
}
type UsageCost struct{ Input, Output, CacheRead, CacheWrite, Total float64 }
type UserContent interface{ userContent() }
type UserText string
type UserBlocks []ContentBlock

func (UserText) userContent()   {}
func (UserBlocks) userContent() {}

type ContentBlock interface{ contentBlock() }
type TextBlock struct {
	Text      string
	Signature *string
}
type ThinkingBlock struct {
	Thinking  string
	Signature *string
	Redacted  bool
}
type ImageBlock struct{ Data, MimeType string }
type ToolCallBlock struct {
	ID, Name  string
	Arguments json.RawMessage
}

func (TextBlock) contentBlock()     {}
func (ThinkingBlock) contentBlock() {}
func (ImageBlock) contentBlock()    {}
func (ToolCallBlock) contentBlock() {}

// Reader is an in-memory read-only tree. It owns no writable file handle.
type Reader struct {
	header Header
	path   string
	byID   map[string]Entry
	leafID *string
}

func (reader *Reader) Header() Header  { return reader.header }
func (reader *Reader) Path() string    { return reader.path }
func (reader *Reader) LeafID() *string { return reader.leafID }

// Open matches SessionManager::open: the first nonblank line must be a header;
// malformed entries are skipped, duplicate IDs replace the indexed value, and
// the last valid entry selects the leaf regardless of timestamp or entry kind.
func Open(path string) (*Reader, error) {
	file, err := os.Open(path)
	if err != nil {
		return nil, fmt.Errorf("Failed to open session file: %s: %w", path, err)
	}
	defer file.Close()
	reader := &Reader{path: path, byID: make(map[string]Entry)}
	input := bufio.NewReader(file)
	hasHeader := false
	for lineNumber := 1; ; lineNumber++ {
		line, readErr := input.ReadBytes('\n')
		if readErr != nil && readErr != io.EOF {
			return nil, fmt.Errorf("Failed to read line %d of %s: %w", lineNumber, path, readErr)
		}
		if !utf8.Valid(line) {
			return nil, fmt.Errorf("Failed to read line %d of %s: invalid UTF-8", lineNumber, path)
		}
		if len(bytes.TrimSpace(line)) != 0 {
			if !hasHeader {
				headerValue, ok := parseHeader(line)
				if !ok {
					return nil, fmt.Errorf("Invalid session header in %s", path)
				}
				if *headerValue.Type != "session" {
					return nil, fmt.Errorf("Missing 'session' header in %s", path)
				}
				reader.header = Header{*headerValue.Type, *headerValue.Version, *headerValue.ID, *headerValue.Timestamp, *headerValue.Cwd, headerValue.ParentSession}
				hasHeader = true
			} else if entry, ok := parseRestoredEntry(line); ok {
				reader.byID[entry.ID] = entry
				id := entry.ID
				reader.leafID = &id
			}
		}
		if readErr == io.EOF {
			break
		}
	}
	if !hasHeader {
		return nil, fmt.Errorf("Session file %s is empty or has no header", path)
	}
	return reader, nil
}

// Entries returns the active branch including metadata and pre-compaction
// history. A missing parent ends traversal, exactly as in Rust entries().
func (reader *Reader) Entries() []Entry {
	entries := []Entry{}
	for current := reader.leafID; current != nil; {
		entry, exists := reader.byID[*current]
		if !exists {
			break
		}
		entries = append(entries, entry)
		current = entry.ParentID
	}
	slices.Reverse(entries)
	return entries
}

// ContextMessages uses only the latest compaction on the active branch. A
// missing firstKeptEntryId restores the full branch; no summary is synthesized.
func (reader *Reader) ContextMessages() []Message {
	entries := reader.Entries()
	start := 0
	for i := len(entries) - 1; i >= 0; i-- {
		if compact, ok := entries[i].Payload.(Compaction); ok {
			for j, entry := range entries {
				if entry.ID == compact.FirstKeptEntryID {
					start = j
					break
				}
			}
			break
		}
	}
	messages := []Message{}
	for _, entry := range entries[start:] {
		if message, ok := entry.Payload.(MessageEntry); ok {
			messages = append(messages, message.Message)
		}
	}
	return messages
}

func parseHeader(data []byte) (header, bool) {
	var sessionHeader header
	ok := validJSONStrings(data) && uniqueFields(data, "type", "version", "id", "timestamp", "cwd", "parentSession") && decodeFields(data, &sessionHeader) == nil && sessionHeader.Type != nil && sessionHeader.Version != nil && sessionHeader.ID != nil && sessionHeader.Timestamp != nil && sessionHeader.Cwd != nil
	return sessionHeader, ok
}

func parseRestoredEntry(data []byte) (Entry, bool) {
	// Reuse the catalog's exact serde validation, rather than introducing a
	// second policy for deciding which persisted entries exist.
	projection, ok := parseEntry(data)
	if !ok {
		return Entry{}, false
	}
	var base struct {
		ID        string  `json:"id"`
		ParentID  *string `json:"parentId"`
		Timestamp string  `json:"timestamp"`
	}
	if decodeFields(data, &base) != nil {
		return Entry{}, false
	}
	fields := jsonFields(data)
	entry := Entry{ID: base.ID, ParentID: base.ParentID, Timestamp: base.Timestamp}
	switch projection.kind {
	case "message":
		entry.Payload = MessageEntry{restoreMessage(fields["message"])}
	case "thinking_effort_change", "thinking_level_change":
		effort := stringValue(fields["thinkingEffort"], stringValue(fields["thinkingLevel"], ""))
		entry.Payload = ThinkingEffortChange{effort}
	case "model_change":
		entry.Payload = ModelChange{stringValue(fields["provider"], ""), stringValue(fields["modelId"], "")}
	case "compaction":
		entry.Payload = Compaction{stringValue(fields["summary"], ""), stringValue(fields["firstKeptEntryId"], ""), uintValue(fields["tokensBefore"], 0), optionalJSON(fields["details"]), boolPointer(fields["fromHook"])}
	case "custom":
		entry.Payload = Custom{stringValue(fields["customType"], ""), optionalJSON(fields["data"])}
	case "label":
		entry.Payload = Label{stringValue(fields["targetId"], ""), stringPointer(fields["label"])}
	case "session_info":
		entry.Payload = SessionInfo{projection.name}
	}
	return entry, true
}

// These Value-style accessors deliberately reproduce types_serde.rs defaults:
// optional fields of the wrong type are ignored; required fields were already
// validated by parseEntry. RawMessage is reserved for arbitrary JSON payloads.
func jsonFields(data []byte) map[string]json.RawMessage {
	var fields map[string]json.RawMessage
	_ = json.Unmarshal(data, &fields)
	return fields
}
func stringPointer(data []byte) *string {
	var value string
	if len(data) == 0 || data[0] != '"' || json.Unmarshal(data, &value) != nil {
		return nil
	}
	return &value
}
func stringValue(data []byte, fallback string) string {
	if value := stringPointer(data); value != nil {
		return *value
	}
	return fallback
}
func boolPointer(data []byte) *bool {
	var value bool
	if !bytes.Equal(data, []byte("true")) && !bytes.Equal(data, []byte("false")) {
		return nil
	}
	_ = json.Unmarshal(data, &value)
	return &value
}
func intValue(data []byte) int64 {
	var value int64
	if json.Unmarshal(data, &value) != nil {
		return 0
	}
	return value
}
func uintValue(data []byte, fallback uint64) uint64 {
	var value uint64
	if len(data) == 0 || bytes.Equal(data, []byte("null")) || json.Unmarshal(data, &value) != nil {
		return fallback
	}
	return value
}
func floatValue(data []byte) float64 {
	var value float64
	if json.Unmarshal(data, &value) != nil {
		return 0
	}
	return value
}
func optionalJSON(data []byte) json.RawMessage {
	if bytes.Equal(data, []byte("null")) {
		return nil
	}
	return data
}

func restoreMessage(data []byte) Message {
	fields := jsonFields(data)
	timestamp := intValue(fields["timestamp"])
	switch stringValue(fields["role"], "") {
	case "user":
		var content UserContent
		if text := stringPointer(fields["content"]); text != nil {
			content = UserText(*text)
		} else {
			content = UserBlocks(restoreBlocks(fields["content"]))
		}
		return UserMessage{content, stringPointer(fields["displayText"]), timestamp}
	case "toolResult":
		isError := boolPointer(fields["isError"])
		details := fields["details"]
		if len(details) == 0 {
			details = json.RawMessage("null")
		}
		return ToolResultMessage{stringValue(fields["toolCallId"], ""), stringValue(fields["toolName"], ""), restoreBlocks(fields["content"]), details, isError != nil && *isError, timestamp}
	default: // parseEntry has validated that this is an assistant.
		reason := Stop
		switch stringValue(fields["stopReason"], "stop") {
		case "length", "Length":
			reason = Length
		case "toolUse", "ToolUse":
			reason = ToolUse
		case "error", "Error":
			reason = Error
		case "aborted", "Aborted":
			reason = Aborted
		}
		return AssistantMessage{restoreBlocks(fields["content"]), stringValue(fields["api"], "anthropic-messages"), stringValue(fields["provider"], "anthropic"), stringValue(fields["model"], ""), stringPointer(fields["responseModel"]), stringPointer(fields["responseId"]), restoreUsage(fields["usage"]), reason, stringPointer(fields["errorMessage"]), timestamp}
	}
}

func restoreBlocks(data []byte) []ContentBlock {
	var values []json.RawMessage
	_ = json.Unmarshal(data, &values)
	blocks := make([]ContentBlock, 0, len(values))
	for _, value := range values {
		fields := jsonFields(value)
		switch stringValue(fields["type"], "") {
		case "text":
			blocks = append(blocks, TextBlock{stringValue(fields["text"], ""), stringPointer(fields["textSignature"])})
		case "thinking":
			redacted := boolPointer(fields["redacted"])
			blocks = append(blocks, ThinkingBlock{stringValue(fields["thinking"], ""), stringPointer(fields["thinkingSignature"]), redacted != nil && *redacted})
		case "image":
			blocks = append(blocks, ImageBlock{stringValue(fields["data"], ""), stringValue(fields["mimeType"], "")})
		case "toolCall":
			arguments := fields["arguments"]
			if len(arguments) == 0 {
				arguments = json.RawMessage("{}")
			}
			blocks = append(blocks, ToolCallBlock{stringValue(fields["id"], ""), stringValue(fields["name"], ""), arguments})
		}
	}
	return blocks
}

func restoreUsage(data []byte) Usage {
	fields := jsonFields(data)
	usage := Usage{Input: uintValue(fields["input"], 0), Output: uintValue(fields["output"], 0), CacheRead: uintValue(fields["cacheRead"], 0), CacheWrite: uintValue(fields["cacheWrite"], 0)}
	usage.TotalTokens = uintValue(fields["totalTokens"], usage.Input+usage.Output+usage.CacheRead+usage.CacheWrite)
	costFields := jsonFields(fields["cost"])
	usage.Cost = UsageCost{floatValue(costFields["input"]), floatValue(costFields["output"]), floatValue(costFields["cacheRead"]), floatValue(costFields["cacheWrite"]), floatValue(costFields["total"])}
	return usage
}
