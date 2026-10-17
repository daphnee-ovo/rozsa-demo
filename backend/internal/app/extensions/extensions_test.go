package extensions

import (
	"context"
	"errors"
	"reflect"
	"testing"
)

type testExtension struct {
	Adapter
	before  func(any) (any, error)
	context func([]Message) ([]Message, error)
	session func() error
	seen    *[]string
}

type notificationExtension struct {
	Adapter
	order *[]string
	fail  bool
}

func (extension notificationExtension) AfterProviderResponse(context.Context, AfterProviderResponse) error {
	return extension.notify("after_provider_response")
}
func (extension notificationExtension) ToolCall(context.Context, ToolCall) error {
	return extension.notify("tool_call")
}
func (extension notificationExtension) ToolResult(context.Context, ToolResult) error {
	return extension.notify("tool_result")
}
func (extension notificationExtension) notify(hook string) error {
	*extension.order = append(*extension.order, extension.Name()+"."+hook)
	if extension.fail {
		return errors.New("notification failed")
	}
	return nil
}

func (extension testExtension) SessionStart(context.Context, SessionStart) error {
	if extension.seen != nil {
		*extension.seen = append(*extension.seen, extension.Name()+".start")
	}
	if extension.session != nil {
		return extension.session()
	}
	return nil
}
func (extension testExtension) BeforeProviderRequest(_ context.Context, payload any) (any, error) {
	if extension.before != nil {
		return extension.before(payload)
	}
	return payload, nil
}
func (extension testExtension) Context(_ context.Context, messages []Message) ([]Message, error) {
	if extension.context != nil {
		return extension.context(messages)
	}
	return messages, nil
}

func TestNotificationsContinueAfterErrorsInRegistrationOrder(t *testing.T) {
	var order []string
	runner := NewRunner()
	runner.Register(testExtension{Adapter: Adapter{ExtensionName: "first"}, seen: &order, session: func() error { return errors.New("first failed") }})
	runner.Register(testExtension{Adapter: Adapter{ExtensionName: "second"}, seen: &order})
	diagnostics, err := runner.SessionStart(context.Background(), SessionStart{SessionID: "s"})
	if err != nil || len(diagnostics) != 1 || !reflect.DeepEqual(order, []string{"first.start", "second.start"}) {
		t.Fatalf("diagnostics=%#v err=%v order=%#v", diagnostics, err, order)
	}
}

func TestTransformErrorStopsChainAndReturnsPreviousValue(t *testing.T) {
	var invoked []string
	runner := NewRunner()
	runner.Register(testExtension{Adapter: Adapter{ExtensionName: "first"}, before: func(payload any) (any, error) {
		invoked = append(invoked, "first")
		return payload.(string) + "-changed", nil
	}})
	runner.Register(testExtension{Adapter: Adapter{ExtensionName: "fails"}, before: func(any) (any, error) {
		invoked = append(invoked, "fails")
		return nil, errors.New("conversion failed")
	}})
	runner.Register(testExtension{Adapter: Adapter{ExtensionName: "last"}, before: func(payload any) (any, error) {
		invoked = append(invoked, "last")
		return payload, nil
	}})
	result, diagnostics, err := runner.BeforeProviderRequest(context.Background(), "initial")
	if err != nil || result != "initial-changed" || len(diagnostics) != 1 || !reflect.DeepEqual(invoked, []string{"first", "fails"}) {
		t.Fatalf("result=%v diagnostics=%#v err=%v invoked=%#v", result, diagnostics, err, invoked)
	}
}

func TestContextTransformFailurePreservesPreviousMessages(t *testing.T) {
	original := []Message{{Role: "user", Content: "input"}}
	runner := NewRunner()
	runner.Register(testExtension{Adapter: Adapter{ExtensionName: "transform"}, context: func(messages []Message) ([]Message, error) {
		return append(messages, Message{Role: "system", Content: "added"}), nil
	}})
	runner.Register(testExtension{Adapter: Adapter{ExtensionName: "fails"}, context: func([]Message) ([]Message, error) {
		return nil, errors.New("context failed")
	}})
	result, diagnostics, err := runner.Context(context.Background(), original)
	if err != nil || len(result) != 2 || len(diagnostics) != 1 || len(original) != 1 {
		t.Fatalf("result=%#v diagnostics=%#v err=%v original=%#v", result, diagnostics, err, original)
	}
}

func TestCancellationStopsFurtherHooks(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	var order []string
	runner := NewRunner()
	runner.Register(testExtension{Adapter: Adapter{ExtensionName: "cancel"}, seen: &order, session: func() error {
		cancel()
		return nil
	}})
	runner.Register(testExtension{Adapter: Adapter{ExtensionName: "later"}, seen: &order})
	diagnostics, err := runner.SessionStart(ctx, SessionStart{})
	if !errors.Is(err, context.Canceled) || len(diagnostics) != 0 || !reflect.DeepEqual(order, []string{"cancel.start"}) {
		t.Fatalf("diagnostics=%#v err=%v order=%#v", diagnostics, err, order)
	}
}

func TestProviderAndToolNotificationsContinueAfterErrors(t *testing.T) {
	var order []string
	runner := NewRunner()
	runner.Register(notificationExtension{Adapter: Adapter{ExtensionName: "first"}, order: &order, fail: true})
	runner.Register(notificationExtension{Adapter: Adapter{ExtensionName: "second"}, order: &order})
	checks := []struct {
		name string
		run  func() ([]Diagnostic, error)
	}{
		{"after_provider_response", func() ([]Diagnostic, error) {
			return runner.AfterProviderResponse(context.Background(), AfterProviderResponse{})
		}},
		{"tool_call", func() ([]Diagnostic, error) { return runner.ToolCall(context.Background(), ToolCall{}) }},
		{"tool_result", func() ([]Diagnostic, error) { return runner.ToolResult(context.Background(), ToolResult{}) }},
	}
	for _, check := range checks {
		diagnostics, err := check.run()
		if err != nil || len(diagnostics) != 1 || diagnostics[0].Hook != check.name {
			t.Fatalf("%s: diagnostics=%#v err=%v", check.name, diagnostics, err)
		}
	}
	if len(order) != 6 {
		t.Fatalf("notification chain did not continue: %#v", order)
	}
}

func TestCanceledContextTransformReturnsLastCompletedValue(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	runner := NewRunner()
	runner.Register(testExtension{Adapter: Adapter{ExtensionName: "transform"}, context: func(messages []Message) ([]Message, error) {
		return append(messages, Message{Role: "system", Content: "completed"}), nil
	}})
	runner.Register(testExtension{Adapter: Adapter{ExtensionName: "cancel"}, context: func(messages []Message) ([]Message, error) {
		cancel()
		return append(messages, Message{Role: "system", Content: "partial"}), nil
	}})
	result, _, err := runner.Context(ctx, []Message{{Role: "user", Content: "input"}})
	if !errors.Is(err, context.Canceled) || len(result) != 2 || result[1].Content != "completed" {
		t.Fatalf("result=%#v err=%v", result, err)
	}
}
