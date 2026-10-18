package runtime

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"

	"rozsa/backend/internal/app/questions"
	"rozsa/backend/internal/core/tools"
	"rozsa/backend/internal/model/types"
)

const askUserQuestionName = "askUserQuestion"

type askUserQuestionTool struct {
	sessionID string
	owner     *questions.Owner
}

func (runtime *Runtime) applicationTools(sessionID string) []tools.Tool {
	return []tools.Tool{askUserQuestionTool{sessionID: sessionID, owner: runtime.questions}, runtime.subagentTool(sessionID)}
}

func (tool askUserQuestionTool) Name() string  { return askUserQuestionName }
func (tool askUserQuestionTool) Label() string { return "Ask User Question" }
func (tool askUserQuestionTool) Description() string {
	return "Ask the user one or more structured questions with single-select or multi-select options. A custom Other answer is always available."
}
func (tool askUserQuestionTool) ParametersSchema() json.RawMessage { return questionSchema }
func (tool askUserQuestionTool) PrepareArguments(arguments json.RawMessage) json.RawMessage {
	return arguments
}
func (tool askUserQuestionTool) ExecutionMode() *tools.ExecutionMode {
	mode := tools.Sequential
	return &mode
}

func (tool askUserQuestionTool) Execute(ctx context.Context, toolCallID string, arguments json.RawMessage, _ func(tools.Result)) (tools.Result, error) {
	var params questions.Params
	if err := json.Unmarshal(arguments, &params); err != nil {
		return tools.Result{}, fmt.Errorf("invalid askUserQuestion parameters: %w", err)
	}
	_, future, err := tool.owner.Submit(tool.sessionID, toolCallID, params)
	if err != nil {
		return tools.Result{}, err
	}
	answer, err := future.Wait(ctx)
	if err != nil {
		if errors.Is(err, questions.ErrCancelled) {
			return tools.Result{}, errors.New("user cancelled askUserQuestion")
		}
		return tools.Result{}, err
	}
	answerValues := make(map[string]any, len(answer.Answers))
	for header, value := range answer.Answers {
		if value.Single != nil {
			answerValues[header] = *value.Single
		} else {
			answerValues[header] = value.Multiple
		}
	}
	details, err := json.Marshal(map[string]any{"answers": answerValues})
	if err != nil {
		return tools.Result{}, fmt.Errorf("encode askUserQuestion result: %w", err)
	}
	return tools.Result{Content: []types.ContentBlock{types.TextBlock{Text: string(details)}}, Details: details}, nil
}

var questionSchema = json.RawMessage(`{
  "type":"object",
  "additionalProperties":false,
  "properties":{"questions":{"type":"array","minItems":1,"maxItems":4,"items":{
    "type":"object","additionalProperties":false,
    "properties":{
      "question":{"type":"string"},
      "header":{"type":"string"},
      "options":{"type":"array","minItems":1,"maxItems":8,"items":{"type":"object","additionalProperties":false,"properties":{"label":{"type":"string"},"description":{"type":"string"}},"required":["label","description"]}},
      "multiSelect":{"type":"boolean","default":false}
    },
    "required":["question","header","options"]
  }}},
  "required":["questions"]
}`)
