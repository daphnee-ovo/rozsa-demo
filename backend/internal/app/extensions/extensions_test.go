package extensions_test

import (
	"context"
	"errors"
	"reflect"
	"testing"

	"rozsa/backend/internal/app/extensions"
)

type testExtension struct {
	extensions.Adapter
	before  func(any) (any, error)
	context func([]extensions.Message) ([]extensions.Message, error)
	session func() error
	seen    *[]string
}

type notificationExtension struct {
	extensions.Adapter
	order *[]string
	fail  bool
}

func (extension notificationExtension) AfterProviderResponse(context.Context, extensions.AfterProviderResponse) error {
	return extension.notify("after_provider_response")
}
func (extension notificationExtension) ToolCall(context.Context, extensions.ToolCall) error {
	return extension.notify("tool_call")
}
func (extension notificationExtension) ToolResult(context.Context, extensions.ToolResult) error {
	return extension.notify("tool_result")
}
func (extension notificationExtension) notify(hook string) error {
	*extension.order = append(*extension.order, extension.Name()+"."+hook)
	if extension.fail {
		return errors.New("notification failed")
	}
	return nil
}

func (extension testExtension) SessionStart(context.Context, extensions.SessionStart) error {
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
func (extension testExtension) Context(_ context.Context, messages []extensions.Message) ([]extensions.Message, error) {
	if extension.context != nil {
		return extension.context(messages)
	}
	return messages, nil
}

func TestNotificationsContinueAfterErrorsInRegistrationOrder(t *testing.T) {
	var order []string
	runner := extensions.NewRunner()
	runner.Register(testExtension{Adapter: extensions.Adapter{ExtensionName: "first"}, seen: &order, session: func() error { return errors.New("first failed") }})
	runner.Register(testExtension{Adapter: extensions.Adapter{ExtensionName: "second"}, seen: &order})
	diagnostics, err := runner.SessionStart(context.Background(), extensions.SessionStart{SessionID: "s"})
	if err != nil || len(diagnostics) != 1 || !reflect.DeepEqual(order, []string{"first.start", "second.start"}) {
		t.Fatalf("diagnostics=%#v err=%v order=%#v", diagnostics, err, order)
	}
}

func TestTransformErrorStopsChainAndReturnsPreviousValue(t *testing.T) {
	var invoked []string
	runner := extensions.NewRunner()
	runner.Register(testExtension{Adapter: extensions.Adapter{ExtensionName: "first"}, before: func(payload any) (any, error) {
		invoked = append(invoked, "first")
		return payload.(string) + "-changed", nil
	}})
	runner.Register(testExtension{Adapter: extensions.Adapter{ExtensionName: "fails"}, before: func(any) (any, error) {
		invoked = append(invoked, "fails")
		return nil, errors.New("conversion failed")
	}})
	runner.Register(testExtension{Adapter: extensions.Adapter{ExtensionName: "last"}, before: func(payload any) (any, error) {
		invoked = append(invoked, "last")
		return payload, nil
	}})
	result, diagnostics, err := runner.BeforeProviderRequest(context.Background(), "initial")
	if err != nil || result != "initial-changed" || len(diagnostics) != 1 || !reflect.DeepEqual(invoked, []string{"first", "fails"}) {
		t.Fatalf("result=%v diagnostics=%#v err=%v invoked=%#v", result, diagnostics, err, invoked)
	}
}

func TestContextTransformFailurePreservesPreviousMessages(t *testing.T) {
	original := []extensions.Message{{Role: "user", Content: "input"}}
	runner := extensions.NewRunner()
	runner.Register(testExtension{Adapter: extensions.Adapter{ExtensionName: "transform"}, context: func(messages []extensions.Message) ([]extensions.Message, error) {
		return append(messages, extensions.Message{Role: "system", Content: "added"}), nil
	}})
	runner.Register(testExtension{Adapter: extensions.Adapter{ExtensionName: "fails"}, context: func([]extensions.Message) ([]extensions.Message, error) {
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
	runner := extensions.NewRunner()
	runner.Register(testExtension{Adapter: extensions.Adapter{ExtensionName: "cancel"}, seen: &order, session: func() error {
		cancel()
		return nil
	}})
	runner.Register(testExtension{Adapter: extensions.Adapter{ExtensionName: "later"}, seen: &order})
	diagnostics, err := runner.SessionStart(ctx, extensions.SessionStart{})
	if !errors.Is(err, context.Canceled) || len(diagnostics) != 0 || !reflect.DeepEqual(order, []string{"cancel.start"}) {
		t.Fatalf("diagnostics=%#v err=%v order=%#v", diagnostics, err, order)
	}
}

func TestProviderAndToolNotificationsContinueAfterErrors(t *testing.T) {
	var order []string
	runner := extensions.NewRunner()
	runner.Register(notificationExtension{Adapter: extensions.Adapter{ExtensionName: "first"}, order: &order, fail: true})
	runner.Register(notificationExtension{Adapter: extensions.Adapter{ExtensionName: "second"}, order: &order})
	checks := []struct {
		name string
		run  func() ([]extensions.Diagnostic, error)
	}{
		{"after_provider_response", func() ([]extensions.Diagnostic, error) {
			return runner.AfterProviderResponse(context.Background(), extensions.AfterProviderResponse{})
		}},
		{"tool_call", func() ([]extensions.Diagnostic, error) {
			return runner.ToolCall(context.Background(), extensions.ToolCall{})
		}},
		{"tool_result", func() ([]extensions.Diagnostic, error) {
			return runner.ToolResult(context.Background(), extensions.ToolResult{})
		}},
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
	runner := extensions.NewRunner()
	runner.Register(testExtension{Adapter: extensions.Adapter{ExtensionName: "transform"}, context: func(messages []extensions.Message) ([]extensions.Message, error) {
		return append(messages, extensions.Message{Role: "system", Content: "completed"}), nil
	}})
	runner.Register(testExtension{Adapter: extensions.Adapter{ExtensionName: "cancel"}, context: func(messages []extensions.Message) ([]extensions.Message, error) {
		cancel()
		return append(messages, extensions.Message{Role: "system", Content: "partial"}), nil
	}})
	result, _, err := runner.Context(ctx, []extensions.Message{{Role: "user", Content: "input"}})
	if !errors.Is(err, context.Canceled) || len(result) != 2 || result[1].Content != "completed" {
		t.Fatalf("result=%#v err=%v", result, err)
	}
}
