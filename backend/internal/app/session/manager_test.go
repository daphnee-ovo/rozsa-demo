package session

import (
	"bytes"
	"context"
	"encoding/hex"
	"encoding/json"
	"errors"
	"io"
	"os"
	"path/filepath"
	"reflect"
	"runtime"
	"strings"
	"sync"
	"testing"
	"time"
)

func managerForTest(t *testing.T) *Manager {
	t.Helper()
	manager, err := Create(filepath.Join(t.TempDir(), "session.jsonl"), "session", "/workspace", nil)
	if err != nil {
		t.Fatal(err)
	}
	return manager
}

func appendForTest(t *testing.T, manager *Manager, payload EntryPayload) string {
	t.Helper()
	id, err := manager.Append(payload)
	if err != nil {
		t.Fatal(err)
	}
	return id
}

func readForTest(t *testing.T, path string) []byte {
	t.Helper()
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	return data
}

func reopenForTest(t *testing.T, manager *Manager) *Manager {
	t.Helper()
	reopened, err := OpenManager(manager.Path())
	if err != nil {
		t.Fatal(err)
	}
	return reopened
}

func sameMessages(t *testing.T, actual, expected []Message) {
	t.Helper()
	project := func(messages []Message) []byte {
		values := []any{}
		for _, message := range messages {
			values = append(values, messageWire(message))
		}
		data, err := json.Marshal(values)
		if err != nil {
			t.Fatal(err)
		}
		return data
	}
	assertJSONEqual(t, project(actual), project(expected))
}

// References: manager.rs create/create_lazy/append_entry/set_cwd and
// working_directory_persistence_test.rs. Reads and no-op cwd do not create files.
func TestManagerLazyTriggers(t *testing.T) {
	name := "name"
	operations := map[string]func(*Manager) error{
		"message": func(manager *Manager) error {
			_, err := manager.AppendMessage(UserMessage{Content: UserText("hello")})
			return err
		},
		"model":    func(manager *Manager) error { _, err := manager.AppendModelChange("provider", "model"); return err },
		"thinking": func(manager *Manager) error { _, err := manager.AppendThinkingEffortChange("high"); return err },
		"compaction": func(manager *Manager) error {
			_, err := manager.AppendCompaction("summary", "missing", 9, nil, nil)
			return err
		},
		"custom": func(manager *Manager) error { _, err := manager.AppendCustom("extension", nil); return err },
		"name":   func(manager *Manager) error { _, err := manager.AppendSessionInfo(&name); return err },
		"cwd":    func(manager *Manager) error { return manager.SetCwd("/next") },
	}
	for name, operation := range operations {
		t.Run(name, func(t *testing.T) {
			path := filepath.Join(t.TempDir(), "nested", "session.jsonl")
			origin := "/synthetic/origin.jsonl"
			manager := CreateLazy(path, "id", "/workspace", &origin)
			origin = "mutated"
			if manager.Cwd() != "/workspace" || manager.SessionID() != "id" || manager.Path() != path || manager.LeafID() != nil || len(manager.Entries()) != 0 || len(manager.ContextMessages()) != 0 || manager.CurrentName() != nil || manager.LatestCustom("missing") != nil {
				t.Fatal("incorrect lazy state")
			}
			if err := manager.SetCwd("/workspace"); err != nil {
				t.Fatal(err)
			}
			if _, err := manager.AppendLabel("missing", nil); err == nil {
				t.Fatal("missing label target accepted")
			}
			if _, err := os.Stat(filepath.Dir(path)); !errors.Is(err, os.ErrNotExist) {
				t.Fatalf("lazy read created directory: %v", err)
			}
			if err := operation(manager); err != nil {
				t.Fatal(err)
			}
			header := reopenForTest(t, manager).Header()
			if header.Type != "session" || header.Version != 3 || header.ID != "id" || header.ParentSession == nil || *header.ParentSession != "/synthetic/origin.jsonl" {
				t.Fatalf("header: %+v", header)
			}
			if _, err := time.Parse(time.RFC3339Nano, header.Timestamp); err != nil {
				t.Fatal(err)
			}
			if name == "cwd" && header.Cwd != "/next" {
				t.Fatal("cwd did not persist")
			}
		})
	}
	manager := managerForTest(t)
	appendForTest(t, manager, Custom{Type: "old"})
	replacement, err := Create(manager.Path(), "replacement", "/new", nil)
	if err != nil {
		t.Fatal(err)
	}
	if len(replacement.Entries()) != 0 || reopenForTest(t, replacement).SessionID() != "replacement" {
		t.Fatal("create must replace an existing file")
	}
	if bytes.Contains(readForTest(t, replacement.Path()), []byte("parentSession")) {
		t.Fatal("absent parentSession was serialized")
	}
}

// Every old reader fixture is copied into an isolated writable file. Error
// fixtures retain the same error; valid trees continue from the original leaf.
func TestManagerOldFixturesAppendReopenContinue(t *testing.T) {
	var cases []struct {
		Name        string
		Lines       []json.RawMessage
		ErrorPrefix string
	}
	if err := json.Unmarshal(readForTest(t, "testdata/reader-cases.json"), &cases); err != nil {
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
			path := write(t, t.TempDir(), "copy.jsonl", text.String())
			manager, err := OpenManager(path)
			if test.ErrorPrefix != "" {
				if err == nil || !strings.HasPrefix(err.Error(), test.ErrorPrefix) {
					t.Fatalf("error: %v, want %s", err, test.ErrorPrefix)
				}
				if string(readForTest(t, path)) != text.String() {
					t.Fatal("error modified fixture")
				}
				return
			}
			if err != nil {
				t.Fatal(err)
			}
			previousLeaf, previousMessages := manager.LeafID(), manager.ContextMessages()
			message := UserMessage{Content: UserText("continued"), Timestamp: 99}
			id, err := manager.AppendMessage(message)
			if err != nil {
				t.Fatal(err)
			}
			if len(id) != 8 {
				t.Fatalf("entry ID %q", id)
			}
			if _, err := hex.DecodeString(id); err != nil {
				t.Fatal(err)
			}
			entries := manager.Entries()
			if !reflect.DeepEqual(entries[len(entries)-1].ParentID, previousLeaf) {
				t.Fatal("append did not link old leaf")
			}
			if !bytes.HasPrefix(readForTest(t, path), []byte(text.String())) {
				t.Fatal("append changed existing bytes")
			}
			reopened := reopenForTest(t, manager)
			if *reopened.LeafID() != id || reopened.SessionID() != manager.SessionID() {
				t.Fatal("reopen lost identity or leaf")
			}
			sameMessages(t, reopened.ContextMessages(), append(previousMessages, message))
			second := appendForTest(t, reopened, Custom{Type: "resumed", Data: json.RawMessage(`{"ok":true}`)})
			if *reopenForTest(t, reopened).LeafID() != second {
				t.Fatal("reopened append lost")
			}
			branch := CreateLazy(filepath.Join(t.TempDir(), "branch.jsonl"), "branch", reopened.Cwd(), &path)
			if err := branch.CopyContextMessagesFromPath(path); err != nil {
				t.Fatal(err)
			}
			sameMessages(t, reopenForTest(t, branch).ContextMessages(), reopened.ContextMessages())
			if *branch.Header().ParentSession != path {
				t.Fatal("branch origin missing")
			}
			for _, entry := range branch.Entries() {
				if _, ok := entry.Payload.(MessageEntry); !ok {
					t.Fatal("branch copied metadata")
				}
			}
		})
	}
}

func TestManagerAllPayloadsAndMessageSerialization(t *testing.T) {
	manager := managerForTest(t)
	signature, display, response, errorMessage, label, name := "signature", "display", "response", "cancelled", "bookmark", "title"
	flag := false
	blocks := []ContentBlock{TextBlock{"text", &signature}, ThinkingBlock{"reason", &signature, true}, ImageBlock{"base64", "image/png"}, ToolCallBlock{"call", "tool", json.RawMessage(`{"large":18446744073709551615}`)}}
	messages := []Message{
		UserMessage{UserText("plain"), &display, -1},
		UserMessage{UserBlocks(blocks), nil, 2},
		AssistantMessage{blocks, "custom-api", "custom-provider", "model", &response, &response, Usage{1, 2, 3, 4, 10, UsageCost{0.1, 0.2, 0.3, 0.4, 1}}, Aborted, &errorMessage, 3},
		ToolResultMessage{"call", "tool", blocks, json.RawMessage(`{"result":true}`), true, 4},
		ToolResultMessage{ToolCallID: "call", ToolName: "tool", Details: json.RawMessage("null")},
	}
	for _, message := range messages {
		if _, err := manager.AppendMessage(message); err != nil {
			t.Fatal(err)
		}
		fields := jsonFields(bytes.Split(bytes.TrimSpace(readForTest(t, manager.Path())), []byte{'\n'})[len(manager.Entries())])
		expected, err := json.Marshal(messageWire(message))
		if err != nil {
			t.Fatal(err)
		}
		assertJSONEqual(t, fields["message"], expected)
	}
	first := manager.Entries()[0].ID
	if _, err := manager.AppendModelChange("provider", "model"); err != nil {
		t.Fatal(err)
	}
	if _, err := manager.AppendThinkingEffortChange("high"); err != nil {
		t.Fatal(err)
	}
	if _, err := manager.AppendCompaction("summary", first, ^uint64(0), json.RawMessage(`{"details":[1]}`), &flag); err != nil {
		t.Fatal(err)
	}
	if _, err := manager.AppendCustom("extension", json.RawMessage(`[1,null]`)); err != nil {
		t.Fatal(err)
	}
	if _, err := manager.AppendLabel(first, &label); err != nil {
		t.Fatal(err)
	}
	if _, err := manager.AppendSessionInfo(&name); err != nil {
		t.Fatal(err)
	}
	before := readForTest(t, manager.Path())
	if _, err := manager.AppendLabel("unknown", nil); err == nil {
		t.Fatal("invalid label accepted")
	}
	if !bytes.Equal(before, readForTest(t, manager.Path())) {
		t.Fatal("invalid label changed file")
	}
	reopened := reopenForTest(t, manager)
	sameMessages(t, reopened.ContextMessages(), messages)
	if !reflect.DeepEqual(manager.Entries()[len(messages):], reopened.Entries()[len(messages):]) {
		t.Fatal("metadata changed on reopen")
	}
	if reopened.CurrentName() == nil || *reopened.CurrentName() != name || reopened.LatestCustom("extension") == nil {
		t.Fatal("latest metadata not restored")
	}
	for i, entry := range reopened.Entries() {
		if i == 0 && entry.ParentID != nil {
			t.Fatal("root parent must be null")
		}
		if i > 0 && *entry.ParentID != reopened.Entries()[i-1].ID {
			t.Fatal("broken append chain")
		}
	}
	if _, err := manager.AppendLabel(first, nil); err != nil {
		t.Fatal(err)
	}
	if _, err := manager.AppendSessionInfo(nil); err != nil {
		t.Fatal(err)
	}
	if reopenForTest(t, manager).CurrentName() != nil {
		t.Fatal("name clear lost")
	}
	if _, err := manager.AppendCompaction("summary", first, 0, nil, nil); err != nil {
		t.Fatal(err)
	}
	if _, err := manager.AppendCustom("empty", nil); err != nil {
		t.Fatal(err)
	}
	lastLines := bytes.Split(bytes.TrimSpace(readForTest(t, manager.Path())), []byte{'\n'})
	for _, field := range []string{"data", "details", "fromHook", "label", "name"} {
		for _, line := range lastLines[len(lastLines)-4:] {
			if _, exists := jsonFields(line)[field]; exists {
				t.Fatalf("absent %s should be omitted: %s", field, line)
			}
		}
	}
}

func TestManagerSetCwdAndRenamePreserveHistory(t *testing.T) {
	manager := managerForTest(t)
	appendForTest(t, manager, MessageEntry{UserMessage{Content: UserText("history")}})
	file, err := os.OpenFile(manager.Path(), os.O_APPEND|os.O_WRONLY, 0)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := file.WriteString("\r\nmalformed\n{\"truncated\":"); err != nil {
		t.Fatal(err)
	}
	if err := file.Close(); err != nil {
		t.Fatal(err)
	}
	before := readForTest(t, manager.Path())
	_, tail, _ := bytes.Cut(before, []byte{'\n'})
	header := manager.Header()
	if err := manager.SetCwd("/next"); err != nil {
		t.Fatal(err)
	}
	_, afterTail, _ := bytes.Cut(readForTest(t, manager.Path()), []byte{'\n'})
	if !bytes.Equal(tail, afterTail) {
		t.Fatal("cwd rewrite changed trailing bytes")
	}
	header.Cwd = "/next"
	if !reflect.DeepEqual(reopenForTest(t, manager).Header(), header) {
		t.Fatal("cwd rewrite changed other header fields")
	}
	// A separate clean file verifies rename as an append, without repairing tails.
	manager = managerForTest(t)
	oldLeaf := appendForTest(t, manager, MessageEntry{UserMessage{Content: UserText("history")}})
	before = readForTest(t, manager.Path())
	name := " renamed "
	if err := Rename(manager.Path(), &name); err != nil {
		t.Fatal(err)
	}
	if !bytes.HasPrefix(readForTest(t, manager.Path()), before) {
		t.Fatal("rename rewrote history")
	}
	reopened := reopenForTest(t, manager)
	entries := reopened.Entries()
	if len(entries) != 2 || *entries[1].ParentID != oldLeaf || *reopened.CurrentName() != name {
		t.Fatal("rename did not append linked session_info")
	}
	if manager.CurrentName() != nil || *manager.LeafID() != oldLeaf {
		t.Fatal("separate rename silently refreshed an active instance")
	}
	if err := Rename(manager.Path(), nil); err != nil {
		t.Fatal(err)
	}
	if reopenForTest(t, manager).CurrentName() != nil {
		t.Fatal("rename clear failed")
	}
}

type failingAppend struct {
	file               *os.File
	limit              int
	writeErr, closeErr error
}

func (writer failingAppend) Write(data []byte) (int, error) {
	if writer.limit < len(data) {
		data = data[:writer.limit]
	}
	if writer.file == nil {
		return len(data), writer.writeErr
	}
	written, err := writer.file.Write(data)
	if err != nil {
		return written, err
	}
	return written, writer.writeErr
}
func (writer failingAppend) Close() error {
	if writer.file != nil {
		if err := writer.file.Close(); err != nil {
			return err
		}
	}
	return writer.closeErr
}

func TestManagerAppendFailureDoesNotAdvanceMemory(t *testing.T) {
	for _, test := range []struct {
		name                         string
		limit                        int
		writeErr, closeErr, expected error
	}{
		{"write", 0, io.ErrClosedPipe, nil, io.ErrClosedPipe},
		{"cancelled", 0, context.Canceled, nil, context.Canceled},
		{"short-write", 4, nil, nil, io.ErrShortWrite},
		{"close", 1 << 20, nil, io.ErrClosedPipe, io.ErrClosedPipe},
	} {
		t.Run(test.name, func(t *testing.T) {
			manager := managerForTest(t)
			appendForTest(t, manager, Custom{Type: "previous"})
			entries, leaf := manager.Entries(), manager.LeafID()
			manager.openAppend = func(string) (io.WriteCloser, error) {
				return failingAppend{limit: test.limit, writeErr: test.writeErr, closeErr: test.closeErr}, nil
			}
			if id, err := manager.AppendCustom("failed", nil); id != "" || !errors.Is(err, test.expected) {
				t.Fatalf("failure reported success: %q %v", id, err)
			}
			if !reflect.DeepEqual(manager.Entries(), entries) || *manager.LeafID() != *leaf {
				t.Fatal("failed append advanced state")
			}
		})
	}
	manager := managerForTest(t)
	appendForTest(t, manager, Custom{Type: "previous"})
	leaf := *manager.LeafID()
	manager.openAppend = func(path string) (io.WriteCloser, error) {
		file, err := os.OpenFile(path, os.O_APPEND|os.O_WRONLY, 0)
		return failingAppend{file: file, limit: 12, writeErr: io.ErrUnexpectedEOF}, err
	}
	if id, err := manager.AppendCustom("partial", nil); id != "" || !errors.Is(err, io.ErrUnexpectedEOF) {
		t.Fatalf("partial write: %s %v", id, err)
	}
	reopened := reopenForTest(t, manager)
	if *reopened.LeafID() != leaf || len(reopened.Entries()) != 1 {
		t.Fatal("partial entry was restored as committed")
	}
	if bytes.HasSuffix(readForTest(t, manager.Path()), []byte{'\n'}) {
		t.Fatal("test did not leave a truncated tail")
	}
	// Open skips the partial line; it does not repair or truncate it.
	if *manager.LeafID() != leaf {
		t.Fatal("partial write advanced leaf")
	}
}

func TestManagerFilesystemAndSerializationFailures(t *testing.T) {
	dir := t.TempDir()
	blocker := write(t, dir, "blocker", "file")
	for _, path := range []string{filepath.Join(blocker, "session.jsonl"), dir} {
		manager := CreateLazy(path, "session", "/workspace", nil)
		if _, err := manager.AppendCustom("failed", nil); err == nil || manager.materialized || manager.LeafID() != nil {
			t.Fatalf("materialization failure: %v", err)
		}
	}
	manager := CreateLazy(filepath.Join(dir, "lazy.jsonl"), "session", "/workspace", nil)
	if id, err := manager.AppendCustom("invalid", json.RawMessage(`{`)); err == nil || id != "" {
		t.Fatal("invalid raw JSON accepted")
	}
	if !manager.materialized || len(reopenForTest(t, manager).Entries()) != 0 {
		t.Fatal("serialization failure must leave only the materialized header")
	}
	before := readForTest(t, manager.Path())
	invalid := []EntryPayload{nil, MessageEntry{nil}, MessageEntry{UserMessage{}}, MessageEntry{AssistantMessage{StopReason: StopReason(255)}}, MessageEntry{AssistantMessage{Content: []ContentBlock{ToolCallBlock{}}}}, MessageEntry{UserMessage{Content: UserBlocks{nil}}}}
	for _, payload := range invalid {
		if id, err := manager.Append(payload); id != "" || err == nil {
			t.Fatalf("invalid %T accepted", payload)
		}
	}
	if !bytes.Equal(before, readForTest(t, manager.Path())) {
		t.Fatal("serialization error changed entries")
	}
	open := manager.openAppend
	manager.openAppend = func(string) (io.WriteCloser, error) { return nil, os.ErrPermission }
	if id, err := manager.AppendCustom("failed", nil); id != "" || !errors.Is(err, os.ErrPermission) {
		t.Fatal("open error hidden")
	}
	manager.openAppend = open
	for _, content := range []string{"", "{}\n", strings.TrimSuffix(sessionHeader("session"), "\n"), "\n" + sessionHeader("session"), sessionHeader("session") + "\xff"} {
		write(t, dir, "lazy.jsonl", content)
		if err := manager.SetCwd("/changed"); err == nil || manager.Cwd() != "/workspace" {
			t.Fatalf("cwd error hidden for %q: %v", content, err)
		}
	}
	if err := manager.CopyContextMessagesFromPath(filepath.Join(dir, "missing")); err == nil {
		t.Fatal("copy source error hidden")
	}
	if err := Rename(filepath.Join(dir, "missing"), nil); err == nil {
		t.Fatal("rename source error hidden")
	}
	if err := Delete(dir); err == nil {
		t.Fatal("delete accepted a directory")
	}
	if err := Delete(manager.Path()); err != nil {
		t.Fatal(err)
	}
	if _, err := manager.AppendCustom("deleted", nil); !errors.Is(err, os.ErrNotExist) {
		t.Fatalf("append recreated a deleted materialized file: %v", err)
	}
	if err := manager.SetCwd("/changed"); !errors.Is(err, os.ErrNotExist) {
		t.Fatalf("cwd read error hidden: %v", err)
	}
	if err := Delete(manager.Path()); !errors.Is(err, os.ErrNotExist) {
		t.Fatalf("missing delete error hidden: %v", err)
	}
}

func TestManagerBranchCopyPartialFailureAndEmptySource(t *testing.T) {
	source := managerForTest(t)
	branch := CreateLazy(filepath.Join(t.TempDir(), "branch.jsonl"), "branch", "/workspace", nil)
	if err := branch.CopyContextMessagesFrom(source); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(branch.Path()); !errors.Is(err, os.ErrNotExist) {
		t.Fatal("empty copy created a file")
	}
	appendForTest(t, source, MessageEntry{UserMessage{Content: UserText("discarded")}})
	kept := appendForTest(t, source, MessageEntry{UserMessage{Content: UserText("kept")}})
	appendForTest(t, source, Compaction{Summary: "summary", FirstKeptEntryID: kept})
	appendForTest(t, source, MessageEntry{UserMessage{Content: UserText("after")}})
	open := branch.openAppend
	calls := 0
	branch.openAppend = func(path string) (io.WriteCloser, error) {
		calls++
		if calls == 2 {
			return nil, os.ErrPermission
		}
		return open(path)
	}
	if err := branch.CopyContextMessagesFrom(source); !errors.Is(err, os.ErrPermission) {
		t.Fatalf("copy partial error: %v", err)
	}
	sameMessages(t, reopenForTest(t, branch).ContextMessages(), source.ContextMessages()[:1])
	if len(branch.Entries()) != 1 || branch.Entries()[0].ID == kept {
		t.Fatal("copy did not generate a new root")
	}
	branch = CreateLazy(filepath.Join(t.TempDir(), "complete.jsonl"), "branch", "/workspace", nil)
	if err := branch.CopyContextMessagesFrom(source); err != nil {
		t.Fatal(err)
	}
	sameMessages(t, reopenForTest(t, branch).ContextMessages(), source.ContextMessages())
}

// Separate instances intentionally remain stale. Both appends can reach disk
// as sibling branches; this verifies the limitation, not conflict resolution.
func TestManagerSeparateInstancesAndMetadataAcrossBranches(t *testing.T) {
	manager := managerForTest(t)
	root := appendForTest(t, manager, Custom{Type: "root"})
	other := reopenForTest(t, manager)
	left := appendForTest(t, manager, Custom{Type: "left"})
	right := appendForTest(t, other, Custom{Type: "right"})
	if *manager.LeafID() != left || *other.LeafID() != right {
		t.Fatal("instances leaked state")
	}
	reopened := reopenForTest(t, manager)
	if entries := reopened.Entries(); len(entries) != 2 || entries[0].ID != root || entries[1].ID != right {
		t.Fatal("last leaf must select the right branch")
	}
	if len(reopened.reader.byID) != 3 || reopened.LatestCustom("left") == nil {
		t.Fatal("inactive branch lost from index")
	}
	// Metadata selection uses the largest timestamp across the whole index,
	// not active-branch order. Equal timestamps remain unspecified as in Rust.
	path := write(t, t.TempDir(), "metadata.jsonl", sessionHeader("s")+
		`{"type":"session_info","id":"future","timestamp":"9999","name":"future"}`+"\n"+
		`{"type":"session_info","id":"leaf","timestamp":"0000","name":"last"}`+"\n"+
		`{"type":"custom","id":"custom-new","timestamp":"9999","customType":"x","data":1}`+"\n"+
		`{"type":"custom","id":"custom-old","timestamp":"0000","customType":"x","data":2}`+"\n")
	metadata, err := OpenManager(path)
	if err != nil {
		t.Fatal(err)
	}
	if *metadata.CurrentName() != "future" || metadata.LatestCustom("x").ID != "custom-new" {
		t.Fatal("metadata selected by append order or active branch")
	}
	if _, err := metadata.AppendLabel("future", nil); err != nil {
		t.Fatal("label must accept an indexed inactive target")
	}
}

func TestManagerConcurrentAccessAndOwnedSnapshots(t *testing.T) {
	manager := managerForTest(t)
	var group sync.WaitGroup
	for i := 0; i < 20; i++ {
		group.Go(func() {
			if _, err := manager.AppendCustom("parallel", json.RawMessage(`{"ok":true}`)); err != nil {
				t.Error(err)
			}
			_ = manager.Header()
			_ = manager.Entries()
			_ = manager.ContextMessages()
			_ = manager.CurrentName()
			_ = manager.LatestCustom("parallel")
		})
	}
	group.Wait()
	if len(reopenForTest(t, manager).Entries()) != 20 {
		t.Fatal("concurrent append lost entries")
	}
	signature, display, name := "signature", "display", "title"
	arguments := json.RawMessage(`{"a":1}`)
	blocks := UserBlocks{TextBlock{"text", &signature}, ToolCallBlock{"id", "tool", arguments}}
	appendForTest(t, manager, MessageEntry{UserMessage{blocks, &display, 0}})
	appendForTest(t, manager, SessionInfo{&name})
	signature, display, name, arguments[2] = "changed", "changed", "changed", 'b'
	blocks[0] = TextBlock{Text: "changed"}
	leaf := manager.LeafID()
	*leaf = "changed"
	entries := manager.Entries()
	*entries[1].ParentID = "changed"
	custom := manager.LatestCustom("parallel")
	custom.Payload.(Custom).Data[2] = 'x'
	messages := manager.ContextMessages()
	*messages[0].(UserMessage).DisplayText = "changed"
	content := messages[0].(UserMessage).Content.(UserBlocks)
	*content[0].(TextBlock).Signature = "changed"
	content[1].(ToolCallBlock).Arguments[2] = 'x'
	if *manager.CurrentName() != "title" || *manager.LeafID() == "changed" {
		t.Fatal("external mutation changed manager state")
	}
	sameMessages(t, manager.ContextMessages(), reopenForTest(t, manager).ContextMessages())
	if got := string(manager.LatestCustom("parallel").Payload.(Custom).Data); got != `{"ok":true}` {
		t.Fatalf("snapshot shared JSON: %s", got)
	}
	if len(manager.Entries()) != 22 {
		t.Fatalf("snapshot mutation broke traversal: %d", len(manager.Entries()))
	}
}

func TestManagerRejectsInvalidUTF8WithoutSubstitution(t *testing.T) {
	for _, payload := range []EntryPayload{
		Custom{Type: "\xff"}, Custom{Type: "x", Data: json.RawMessage("\"\xff\"")},
		MessageEntry{UserMessage{Content: UserText("\xff")}},
		MessageEntry{AssistantMessage{Provider: "\xff"}},
		MessageEntry{UserMessage{Content: UserBlocks{ImageBlock{Data: "\xff"}}}},
	} {
		manager := managerForTest(t)
		before := readForTest(t, manager.Path())
		if id, err := manager.Append(payload); id != "" || err == nil {
			t.Fatalf("invalid UTF-8 in %T silently replaced: %s %v", payload, id, err)
		}
		if manager.LeafID() != nil || !bytes.Equal(before, readForTest(t, manager.Path())) {
			t.Fatal("invalid text changed persisted state")
		}
	}
	manager := managerForTest(t)
	if err := manager.SetCwd("\xff"); err == nil || manager.Cwd() != "/workspace" {
		t.Fatal("invalid cwd silently replaced")
	}
	if _, err := Create(filepath.Join(t.TempDir(), "invalid.jsonl"), "\xff", "/workspace", nil); err == nil {
		t.Fatal("invalid header silently replaced")
	}
}

func TestManagerFileWriteFailures(t *testing.T) {
	manager := managerForTest(t)
	appendForTest(t, manager, Custom{Type: "previous"})
	before, leaf := readForTest(t, manager.Path()), *manager.LeafID()
	// A real read-only descriptor makes the OS reject Write, independently of
	// simulated short writes and open errors in the other failure tests.
	manager.openAppend = func(path string) (io.WriteCloser, error) { return os.Open(path) }
	if id, err := manager.AppendCustom("failed", nil); id != "" || err == nil {
		t.Fatalf("OS write failure reported success: %s %v", id, err)
	}
	if *manager.LeafID() != leaf || !bytes.Equal(before, readForTest(t, manager.Path())) {
		t.Fatal("failed OS write changed state")
	}
	if runtime.GOOS == "windows" || os.Geteuid() == 0 {
		t.Skip("cwd mode-bit write denial requires a non-root Unix user")
	}
	if err := os.Chmod(manager.Path(), 0o444); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() {
		if err := os.Chmod(manager.Path(), 0o644); err != nil {
			t.Error(err)
		}
	})
	if err := manager.SetCwd("/denied"); err == nil {
		t.Fatal("cwd write failure reported success")
	}
	if manager.Cwd() != "/workspace" || !bytes.Equal(before, readForTest(t, manager.Path())) {
		t.Fatal("failed cwd write advanced state")
	}
}

func TestAppendSessionInfoIfUnsetPreservesManualRename(t *testing.T) {
	manager := managerForTest(t)
	appended, err := manager.AppendSessionInfoIfUnset("generated title")
	if err != nil || !appended || manager.CurrentName() == nil || *manager.CurrentName() != "generated title" {
		t.Fatalf("generated session name: appended=%v name=%v err=%v", appended, manager.CurrentName(), err)
	}
	manual := "manual title"
	if _, err := manager.AppendSessionInfo(&manual); err != nil {
		t.Fatal(err)
	}
	appended, err = manager.AppendSessionInfoIfUnset("late generated title")
	if err != nil || appended || manager.CurrentName() == nil || *manager.CurrentName() != manual {
		t.Fatalf("late generator replaced manual name: appended=%v name=%v err=%v", appended, manager.CurrentName(), err)
	}
}
