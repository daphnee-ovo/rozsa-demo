package runtime

import (
	"sync"

	"rozsa/backend/internal/app/session"
)

// Rust keeps delivered steering in the live conversation, but persists only
// non-user loop messages. Reopening reconstructs context from JSONL instead.
type conversation struct {
	mu      sync.Mutex
	context []session.Message
	display []session.Message
}

func (runtime *Runtime) conversation(manager *session.Manager) *conversation {
	runtime.mu.Lock()
	defer runtime.mu.Unlock()
	return runtime.conversationLocked(manager)
}

func (runtime *Runtime) conversationLocked(manager *session.Manager) *conversation {
	if runtime.conversations == nil {
		runtime.conversations = make(map[string]*conversation)
	}
	if existing := runtime.conversations[manager.Path()]; existing != nil {
		return existing
	}
	conversation := newConversation(manager)
	runtime.conversations[manager.Path()] = conversation
	return conversation
}

func newConversation(manager *session.Manager) *conversation {
	conversation := &conversation{context: manager.ContextMessages()}
	entries := manager.Entries()
	for index := len(entries) - 1; index >= 0; index-- {
		if compact, ok := entries[index].Payload.(session.Compaction); ok && compact.Summary != "" {
			display := "Previous conversation summary"
			conversation.context = append([]session.Message{session.UserMessage{Content: session.UserText(compact.Summary), DisplayText: &display}}, conversation.context...)
			break
		}
	}
	for _, entry := range entries {
		if message, ok := entry.Payload.(session.MessageEntry); ok {
			conversation.display = append(conversation.display, message.Message)
		}
	}
	return conversation
}

func (conversation *conversation) contextMessages() []session.Message {
	conversation.mu.Lock()
	defer conversation.mu.Unlock()
	return append([]session.Message(nil), conversation.context...)
}

func (conversation *conversation) displayMessages() []session.Message {
	conversation.mu.Lock()
	defer conversation.mu.Unlock()
	return append([]session.Message(nil), conversation.display...)
}

func (conversation *conversation) append(message session.Message) {
	conversation.mu.Lock()
	defer conversation.mu.Unlock()
	conversation.context = append(conversation.context, message)
	conversation.display = append(conversation.display, message)
}

func (conversation *conversation) appendContext(message session.Message) {
	conversation.mu.Lock()
	defer conversation.mu.Unlock()
	conversation.context = append(conversation.context, message)
}

func (conversation *conversation) appendDisplay(message session.Message) {
	conversation.mu.Lock()
	defer conversation.mu.Unlock()
	conversation.display = append(conversation.display, message)
}

func (conversation *conversation) replaceContext(messages []session.Message) {
	conversation.mu.Lock()
	defer conversation.mu.Unlock()
	conversation.context = append([]session.Message(nil), messages...)
}
