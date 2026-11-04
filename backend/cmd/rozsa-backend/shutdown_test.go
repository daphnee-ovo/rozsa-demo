package main_test

import (
	"testing"

	"rozsa/backend/internal/app/session"
)

func assertAbortedPartialSession(t *testing.T, path, id string) {
	t.Helper()
	reader, err := session.Open(path)
	if err != nil {
		t.Fatal(err)
	}
	messages := reader.ContextMessages()
	if len(messages) != 2 {
		t.Fatalf("session %s persisted %d messages after shutdown acknowledgement", id, len(messages))
	}
	user, ok := messages[0].(session.UserMessage)
	if !ok {
		t.Fatalf("session %s first message = %#v, want user question", id, messages[0])
	}
	blocks, ok := user.Content.(session.UserBlocks)
	if !ok || len(blocks) != 1 {
		t.Fatalf("session %s user content = %#v, want one text block", id, user.Content)
	}
	userText, ok := blocks[0].(session.TextBlock)
	if !ok || userText.Text != "question "+id {
		t.Fatalf("session %s user text = %#v, want %q", id, user.Content, "question "+id)
	}
	assistant, ok := messages[1].(session.AssistantMessage)
	if !ok || assistant.StopReason != session.Aborted {
		t.Fatalf("session %s assistant = %#v, want an aborted final write", id, messages[1])
	}
	partial := false
	for _, block := range assistant.Content {
		if text, ok := block.(session.TextBlock); ok && text.Text == "partial" {
			partial = true
		}
	}
	if !partial {
		t.Fatalf("session %s assistant content = %#v, want partial text", id, assistant.Content)
	}
}
