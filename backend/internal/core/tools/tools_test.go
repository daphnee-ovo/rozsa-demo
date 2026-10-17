package tools

import (
	"context"
	"encoding/json"
	"testing"
)

type argumentTool struct{}

func (argumentTool) Name() string        { return "arg" }
func (argumentTool) Description() string { return "" }
func (argumentTool) Label() string       { return "" }
func (argumentTool) ParametersSchema() json.RawMessage {
	return json.RawMessage(`{"type":"object","properties":{"count":{"type":"integer"},"enabled":{"type":"boolean"},"labels":{"type":"array","items":{"type":"string"}}}}`)
}
func (argumentTool) Execute(_ context.Context, _ string, _ json.RawMessage, _ func(Result)) (Result, error) {
	return Result{}, nil
}
func (argumentTool) PrepareArguments(value json.RawMessage) json.RawMessage { return value }
func (argumentTool) ExecutionMode() *ExecutionMode                          { return nil }

func TestNormalizeArgumentsNullAndSchemaCoercion(t *testing.T) {
	got, err := NormalizeArguments(json.RawMessage("null"), argumentTool{})
	if err != nil || string(got) != "{}" {
		t.Fatalf("null args = %s, %v; want {}", got, err)
	}
	got, err = NormalizeArguments(json.RawMessage(`{"count":"4","enabled":1,"labels":6}`), argumentTool{})
	if err != nil {
		t.Fatal(err)
	}
	var value map[string]any
	if err := json.Unmarshal(got, &value); err != nil {
		t.Fatal(err)
	}
	if value["count"] != float64(4) || value["enabled"] != true {
		t.Fatalf("coerced values = %#v", value)
	}
	labels, ok := value["labels"].([]any)
	if !ok || len(labels) != 1 || labels[0] != "6" {
		t.Fatalf("array coercion = %#v", value["labels"])
	}
}

var _ Tool = argumentTool{}
