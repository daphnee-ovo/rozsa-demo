package tools

import (
	"context"
	"encoding/json"
	"fmt"
	"strconv"

	"rozsa/backend/internal/model/types"
)

type ExecutionMode uint8

const (
	Parallel ExecutionMode = iota
	Sequential
)

type Result struct {
	Content   []types.ContentBlock
	Details   json.RawMessage
	Terminate bool
}

type Tool interface {
	Name() string
	Description() string
	Label() string
	ParametersSchema() json.RawMessage
	Execute(context.Context, string, json.RawMessage, func(Result)) (Result, error)
	PrepareArguments(json.RawMessage) json.RawMessage
	ExecutionMode() *ExecutionMode
}

func CoerceArguments(schema, arguments json.RawMessage) (json.RawMessage, error) {
	var schemaValue any
	if err := json.Unmarshal(schema, &schemaValue); err != nil {
		return nil, fmt.Errorf("decode tool schema: %w", err)
	}
	var argumentValue any
	if err := json.Unmarshal(arguments, &argumentValue); err != nil {
		return nil, fmt.Errorf("decode tool arguments: %w", err)
	}
	coerceValue(schemaValue, &argumentValue)
	encoded, err := json.Marshal(argumentValue)
	if err != nil {
		return nil, fmt.Errorf("encode coerced tool arguments: %w", err)
	}
	return encoded, nil
}

func coerceValue(schema any, value *any) {
	definition, ok := schema.(map[string]any)
	if !ok {
		return
	}
	expected, _ := definition["type"].(string)
	if expected == "" {
		variants, _ := definition["anyOf"].([]any)
		if variants == nil {
			variants, _ = definition["oneOf"].([]any)
		}
		for _, variant := range variants {
			candidate, err := json.Marshal(*value)
			if err != nil {
				return
			}
			var copyValue any
			if json.Unmarshal(candidate, &copyValue) != nil {
				return
			}
			coerceValue(variant, &copyValue)
			before, _ := json.Marshal(*value)
			after, _ := json.Marshal(copyValue)
			if string(before) != string(after) {
				*value = copyValue
				return
			}
		}
		return
	}
	switch expected {
	case "object":
		object, ok := (*value).(map[string]any)
		if !ok {
			return
		}
		properties, _ := definition["properties"].(map[string]any)
		for key, childSchema := range properties {
			if child, exists := object[key]; exists {
				coerceValue(childSchema, &child)
				object[key] = child
			}
		}
	case "array":
		items, exists := definition["items"]
		if !exists {
			return
		}
		if array, ok := (*value).([]any); ok {
			for index := range array {
				child := array[index]
				coerceValue(items, &child)
				array[index] = child
			}
			return
		}
		child := *value
		coerceValue(items, &child)
		*value = []any{child}
	case "string":
		switch current := (*value).(type) {
		case nil:
			*value = ""
		case bool:
			*value = fmt.Sprint(current)
		case float64:
			*value = strconv.FormatFloat(current, 'f', -1, 64)
		}
	case "boolean":
		switch current := (*value).(type) {
		case nil:
			*value = false
		case string:
			if current == "true" {
				*value = true
			} else if current == "false" {
				*value = false
			}
		case float64:
			if current == 1 {
				*value = true
			} else if current == 0 {
				*value = false
			}
		}
	case "number", "integer":
		switch current := (*value).(type) {
		case nil:
			*value = float64(0)
		case bool:
			if current {
				*value = float64(1)
			} else {
				*value = float64(0)
			}
		case string:
			if parsed, err := strconv.ParseFloat(current, 64); err == nil && (expected != "integer" || parsed == float64(int64(parsed))) {
				*value = parsed
			}
		}
	}
}

func NormalizeArguments(raw json.RawMessage, tool Tool) (json.RawMessage, error) {
	if len(raw) == 0 || string(raw) == "null" {
		return json.RawMessage("{}"), nil
	}
	prepared := tool.PrepareArguments(append(json.RawMessage(nil), raw...))
	return CoerceArguments(tool.ParametersSchema(), prepared)
}

func Metadata(tools []Tool) []types.ToolSchema {
	metadata := make([]types.ToolSchema, 0, len(tools))
	for _, tool := range tools {
		metadata = append(metadata, types.ToolSchema{Name: tool.Name(), Description: tool.Description(), Parameters: append(json.RawMessage(nil), tool.ParametersSchema()...)})
	}
	return metadata
}
