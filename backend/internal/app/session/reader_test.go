package session

import (
	"bytes"
	"encoding/json"
	"fmt"
	"math/big"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
)

// The Rust reference consumes this same file. Origins: manager.rs
// open/entries/context_messages and agent_session_restore_test.rs's three roles.
func TestSharedReaderFixtures(t *testing.T) {
	data, err := os.ReadFile("testdata/reader-cases.json")
	if err != nil {
		t.Fatal(err)
	}
	var cases []struct {
		Name        string            `json:"name"`
		Lines       []json.RawMessage `json:"lines"`
		Leaf        *string           `json:"leaf"`
		History     []string          `json:"history"`
		Messages    json.RawMessage   `json:"messages"`
		ErrorPrefix string            `json:"errorPrefix"`
	}
	if err := json.Unmarshal(data, &cases); err != nil {
		t.Fatal(err)
	}
	for _, test := range cases {
		t.Run(test.Name, func(t *testing.T) {
			var text strings.Builder
			for _, line := range test.Lines {
				if line[0] == '"' {
					var raw string
					if err := json.Unmarshal(line, &raw); err != nil {
						t.Fatal(err)
					}
					text.WriteString(raw)
				} else {
					text.Write(line)
				}
				text.WriteByte('\n')
			}
			path := write(t, fixtureDir(t), "session.jsonl", text.String())
			before, err := os.Stat(path)
			if err != nil {
				t.Fatal(err)
			}
			reader, err := Open(path)
			if test.ErrorPrefix != "" {
				if err == nil || !strings.HasPrefix(err.Error(), test.ErrorPrefix) {
					t.Fatalf("expected %q; got %v", test.ErrorPrefix, err)
				}
			} else {
				if err != nil {
					t.Fatal(err)
				}
				if !reflect.DeepEqual(reader.LeafID(), test.Leaf) {
					t.Fatalf("leaf: %v, want %v", reader.LeafID(), test.Leaf)
				}
				ids := []string{}
				for _, entry := range reader.Entries() {
					ids = append(ids, entry.ID)
				}
				if !reflect.DeepEqual(ids, test.History) {
					t.Fatalf("history: %v, want %v", ids, test.History)
				}
				messages := []any{}
				for _, message := range reader.ContextMessages() {
					messages = append(messages, messageWire(message))
				}
				actual, err := json.Marshal(messages)
				if err != nil {
					t.Fatal(err)
				}
				assertJSONEqual(t, actual, test.Messages)
				if reader.Path() != path {
					t.Fatal("path changed")
				}
			}
			assertUnchanged(t, path, []byte(text.String()), before)
		})
	}
}

func assertUnchanged(t *testing.T, path string, original []byte, before os.FileInfo) {
	t.Helper()
	after, err := os.Stat(path)
	if err != nil {
		t.Fatal(err)
	}
	actual, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(actual, original) || !before.ModTime().Equal(after.ModTime()) || before.Mode() != after.Mode() {
		t.Fatal("read-only operation modified file")
	}
}

func assertJSONEqual(t *testing.T, actual, expected []byte) {
	t.Helper()
	decode := func(data []byte) any {
		decoder := json.NewDecoder(bytes.NewReader(data))
		decoder.UseNumber() // Compare u64/i64 exactly, never round through float64.
		var value any
		if err := decoder.Decode(&value); err != nil {
			t.Fatal(err)
		}
		return normalizeJSONNumbers(value)
	}
	if !reflect.DeepEqual(decode(actual), decode(expected)) {
		t.Fatalf("messages: %s\nwant: %s", actual, expected)
	}
}

// JSON does not prescribe float spelling. Only numeric spelling (0 vs 0.0)
// is normalized; exact integer values and all array/message ordering remain.
func normalizeJSONNumbers(value any) any {
	switch typedValue := value.(type) {
	case json.Number:
		rational, ok := new(big.Rat).SetString(string(typedValue))
		if !ok {
			panic("invalid JSON number")
		}
		return struct{ Number string }{rational.RatString()}
	case []any:
		for i := range typedValue {
			typedValue[i] = normalizeJSONNumbers(typedValue[i])
		}
	case map[string]any:
		for key := range typedValue {
			typedValue[key] = normalizeJSONNumbers(typedValue[key])
		}
	}
	return value
}

// Test-only projection to Rust's normalized Serialize output. Production
// transport wiring and message serialization remain with their own slices.
func blocksWire(blocks []ContentBlock) []any {
	result := []any{}
	for _, block := range blocks {
		var value map[string]any
		switch contentBlock := block.(type) {
		case TextBlock:
			value = map[string]any{"type": "text", "text": contentBlock.Text}
			if contentBlock.Signature != nil {
				value["textSignature"] = *contentBlock.Signature
			}
		case ThinkingBlock:
			value = map[string]any{"type": "thinking", "thinking": contentBlock.Thinking}
			if contentBlock.Signature != nil {
				value["thinkingSignature"] = *contentBlock.Signature
			}
			if contentBlock.Redacted {
				value["redacted"] = true
			}
		case ImageBlock:
			value = map[string]any{"type": "image", "data": contentBlock.Data, "mimeType": contentBlock.MimeType}
		case ToolCallBlock:
			value = map[string]any{"type": "toolCall", "id": contentBlock.ID, "name": contentBlock.Name, "arguments": contentBlock.Arguments}
		default:
			panic(fmt.Sprintf("unexpected block %T", block))
		}
		result = append(result, value)
	}
	return result
}

func messageWire(message Message) map[string]any {
	switch typedMessage := message.(type) {
	case UserMessage:
		var blocks []ContentBlock
		switch content := typedMessage.Content.(type) {
		case UserText:
			blocks = []ContentBlock{TextBlock{Text: string(content)}}
		case UserBlocks:
			blocks = []ContentBlock(content)
		default:
			panic("unknown user content")
		}
		wireMessage := map[string]any{"role": "user", "content": blocksWire(blocks), "timestamp": typedMessage.Timestamp}
		if typedMessage.DisplayText != nil {
			wireMessage["displayText"] = *typedMessage.DisplayText
		}
		return wireMessage
	case AssistantMessage:
		cost := typedMessage.Usage.Cost
		wireMessage := map[string]any{"role": "assistant", "content": blocksWire(typedMessage.Content), "api": typedMessage.API, "provider": typedMessage.Provider, "model": typedMessage.Model, "timestamp": typedMessage.Timestamp,
			"stopReason": []string{"stop", "length", "toolUse", "error", "aborted"}[typedMessage.StopReason],
			"usage":      map[string]any{"input": typedMessage.Usage.Input, "output": typedMessage.Usage.Output, "cacheRead": typedMessage.Usage.CacheRead, "cacheWrite": typedMessage.Usage.CacheWrite, "totalTokens": typedMessage.Usage.TotalTokens, "cost": map[string]any{"input": cost.Input, "output": cost.Output, "cacheRead": cost.CacheRead, "cacheWrite": cost.CacheWrite, "total": cost.Total}}}
		if typedMessage.ResponseModel != nil {
			wireMessage["responseModel"] = *typedMessage.ResponseModel
		}
		if typedMessage.ResponseID != nil {
			wireMessage["responseId"] = *typedMessage.ResponseID
		}
		if typedMessage.ErrorMessage != nil {
			wireMessage["errorMessage"] = *typedMessage.ErrorMessage
		}
		return wireMessage
	case ToolResultMessage:
		wireMessage := map[string]any{"role": "toolResult", "toolCallId": typedMessage.ToolCallID, "toolName": typedMessage.ToolName, "content": blocksWire(typedMessage.Content), "isError": typedMessage.IsError, "timestamp": typedMessage.Timestamp}
		if !bytes.Equal(typedMessage.Details, []byte("null")) {
			wireMessage["details"] = typedMessage.Details
		}
		return wireMessage
	default:
		panic(fmt.Sprintf("unexpected message %T", message))
	}
}

func TestReaderHeaderAndEntryPayloads(t *testing.T) {
	path := write(t, fixtureDir(t), "header.jsonl", sessionHeader("id"))
	reader, err := Open(path)
	if err != nil {
		t.Fatal(err)
	}
	header := reader.Header()
	if header.Type != "session" || header.Version != 3 || header.ID != "id" || header.Timestamp != "2026-06-23T09:51:39.256Z" || header.Cwd != "/workspace" || header.ParentSession == nil || *header.ParentSession != "/synthetic/origin.jsonl" {
		t.Fatalf("header: %+v", header)
	}
	inputs := []string{
		`{"type":"thinking_level_change","thinkingLevel":"high"}`,
		`{"type":"thinking_effort_change","thinkingEffort":"low"}`,
		`{"type":"model_change","provider":"p","modelId":"m"}`,
		`{"type":"compaction","summary":"s","firstKeptEntryId":"kept","tokensBefore":18446744073709551615,"details":{"test":true},"fromHook":false}`,
		`{"type":"custom","customType":"x","data":[1,"a",null]}`,
		`{"type":"label","targetId":"target","label":"bookmark"}`,
		`{"type":"session_info","name":"name"}`,
	}
	name := "name"
	label := "bookmark"
	flag := false
	expected := []EntryPayload{ThinkingEffortChange{"high"}, ThinkingEffortChange{"low"}, ModelChange{"p", "m"}, Compaction{"s", "kept", ^uint64(0), json.RawMessage(`{"test":true}`), &flag}, Custom{"x", json.RawMessage(`[1,"a",null]`)}, Label{"target", &label}, SessionInfo{&name}}
	for i, input := range inputs {
		input = strings.TrimSuffix(input, "}") + `,"id":"e","timestamp":"t","parentId":"previous"}`
		entry, ok := parseRestoredEntry([]byte(input))
		if !ok || entry.ID != "e" || entry.ParentID == nil || *entry.ParentID != "previous" || entry.Timestamp != "t" || !reflect.DeepEqual(entry.Payload, expected[i]) {
			t.Fatalf("entry %d: %+v", i, entry)
		}
	}
}

func TestReaderIOAndUnboundedLines(t *testing.T) {
	dir := fixtureDir(t)
	if _, err := Open(filepath.Join(dir, "missing.jsonl")); err == nil || !strings.HasPrefix(err.Error(), "Failed to open session file: ") {
		t.Fatalf("missing file: %v", err)
	}
	for _, text := range []string{sessionHeader("s") + "\xff\n", "\xff\n"} {
		path := write(t, dir, "utf8.jsonl", text)
		if _, err := Open(path); err == nil || !strings.HasPrefix(err.Error(), "Failed to read line ") {
			t.Fatalf("invalid UTF-8: %v", err)
		}
	}
	long := strings.Repeat("玫🌹", 20000)
	content, _ := json.Marshal(long)
	path := write(t, dir, "long.jsonl", sessionHeader("s")+strings.TrimSuffix(message("user", string(content), "t"), "\n"))
	reader, err := Open(path)
	if err != nil {
		t.Fatal(err)
	}
	if reader.ContextMessages()[0].(UserMessage).Content.(UserText) != UserText(long) {
		t.Fatal("long final line truncated")
	}
}
