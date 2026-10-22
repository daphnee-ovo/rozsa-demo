// User-approved private-injection exception for append write/short/partial/close failures and a mid-copy append failure.
// The public API accepts a path and payload then opens the OS file internally, so these failures cannot be produced reliably across platforms.
// No other session tests may inject private dependencies; each approved test and injection point states its exact behavior and failure.
package session

import (
	"bytes"
	"errors"
	"io"
	"os"
	"path/filepath"
	"reflect"
	"testing"
)

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

func managerForFaultTest(t *testing.T) *Manager {
	t.Helper()
	manager, err := Create(filepath.Join(t.TempDir(), "session.jsonl"), "session", "/workspace", nil)
	if err != nil {
		t.Fatal(err)
	}
	return manager
}

func appendFaultTestEntry(t *testing.T, manager *Manager, payload EntryPayload) string {
	t.Helper()
	entryID, err := manager.Append(payload)
	if err != nil {
		t.Fatal(err)
	}
	return entryID
}

// Approved private-injection exception: this is required to prove failed append errors cannot report success or advance public Entries/LeafID.
func TestManagerAppendFailuresDoNotAdvancePublicState(t *testing.T) {
	for _, test := range []struct {
		name                         string
		limit                        int
		writeErr, closeErr, expected error
	}{
		{"write error", 0, io.ErrClosedPipe, nil, io.ErrClosedPipe},
		{"short write", 4, nil, nil, io.ErrShortWrite},
		{"close error", 1 << 20, nil, io.ErrClosedPipe, io.ErrClosedPipe},
	} {
		t.Run(test.name, func(t *testing.T) {
			manager := managerForFaultTest(t)
			appendFaultTestEntry(t, manager, Custom{Type: "previous"})
			entries, leaf := manager.Entries(), manager.LeafID()
			// Inject the table's approved write, short-write, or close failure.
			manager.openAppend = func(string) (io.WriteCloser, error) {
				return failingAppend{limit: test.limit, writeErr: test.writeErr, closeErr: test.closeErr}, nil
			}
			if entryID, err := manager.AppendCustom("failed", nil); entryID != "" || !errors.Is(err, test.expected) {
				t.Fatalf("failure reported success: %q %v", entryID, err)
			}
			if !reflect.DeepEqual(manager.Entries(), entries) || manager.LeafID() == nil || *manager.LeafID() != *leaf {
				t.Fatal("failed append advanced public session state")
			}
		})
	}
}

// Approved private-injection exception: the public filesystem API cannot reliably create a partial tail, which must not restore as a committed entry.
func TestManagerPartialAppendDoesNotRestoreTruncatedRecord(t *testing.T) {
	manager := managerForFaultTest(t)
	appendFaultTestEntry(t, manager, Custom{Type: "previous"})
	previousLeaf := *manager.LeafID()
	// Inject a 12-byte partial write to create the approved truncated-tail case.
	manager.openAppend = func(path string) (io.WriteCloser, error) {
		file, err := os.OpenFile(path, os.O_APPEND|os.O_WRONLY, 0)
		return failingAppend{file: file, limit: 12, writeErr: io.ErrUnexpectedEOF}, err
	}
	if entryID, err := manager.AppendCustom("partial", nil); entryID != "" || !errors.Is(err, io.ErrUnexpectedEOF) {
		t.Fatalf("partial write: %s %v", entryID, err)
	}
	reopened, err := OpenManager(manager.Path())
	if err != nil {
		t.Fatal(err)
	}
	if reopened.LeafID() == nil || *reopened.LeafID() != previousLeaf || len(reopened.Entries()) != 1 {
		t.Fatal("truncated record was restored as committed")
	}
	contents, err := os.ReadFile(manager.Path())
	if err != nil {
		t.Fatal(err)
	}
	if bytes.HasSuffix(contents, []byte{'\n'}) {
		t.Fatal("test did not leave a truncated JSONL tail")
	}
	if manager.LeafID() == nil || *manager.LeafID() != previousLeaf {
		t.Fatal("partial write advanced public leaf")
	}
}

// Approved private-injection exception: a controlled later append failure is needed to prove copy errors propagate and the committed prefix remains.
func TestManagerCopyFailureKeepsCommittedPrefix(t *testing.T) {
	source := managerForFaultTest(t)
	appendFaultTestEntry(t, source, MessageEntry{UserMessage{Content: UserText("first")}})
	appendFaultTestEntry(t, source, MessageEntry{UserMessage{Content: UserText("second")}})
	branch := CreateLazy(filepath.Join(t.TempDir(), "branch.jsonl"), "branch", "/workspace", nil)
	openAppend := branch.openAppend
	appendOpens := 0
	// Fail the second copy append open; this is the specifically approved mid-copy failure.
	branch.openAppend = func(path string) (io.WriteCloser, error) {
		appendOpens++
		if appendOpens == 2 {
			return nil, os.ErrPermission
		}
		return openAppend(path)
	}
	if err := branch.CopyContextMessagesFrom(source); !errors.Is(err, os.ErrPermission) {
		t.Fatalf("copy error was hidden: %v", err)
	}
	assertCopiedPrefix := func(t *testing.T, manager *Manager, expected Message) {
		t.Helper()
		messages := manager.ContextMessages()
		if len(manager.Entries()) != 1 || len(messages) != 1 || !reflect.DeepEqual(messages[0], expected) {
			t.Fatalf("copied prefix: entries=%+v messages=%#v", manager.Entries(), messages)
		}
	}
	assertCopiedPrefix(t, branch, UserMessage{Content: UserText("first")})
	reopened, err := OpenManager(branch.Path())
	if err != nil {
		t.Fatal(err)
	}
	assertCopiedPrefix(t, reopened, UserMessage{Content: UserBlocks{TextBlock{Text: "first"}}})
}
