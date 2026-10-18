package subagent

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"strings"

	app "rozsa/backend/internal/app/subagent"
	"rozsa/backend/internal/core/tools"
	"rozsa/backend/internal/model/types"
)

type tool struct{ manager app.Manager }

func New(manager app.Manager) tools.Tool { return tool{manager} }
func (tool) Name() string                { return "subagent" }
func (tool) Label() string               { return "Subagent" }
func (tool) Description() string {
	return "Create, message, wait for, list, or interrupt independent subagents. Use spawn with a focused system_prompt and optional prompt to delegate work. Subagents run with their own transcript and can be inspected or interrupted by the user."
}
func (tool) ParametersSchema() json.RawMessage                      { return schema }
func (tool) PrepareArguments(value json.RawMessage) json.RawMessage { return value }
func (tool) ExecutionMode() *tools.ExecutionMode                    { mode := tools.Sequential; return &mode }

func (tool tool) Execute(ctx context.Context, callID string, arguments json.RawMessage, _ func(tools.Result)) (tools.Result, error) {
	if err := ctx.Err(); err != nil {
		return tools.Result{}, err
	}
	var params struct {
		Action       string          `json:"action"`
		ID           string          `json:"id"`
		Name         string          `json:"name"`
		SystemPrompt string          `json:"system_prompt"`
		Prompt       string          `json:"prompt"`
		Wait         bool            `json:"wait"`
		Scope        json.RawMessage `json:"scope"`
	}
	if err := json.Unmarshal(arguments, &params); err != nil {
		return tools.Result{}, fmt.Errorf("invalid subagent arguments: %w", err)
	}
	params.ID, params.Prompt, params.SystemPrompt = strings.TrimSpace(params.ID), strings.TrimSpace(params.Prompt), strings.TrimSpace(params.SystemPrompt)
	switch params.Action {
	case "spawn":
		if params.SystemPrompt == "" {
			return tools.Result{}, errors.New("subagent spawn requires system_prompt")
		}
		scope, err := parseScope(params.Scope)
		if err != nil {
			return tools.Result{}, err
		}
		info, err := tool.manager.Spawn(ctx, app.SpawnConfig{Name: params.Name, SystemPrompt: params.SystemPrompt, Scope: scope}, callID)
		if err != nil {
			return tools.Result{}, err
		}
		params.ID = info.ID
		if params.Prompt != "" {
			if err := tool.manager.Send(ctx, info.ID, params.Prompt, params.Wait, callID); err != nil {
				return tools.Result{}, err
			}
		}
	case "send", "wait", "interrupt":
		if params.ID == "" {
			return tools.Result{}, fmt.Errorf("subagent %s requires id", params.Action)
		}
		var err error
		switch params.Action {
		case "send":
			if params.Prompt == "" {
				return tools.Result{}, errors.New("subagent send requires prompt")
			}
			err = tool.manager.Send(ctx, params.ID, params.Prompt, params.Wait, callID)
		case "wait":
			err = tool.manager.Wait(ctx, params.ID)
		case "interrupt":
			err = tool.manager.Abort(ctx, params.ID)
		}
		if err != nil {
			return tools.Result{}, err
		}
	case "list":
		list := tool.manager.List()
		lines := []string{}
		for _, info := range list {
			lines = append(lines, fmt.Sprintf("%s (%s) — status: %s, model: %s/%s", info.ID, info.Name, info.Status, info.ModelProvider, info.ModelID))
		}
		text := "No subagents."
		if len(lines) > 0 {
			text = strings.Join(lines, "\n")
		}
		return result(text, map[string]any{"action": "list", "subagents": list})
	default:
		return tools.Result{}, fmt.Errorf("Unknown subagent action: '%s'. Valid actions: spawn, send, wait, interrupt, list", params.Action)
	}
	snapshot, err := tool.manager.Snapshot(params.ID)
	if err != nil {
		return tools.Result{}, err
	}
	info := snapshot.Info
	text := fmt.Sprintf("%s (%s)\nstatus: %s\nmodel: %s/%s\nthinking_effort: %s\nmessages: %d", info.ID, info.Name, info.Status, info.ModelProvider, info.ModelID, info.ThinkingEffort, len(snapshot.Messages))
	if info.LastError != nil {
		text += "\nlast_error: " + *info.LastError
	}
	for index := len(snapshot.Messages) - 1; index >= 0; index-- {
		if assistant, ok := snapshot.Messages[index].Standard.(types.AssistantMessage); ok {
			var content strings.Builder
			for _, block := range assistant.Content {
				if block, ok := block.(types.TextBlock); ok {
					content.WriteString(block.Text)
				}
			}
			if strings.TrimSpace(content.String()) != "" {
				text += "\nlast_assistant: " + strings.TrimSpace(content.String())
				break
			}
		}
	}
	return result(text, map[string]any{"action": params.Action, "id": info.ID, "name": info.Name, "status": info.Status, "model_id": info.ModelID, "model_provider": info.ModelProvider, "thinking_effort": info.ThinkingEffort})
}

func result(text string, details any) (tools.Result, error) {
	data, err := json.Marshal(details)
	if err != nil {
		return tools.Result{}, err
	}
	return tools.Result{Content: []types.ContentBlock{types.TextBlock{Text: text}}, Details: data}, nil
}

func parseScope(raw json.RawMessage) (app.Scope, error) {
	if len(raw) == 0 || string(raw) == "null" {
		return app.Inherit(), nil
	}
	var name string
	if json.Unmarshal(raw, &name) == nil {
		if name == "readonly" {
			return app.Readonly(), nil
		}
		if name == "inherit" {
			return app.Inherit(), nil
		}
		return app.Scope{}, fmt.Errorf("unknown subagent scope: %s", name)
	}
	var object struct {
		Type string `json:"type"`
		app.Scope
	}
	if err := json.Unmarshal(raw, &object); err != nil {
		return app.Scope{}, fmt.Errorf("invalid subagent scope: %w", err)
	}
	switch object.Type {
	case "scoped":
		return app.Scoped(object.Paths), nil
	case "custom":
		return object.Scope, nil
	default:
		return app.Scope{}, fmt.Errorf("unknown subagent scope type: %s", object.Type)
	}
}

var schema = json.RawMessage(`{"type":"object","properties":{"action":{"type":"string","enum":["spawn","send","wait","interrupt","list"]},"id":{"type":"string"},"name":{"type":"string"},"system_prompt":{"type":"string"},"prompt":{"type":"string"},"wait":{"type":"boolean"},"scope":{"description":"Scope: inherit, readonly, or object with type scoped/custom, tools, paths, bash_prefixes, skills"}},"required":["action"]}`)
