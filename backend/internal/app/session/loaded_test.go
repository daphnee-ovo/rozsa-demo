package session

import (
	"encoding/json"
	"os"
	"path/filepath"
	"reflect"
	"testing"
)

func TestHistoryBlockSerializationPreservesEmptyText(t *testing.T) {
	for _, kind := range []string{"text", "thinking"} {
		data, err := json.Marshal(HistoryBlock{Type: kind})
		if err != nil || string(data) != `{"type":"`+kind+`","text":""}` {
			t.Fatalf("%s block = %s, %v", kind, data, err)
		}
	}
	data, err := json.Marshal(HistoryBlock{Type: "toolCall", ID: "call", Name: ""})
	var toolCall map[string]any
	if err != nil || json.Unmarshal(data, &toolCall) != nil || toolCall["id"] != "call" || toolCall["name"] != "" {
		t.Fatalf("empty tool name = %s, %v", data, err)
	}
}

func TestLoadedHistoryContextAndIdentity(t *testing.T) {
	dir := fixtureDir(t)
	text := sessionHeader("header-id") +
		`{"type":"message","id":"a","timestamp":"1","message":{"role":"user","content":[{"type":"text","text":"old"},{"type":"image","data":"aGVsbG8=","mimeType":"image/png"}]}}` + "\n" +
		`{"type":"message","id":"b","parentId":"a","timestamp":"2","message":{"role":"assistant","content":[{"type":"thinking","thinking":"reason"},{"type":"text","text":"answer"},{"type":"toolCall","id":"call-read","name":"Read","arguments":{"path":"file.txt"}}],"api":"openai-completions","provider":"openai","model":"test","stopReason":"stop","errorMessage":"provider detail"}}` + "\n" +
		`{"type":"compaction","id":"c","parentId":"b","timestamp":"3","summary":"compressed","firstKeptEntryId":"b","tokensBefore":100}` + "\n" +
		`{"type":"message","id":"d","parentId":"c","timestamp":"4","message":{"role":"toolResult","toolCallId":"call-read","toolName":"Read","content":[{"type":"text","text":"result"}],"details":{"path":"file.txt"},"isError":false}}` + "\n"
	path := write(t, dir, "filename-id.jsonl", text)
	before, _ := os.Stat(path)
	loadedSessions := LoadedSessions{Catalog: Catalog{Directories: []string{dir}}}
	snapshot, err := loadedSessions.Switch(path, "header-id")
	if err != nil {
		t.Fatal(err)
	}
	if snapshot.SessionID != "filename-id" || snapshot.CatalogID != "header-id" || snapshot.Path != path || snapshot.Cwd != "/workspace" || snapshot.ContextMessageCount != 2 {
		t.Fatalf("identity/context: %+v", snapshot)
	}
	ids, roles := []string{}, []string{}
	for _, message := range snapshot.Messages {
		ids = append(ids, message.EntryID)
		roles = append(roles, message.Role)
	}
	if !reflect.DeepEqual(ids, []string{"a", "b", "d"}) || !reflect.DeepEqual(roles, []string{"user", "assistant", "toolResult"}) {
		t.Fatalf("history: %+v", snapshot.Messages)
	}
	userImage, assistantCall := snapshot.Messages[0].Content[1], snapshot.Messages[1].Content[2]
	if userImage.Type != "image" || userImage.Data != "aGVsbG8=" || userImage.MIMEType != "image/png" ||
		assistantCall.Type != "toolCall" || assistantCall.ID != "call-read" || string(assistantCall.Arguments) != `{"path":"file.txt"}` ||
		snapshot.Messages[1].ErrorMessage == nil || *snapshot.Messages[1].ErrorMessage != "provider detail" ||
		snapshot.Messages[2].ToolCallID != "call-read" || string(snapshot.Messages[2].Details) != `{"path":"file.txt"}` {
		t.Fatalf("renderable content was not preserved: %+v", snapshot.Messages)
	}
	if snapshot.Messages[1].Content[0].Type != "thinking" || snapshot.Messages[1].Content[1].Text != "answer" || snapshot.Messages[2].Content[0].Text != "result" {
		t.Fatalf("content: %+v", snapshot.Messages)
	}
	assertUnchanged(t, path, []byte(text), before)
	// Rust switch_session reuses Loaded tabs, rather than rereading on re-entry.
	if err := os.WriteFile(path, []byte("changed synthetic input\n"), 0600); err != nil {
		t.Fatal(err)
	}
	again, err := loadedSessions.Switch(path, "header-id")
	if err != nil || !reflect.DeepEqual(again, snapshot) {
		t.Fatalf("Loaded reuse: %+v %v", again, err)
	}
	if _, err := loadedSessions.Switch(path, "wrong"); err == nil {
		t.Fatal("cached identity mismatch accepted")
	}
}

func TestAdoptedWritableManagerSeedsSessionReadCache(t *testing.T) {
	dir := fixtureDir(t)
	path := write(t, dir, "adopted.jsonl", sessionHeader("adopted")+
		`{"type":"message","id":"user","parentId":null,"timestamp":"1","message":{"role":"user","content":"keep this history"}}`+"\n")
	manager, err := OpenManager(path)
	if err != nil {
		t.Fatal(err)
	}
	service := NewService(Catalog{Directories: []string{dir}})
	if err := service.AdoptManager(manager); err != nil {
		t.Fatal(err)
	}
	snapshot, err := service.Switch(t.Context(), path, "adopted")
	if err != nil || len(snapshot.Messages) != 1 || snapshot.Messages[0].EntryID != "user" {
		t.Fatalf("adopted snapshot = %+v, %v", snapshot, err)
	}
	writable, err := service.WritableManager(path, "adopted")
	if err != nil || writable != manager {
		t.Fatalf("writable manager = %p, want adopted manager %p; error %v", writable, manager, err)
	}
}

func TestLoadedFailureRetryAndEmpty(t *testing.T) {
	dir := fixtureDir(t)
	path := write(t, dir, "empty.jsonl", sessionHeader("empty"))
	loadedSessions := LoadedSessions{Catalog: Catalog{Directories: []string{dir}}}
	snapshot, err := loadedSessions.Switch(path, "empty")
	if err != nil || len(snapshot.Messages) != 0 || snapshot.Messages == nil || snapshot.ContextMessageCount != 0 {
		t.Fatalf("empty: %+v %v", snapshot, err)
	}
	bad := write(t, dir, "bad.jsonl", sessionHeader("bad")+"\xff\n") // catalog can read header; Open rejects invalid UTF-8.
	before, _ := os.Stat(bad)
	if _, err := loadedSessions.Switch(bad, "bad"); err == nil {
		t.Fatal("invalid UTF-8 restored")
	}
	assertUnchanged(t, bad, []byte(sessionHeader("bad")+"\xff\n"), before)
	if loadedSessions.activePath != path {
		t.Fatal("failure replaced active tab")
	}
	write(t, dir, "bad.jsonl", sessionHeader("bad"))
	if _, err := loadedSessions.Switch(bad, "bad"); err != nil {
		t.Fatalf("retry: %v", err)
	}
	if _, err := loadedSessions.Switch(filepath.Join(dir, "missing.jsonl"), "missing"); err == nil {
		t.Fatal("non-catalog path accepted")
	}
	if _, err := loadedSessions.Switch(path, "bad"); err == nil {
		t.Fatal("mismatched path/ID accepted")
	}
}

func TestLoadedDotfileSessionIdentity(t *testing.T) {
	dir := fixtureDir(t)
	path := write(t, dir, ".jsonl", sessionHeader("dotfile"))
	loadedSessions := LoadedSessions{Catalog: Catalog{Directories: []string{dir}}}
	snapshot, err := loadedSessions.Switch(path, "dotfile")
	if err != nil || snapshot.SessionID != ".jsonl" {
		t.Fatalf("Rust file_stem preserves dotfile: %+v %v", snapshot, err)
	}
}
