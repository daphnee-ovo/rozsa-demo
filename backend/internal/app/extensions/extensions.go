// Package extensions defines the extension hook contract and ordered runner.
package extensions

import (
	"context"
	"fmt"
	"net/http"
)

type SessionStart struct{ SessionID, CWD string }
type AfterProviderResponse struct {
	Status  int
	Headers http.Header
}
type ToolCall struct {
	Name, ID  string
	Arguments any
}
type ToolResult struct {
	Name, ID string
	Result   any
	IsError  bool
}
type Message struct {
	Role    string
	Content any
}

// Extension is the public contract. Implementations may handle only the hooks
// they need; the default adapter supplies no-op behavior for the rest.
type Extension interface {
	Name() string
	SessionStart(context.Context, SessionStart) error
	BeforeProviderRequest(context.Context, any) (any, error)
	AfterProviderResponse(context.Context, AfterProviderResponse) error
	ToolCall(context.Context, ToolCall) error
	ToolResult(context.Context, ToolResult) error
	Context(context.Context, []Message) ([]Message, error)
}

type Adapter struct{ ExtensionName string }

func (adapter Adapter) Name() string                             { return adapter.ExtensionName }
func (Adapter) SessionStart(context.Context, SessionStart) error { return nil }
func (Adapter) BeforeProviderRequest(_ context.Context, payload any) (any, error) {
	return payload, nil
}
func (Adapter) AfterProviderResponse(context.Context, AfterProviderResponse) error { return nil }
func (Adapter) ToolCall(context.Context, ToolCall) error                           { return nil }
func (Adapter) ToolResult(context.Context, ToolResult) error                       { return nil }
func (Adapter) Context(_ context.Context, messages []Message) ([]Message, error) {
	return messages, nil
}

type Diagnostic struct {
	Extension string
	Hook      string
	Err       error
}

type Runner interface {
	Register(Extension)
	HasExtensions() bool
	SessionStart(context.Context, SessionStart) ([]Diagnostic, error)
	BeforeProviderRequest(context.Context, any) (any, []Diagnostic, error)
	AfterProviderResponse(context.Context, AfterProviderResponse) ([]Diagnostic, error)
	ToolCall(context.Context, ToolCall) ([]Diagnostic, error)
	ToolResult(context.Context, ToolResult) ([]Diagnostic, error)
	Context(context.Context, []Message) ([]Message, []Diagnostic, error)
}

type orderedRunner struct{ extensions []Extension }

func NewRunner() Runner { return &orderedRunner{} }

func (runner *orderedRunner) Register(extension Extension) {
	runner.extensions = append(runner.extensions, extension)
}

func (runner *orderedRunner) HasExtensions() bool { return len(runner.extensions) != 0 }

func (runner *orderedRunner) SessionStart(ctx context.Context, event SessionStart) ([]Diagnostic, error) {
	return notify(ctx, runner.extensions, "session_start", func(extension Extension) error {
		return extension.SessionStart(ctx, event)
	})
}

func (runner *orderedRunner) AfterProviderResponse(ctx context.Context, event AfterProviderResponse) ([]Diagnostic, error) {
	return notify(ctx, runner.extensions, "after_provider_response", func(extension Extension) error {
		return extension.AfterProviderResponse(ctx, event)
	})
}

func (runner *orderedRunner) ToolCall(ctx context.Context, event ToolCall) ([]Diagnostic, error) {
	return notify(ctx, runner.extensions, "tool_call", func(extension Extension) error {
		return extension.ToolCall(ctx, event)
	})
}

func (runner *orderedRunner) ToolResult(ctx context.Context, event ToolResult) ([]Diagnostic, error) {
	return notify(ctx, runner.extensions, "tool_result", func(extension Extension) error {
		return extension.ToolResult(ctx, event)
	})
}

func (runner *orderedRunner) BeforeProviderRequest(ctx context.Context, payload any) (any, []Diagnostic, error) {
	var diagnostics []Diagnostic
	for _, extension := range runner.extensions {
		if err := ctx.Err(); err != nil {
			return payload, diagnostics, err
		}
		transformed, err := extension.BeforeProviderRequest(ctx, payload)
		if contextErr := ctx.Err(); contextErr != nil {
			return payload, diagnostics, contextErr
		}
		if err != nil {
			diagnostics = append(diagnostics, diagnostic(extension, "before_provider_request", err))
			break
		}
		payload = transformed
	}
	if err := ctx.Err(); err != nil {
		return payload, diagnostics, err
	}
	return payload, diagnostics, nil
}

func (runner *orderedRunner) Context(ctx context.Context, messages []Message) ([]Message, []Diagnostic, error) {
	var diagnostics []Diagnostic
	for _, extension := range runner.extensions {
		if err := ctx.Err(); err != nil {
			return messages, diagnostics, err
		}
		transformed, err := extension.Context(ctx, cloneMessages(messages))
		if contextErr := ctx.Err(); contextErr != nil {
			return messages, diagnostics, contextErr
		}
		if err != nil {
			diagnostics = append(diagnostics, diagnostic(extension, "context", err))
			break
		}
		messages = transformed
	}
	if err := ctx.Err(); err != nil {
		return messages, diagnostics, err
	}
	return messages, diagnostics, nil
}

func notify(ctx context.Context, registered []Extension, hook string, invoke func(Extension) error) ([]Diagnostic, error) {
	var diagnostics []Diagnostic
	for _, extension := range registered {
		if err := ctx.Err(); err != nil {
			return diagnostics, err
		}
		if err := invoke(extension); err != nil {
			diagnostics = append(diagnostics, diagnostic(extension, hook, err))
		}
	}
	if err := ctx.Err(); err != nil {
		return diagnostics, err
	}
	return diagnostics, nil
}

func diagnostic(extension Extension, hook string, err error) Diagnostic {
	return Diagnostic{Extension: extension.Name(), Hook: hook, Err: err}
}

func (diagnostic Diagnostic) Error() string {
	return fmt.Sprintf("extension %q hook %s: %v", diagnostic.Extension, diagnostic.Hook, diagnostic.Err)
}

func cloneMessages(messages []Message) []Message { return append([]Message(nil), messages...) }
