// Package session owns the read-only catalog and JSONL tree restoration. Source contract:
// crates/rozsa-app/src/session/manager.rs and rozsa-model/src/types_serde.rs.
package session

import (
	"bufio"
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"reflect"
	"sort"
	"strconv"
	"strings"
	"time"
	"unicode/utf8"
)

type Meta struct {
	Path              string
	ID                string
	Cwd               string
	Name              *string
	ParentSessionPath *string
	Created           string
	Modified          string
	MessageCount      uint32
	FirstMessage      string
	AllMessagesText   string
}

// ListEntry preserves the get_sessions wire fields, including snake_case.
type ListEntry struct {
	ID           string `json:"id"`
	Path         string `json:"path"`
	Name         string `json:"name"`
	Modified     string `json:"modified"`
	MessageCount uint32 `json:"message_count"`
}

type Catalog struct{ Directories []string }

func (catalog Catalog) GetSessions() ([]ListEntry, error) {
	if len(catalog.Directories) == 0 {
		return nil, fmt.Errorf("No session directories configured")
	}
	metas, err := ListDirs(catalog.Directories)
	if err != nil {
		return nil, err
	}
	entries := make([]ListEntry, 0, len(metas))
	for _, meta := range metas {
		name := meta.FirstMessage
		if meta.Name != nil {
			name = *meta.Name
		} else if chars := []rune(name); len(chars) > 50 {
			name = string(chars[:50]) + "..."
		}
		entries = append(entries, ListEntry{meta.ID, meta.Path, name, meta.Modified, meta.MessageCount})
	}
	return entries, nil
}

func ListDirs(dirs []string) ([]Meta, error) {
	byID := make(map[string]Meta)
	for _, dir := range dirs {
		metas, err := ListDir(dir)
		if err != nil {
			return nil, err
		}
		for _, meta := range metas {
			byID[meta.ID] = meta
		}
	}
	metas := make([]Meta, 0, len(byID))
	for _, meta := range byID {
		metas = append(metas, meta)
	}
	sort.SliceStable(metas, func(i, j int) bool { return metas[i].Modified > metas[j].Modified })
	return metas, nil
}

func ListDir(dir string) ([]Meta, error) {
	// Rust Path::exists treats stat failures as absent. This is the existing contract.
	if _, err := os.Stat(dir); err != nil {
		return []Meta{}, nil
	}
	directory, err := os.Open(dir)
	if err != nil {
		return nil, fmt.Errorf("Failed to read session dir: %s", dir)
	}
	defer directory.Close()
	metas := []Meta{}
	for {
		// Preserve OS enumeration order for equal timestamps / duplicate IDs within a layer.
		entries, err := directory.ReadDir(1)
		if err == io.EOF {
			break
		}
		if err != nil {
			return nil, fmt.Errorf("Failed to read session dir: %s", dir)
		}
		for _, entry := range entries {
			if filepath.Ext(entry.Name()) != ".jsonl" {
				continue
			}
			if meta, err := buildMeta(filepath.Join(dir, entry.Name())); err == nil {
				metas = append(metas, meta)
			}
		}
	}
	sort.SliceStable(metas, func(i, j int) bool { return metas[i].Modified > metas[j].Modified })
	return metas, nil
}

type header struct {
	Type          *string `json:"type"`
	Version       *uint32 `json:"version"`
	ID            *string `json:"id"`
	Timestamp     *string `json:"timestamp"`
	Cwd           *string `json:"cwd"`
	ParentSession *string `json:"parentSession"`
}

func buildMeta(path string) (Meta, error) {
	file, err := os.Open(path)
	if err != nil {
		return Meta{}, err
	}
	defer file.Close()
	reader := bufio.NewReader(file)
	var headerValue *header
	metadata := Meta{Path: path}
	var activity, nameTimestamp *string
	for {
		line, readErr := reader.ReadBytes('\n') // No Scanner token limit: Rust lines are unbounded.
		if readErr != nil && readErr != io.EOF {
			return Meta{}, readErr
		}
		if utf8.Valid(line) {
			line = bytes.TrimSpace(line)
			if len(line) > 0 {
				if headerValue == nil {
					value, ok := parseHeader(line)
					if !ok || *value.Type != "session" {
						return Meta{}, fmt.Errorf("Invalid header in %s", path)
					}
					headerValue = &value
				} else if entry, ok := parseEntry(line); ok {
					if activity == nil || entry.timestamp > *activity {
						timestamp := entry.timestamp
						activity = &timestamp
					}
					if entry.kind == "session_info" && (nameTimestamp == nil || entry.timestamp > *nameTimestamp) {
						timestamp := entry.timestamp
						nameTimestamp = &timestamp
						metadata.Name = entry.name
					}
					if entry.kind == "message" {
						metadata.MessageCount++
						if entry.text != "" {
							if metadata.FirstMessage == "" && entry.role == "user" {
								metadata.FirstMessage = entry.text
							}
							if metadata.AllMessagesText != "" {
								metadata.AllMessagesText += " "
							}
							metadata.AllMessagesText += entry.text
						}
					}
				}
			}
		}
		if readErr == io.EOF {
			break
		}
	}
	if headerValue == nil {
		return Meta{}, fmt.Errorf("No session header in %s", path)
	}
	metadata.ID, metadata.Cwd, metadata.Created, metadata.ParentSessionPath = *headerValue.ID, *headerValue.Cwd, *headerValue.Timestamp, headerValue.ParentSession
	if metadata.Name != nil && strings.TrimSpace(*metadata.Name) == "" {
		metadata.Name = nil
	}
	if metadata.FirstMessage == "" {
		metadata.FirstMessage = "(no messages)"
	}
	if activity != nil {
		metadata.Modified = *activity
	} else if stat, err := os.Stat(path); err == nil {
		metadata.Modified = rustTime(stat.ModTime())
	} else {
		metadata.Modified = metadata.Created
	}
	return metadata, nil
}

// chrono to_rfc3339 uses 0/3/6/9 fractional digits and +00:00, not Z.
func rustTime(timestamp time.Time) string {
	timestamp = timestamp.UTC()
	base := timestamp.Format("2006-01-02T15:04:05")
	nanoseconds := timestamp.Nanosecond()
	if nanoseconds == 0 {
		return base + "+00:00"
	}
	digits := 9
	if nanoseconds%1000000 == 0 {
		digits = 3
	} else if nanoseconds%1000 == 0 {
		digits = 6
	}
	return base + "." + fmt.Sprintf("%09d", nanoseconds)[:digits] + "+00:00"
}

// Only fields relevant to catalog decisions are retained; all persisted variants
// are validated so malformed entries cannot influence activity/count/name.
type catalogEntry struct {
	kind, timestamp, role, text string
	name                        *string
}

func parseEntry(data []byte) (catalogEntry, bool) {
	var base struct {
		Type      *string `json:"type"`
		ID        *string `json:"id"`
		Timestamp *string `json:"timestamp"`
		ParentID  *string `json:"parentId"`
	}
	if !validJSONStrings(data) || decodeFields(data, &base) != nil || base.Type == nil || base.ID == nil || base.Timestamp == nil {
		return catalogEntry{}, false
	}
	fields := []string{"type", "id", "timestamp", "parentId"}
	switch *base.Type {
	case "message":
		fields = append(fields, "message")
	case "session_info":
		fields = append(fields, "name")
	case "thinking_effort_change", "thinking_level_change":
		fields = append(fields, "thinkingEffort")
	case "model_change":
		fields = append(fields, "provider", "modelId")
	case "compaction":
		fields = append(fields, "summary", "firstKeptEntryId", "tokensBefore", "details", "fromHook")
	case "custom":
		fields = append(fields, "customType", "data")
	case "label":
		fields = append(fields, "targetId", "label")
	}
	if !uniqueFields(data, fields...) {
		return catalogEntry{}, false
	}
	entry := catalogEntry{kind: *base.Type, timestamp: *base.Timestamp}
	valid := false
	switch entry.kind {
	case "message":
		var messageFields struct {
			Message json.RawMessage `json:"message"`
		}
		if decodeFields(data, &messageFields) == nil {
			entry.role, entry.text, valid = parseMessage(messageFields.Message)
		}
	case "session_info":
		var sessionInfo struct {
			Name *string `json:"name"`
		}
		valid = decodeFields(data, &sessionInfo) == nil
		entry.name = sessionInfo.Name
	case "thinking_effort_change", "thinking_level_change":
		var effortChange struct {
			Effort *string `json:"thinkingEffort"`
			Level  *string `json:"thinkingLevel"`
		}
		valid = decodeFields(data, &effortChange) == nil && (effortChange.Effort != nil || effortChange.Level != nil) && !(effortChange.Effort != nil && effortChange.Level != nil)
	case "model_change":
		var modelChange struct {
			Provider *string `json:"provider"`
			Model    *string `json:"modelId"`
		}
		valid = decodeFields(data, &modelChange) == nil && modelChange.Provider != nil && modelChange.Model != nil
	case "compaction":
		var compaction struct {
			Summary  *string `json:"summary"`
			First    *string `json:"firstKeptEntryId"`
			Tokens   *uint64 `json:"tokensBefore"`
			FromHook *bool   `json:"fromHook"`
		}
		valid = decodeFields(data, &compaction) == nil && compaction.Summary != nil && compaction.First != nil && compaction.Tokens != nil
	case "custom":
		var customEntry struct {
			CustomType *string `json:"customType"`
		}
		valid = decodeFields(data, &customEntry) == nil && customEntry.CustomType != nil
	case "label":
		var labelEntry struct {
			Target *string `json:"targetId"`
			Label  *string `json:"label"`
		}
		valid = decodeFields(data, &labelEntry) == nil && labelEntry.Target != nil
	}
	return entry, valid
}

// encoding/json replaces lone UTF-16 surrogates; serde_json rejects them.
// Escaped backslashes are consumed first, so a literal "\\ud800" stays valid.
func validJSONStrings(data []byte) bool {
	for i := 0; i < len(data); i++ {
		if data[i] != '\\' {
			continue
		}
		i++
		if i >= len(data) {
			return false
		}
		if data[i] != 'u' {
			continue
		}
		if i+4 >= len(data) {
			return false
		}
		code, err := strconv.ParseUint(string(data[i+1:i+5]), 16, 16)
		if err != nil {
			return false
		}
		i += 4
		if code >= 0xdc00 && code <= 0xdfff {
			return false
		}
		if code >= 0xd800 && code <= 0xdbff {
			if i+6 >= len(data) || data[i+1] != '\\' || data[i+2] != 'u' {
				return false
			}
			low, err := strconv.ParseUint(string(data[i+3:i+7]), 16, 16)
			if err != nil || low < 0xdc00 || low > 0xdfff {
				return false
			}
			i += 6
		}
	}
	return true
}

// Derived serde structs reject duplicate known fields. Message/content custom
// deserializers instead consume Value; their last-key-wins behavior is retained.
func uniqueFields(data []byte, fields ...string) bool {
	known := make(map[string]bool, len(fields))
	for _, field := range fields {
		known[field] = false
	}
	decoder := json.NewDecoder(bytes.NewReader(data))
	start, err := decoder.Token()
	if err != nil || start != json.Delim('{') {
		return false
	}
	for decoder.More() {
		token, err := decoder.Token()
		if err != nil {
			return false
		}
		key, ok := token.(string)
		if !ok {
			return false
		}
		if _, alias := known["thinkingEffort"]; alias && key == "thinkingLevel" {
			key = "thinkingEffort"
		}
		if seen, exists := known[key]; exists {
			if seen {
				return false
			}
			known[key] = true
		}
		var value json.RawMessage
		if decoder.Decode(&value) != nil {
			return false
		}
	}
	_, err = decoder.Token()
	return err == nil
}

// Decode the explicit projection's literal tags. Go's normal struct decoder
// accepts case-insensitive keys; the Rust/Value contract requires exact names.
// Reflection is confined to this JSON boundary, never used for domain dispatch.
func decodeFields(data []byte, projection any) error {
	var fields map[string]json.RawMessage
	if err := json.Unmarshal(data, &fields); err != nil {
		return err
	}
	if fields == nil {
		return fmt.Errorf("expected JSON object")
	}
	dst := reflect.ValueOf(projection).Elem()
	for i := 0; i < dst.NumField(); i++ {
		key := dst.Type().Field(i).Tag.Get("json")
		if value, exists := fields[key]; exists {
			if err := json.Unmarshal(value, dst.Field(i).Addr().Interface()); err != nil {
				return err
			}
		}
	}
	return nil
}

func parseMessage(data []byte) (string, string, bool) {
	var messageFields struct {
		Role    *string         `json:"role"`
		Content json.RawMessage `json:"content"`
	}
	if decodeFields(data, &messageFields) != nil || messageFields.Role == nil {
		return "", "", false
	}
	role := *messageFields.Role
	if role == "user" {
		var text string
		if len(messageFields.Content) > 0 && messageFields.Content[0] == '"' && json.Unmarshal(messageFields.Content, &text) == nil {
			return role, text, true
		}
		if len(messageFields.Content) == 0 {
			return role, "", false
		}
	} else if role == "assistant" {
		var assistantFields struct {
			Model *string `json:"model"`
		}
		if decodeFields(data, &assistantFields) != nil || assistantFields.Model == nil {
			return role, "", false
		}
	} else if role == "toolResult" {
		var toolResultFields struct {
			ID   *string `json:"toolCallId"`
			Name *string `json:"toolName"`
		}
		if decodeFields(data, &toolResultFields) != nil || toolResultFields.ID == nil || toolResultFields.Name == nil {
			return role, "", false
		}
	} else {
		return role, "", false
	}
	text, ok := blockText(messageFields.Content)
	if role == "toolResult" {
		text = ""
	}
	return role, text, ok
}

func blockText(data []byte) (string, bool) {
	if len(data) == 0 {
		return "", true
	} // assistant/toolResult missing content = [].
	if data[0] != '[' {
		return "", false
	}
	var blocks []json.RawMessage
	if json.Unmarshal(data, &blocks) != nil {
		return "", false
	}
	texts := []string{}
	for _, block := range blocks {
		var tag struct {
			Type string `json:"type"`
		}
		if decodeFields(block, &tag) != nil {
			return "", false
		}
		switch tag.Type {
		case "text":
			var textBlock struct {
				Text *string `json:"text"`
			}
			if decodeFields(block, &textBlock) != nil || textBlock.Text == nil {
				return "", false
			}
			texts = append(texts, *textBlock.Text)
		case "thinking":
			var thinkingBlock struct {
				Thinking *string `json:"thinking"`
			}
			if decodeFields(block, &thinkingBlock) != nil || thinkingBlock.Thinking == nil {
				return "", false
			}
		case "image":
			var imageBlock struct {
				Data *string `json:"data"`
				Mime *string `json:"mimeType"`
			}
			if decodeFields(block, &imageBlock) != nil || imageBlock.Data == nil || imageBlock.Mime == nil {
				return "", false
			}
		case "toolCall":
			var toolCall struct {
				ID   *string `json:"id"`
				Name *string `json:"name"`
			}
			if decodeFields(block, &toolCall) != nil || toolCall.ID == nil || toolCall.Name == nil {
				return "", false
			}
		default:
			return "", false
		}
	}
	return strings.Join(texts, " "), true
}
