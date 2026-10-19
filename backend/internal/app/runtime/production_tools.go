package runtime

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"math"
	"strings"
	"time"

	"rozsa/backend/internal/app/session"
	bash "rozsa/backend/internal/app/tools/bash"
	filetools "rozsa/backend/internal/app/tools/file"
	"rozsa/backend/internal/core/tools"
	"rozsa/backend/internal/model/types"
)

type readTool struct{}
type writeTool struct{}
type bashTool struct{ manager *session.Manager }

func newProductionTools(manager *session.Manager) []tools.Tool {
	return []tools.Tool{readTool{}, writeTool{}, bashTool{manager: manager}}
}

func (readTool) Name() string  { return "read" }
func (readTool) Label() string { return "Read" }
func (readTool) Description() string {
	return "Read the contents of a file. Output is formatted with line numbers and truncated to 2000 lines or 50KB."
}
func (readTool) ParametersSchema() json.RawMessage                          { return readToolSchema }
func (readTool) PrepareArguments(arguments json.RawMessage) json.RawMessage { return arguments }
func (readTool) ExecutionMode() *tools.ExecutionMode                        { return nil }

func (readTool) Execute(ctx context.Context, _ string, arguments json.RawMessage, _ func(tools.Result)) (tools.Result, error) {
	if err := ctx.Err(); err != nil {
		return tools.Result{}, err
	}
	var params struct {
		FilePath string `json:"file_path"`
		Path     string `json:"path"`
		Offset   *int   `json:"offset"`
		Limit    *int   `json:"limit"`
	}
	if err := json.Unmarshal(arguments, &params); err != nil {
		return tools.Result{}, fmt.Errorf("invalid Read parameters: %w", err)
	}
	if params.FilePath == "" {
		params.FilePath = params.Path
	}
	if params.FilePath == "" {
		return tools.Result{}, errors.New("Read requires file_path")
	}
	result, err := filetools.Read(params.FilePath, params.Offset, params.Limit)
	if err != nil {
		return textToolResult(err.Error(), json.RawMessage(`{}`)), nil
	}
	details, err := json.Marshal(result.Details)
	if err != nil {
		return tools.Result{}, fmt.Errorf("encode Read details: %w", err)
	}
	return textToolResult(result.Text, details), nil
}

func (writeTool) Name() string  { return "write" }
func (writeTool) Label() string { return "Write" }
func (writeTool) Description() string {
	return "Write content to a file. Creates the file if it does not exist, overwrites it if it does, and creates parent directories."
}
func (writeTool) ParametersSchema() json.RawMessage                          { return writeToolSchema }
func (writeTool) PrepareArguments(arguments json.RawMessage) json.RawMessage { return arguments }
func (writeTool) ExecutionMode() *tools.ExecutionMode                        { return nil }

func (writeTool) Execute(ctx context.Context, _ string, arguments json.RawMessage, _ func(tools.Result)) (tools.Result, error) {
	var params struct {
		FilePath string `json:"file_path"`
		Path     string `json:"path"`
		Content  string `json:"content"`
	}
	if err := json.Unmarshal(arguments, &params); err != nil {
		return tools.Result{}, fmt.Errorf("invalid Write parameters: %w", err)
	}
	if params.FilePath == "" {
		params.FilePath = params.Path
	}
	if params.FilePath == "" {
		return tools.Result{}, errors.New("Write requires file_path")
	}
	result, err := filetools.Write(ctx, params.FilePath, params.Content)
	if err != nil {
		return tools.Result{}, err
	}
	details, err := json.Marshal(result.Details)
	if err != nil {
		return tools.Result{}, fmt.Errorf("encode Write details: %w", err)
	}
	return textToolResult(result.Text, details), nil
}

func (tool bashTool) Name() string  { return "bash" }
func (tool bashTool) Label() string { return "Bash" }
func (tool bashTool) Description() string {
	return "Execute a bash command in the current session working directory. Combined output is limited to 100KB; timeout defaults to 120000ms."
}
func (tool bashTool) ParametersSchema() json.RawMessage                          { return bashToolSchema }
func (tool bashTool) PrepareArguments(arguments json.RawMessage) json.RawMessage { return arguments }
func (tool bashTool) ExecutionMode() *tools.ExecutionMode {
	mode := tools.Sequential
	return &mode
}

func (tool bashTool) Execute(ctx context.Context, _ string, arguments json.RawMessage, _ func(tools.Result)) (tools.Result, error) {
	if tool.manager == nil {
		return tools.Result{}, errors.New("Bash requires a session manager")
	}
	var params struct {
		Command     string `json:"command"`
		TimeoutMs   *int64 `json:"timeout"`
		Description string `json:"description"`
	}
	if err := json.Unmarshal(arguments, &params); err != nil {
		return tools.Result{}, fmt.Errorf("invalid Bash parameters: %w", err)
	}
	if params.Command == "" {
		return tools.Result{}, errors.New("Bash requires command")
	}
	timeout := bash.DefaultTimeout
	if params.TimeoutMs != nil {
		if *params.TimeoutMs < 0 || *params.TimeoutMs > math.MaxInt64/int64(time.Millisecond) {
			return tools.Result{}, errors.New("Bash timeout is outside the supported range")
		}
		if *params.TimeoutMs > 0 {
			timeout = time.Duration(*params.TimeoutMs) * time.Millisecond
		}
	}
	workingDirectory := tool.manager.Cwd()
	if workingDirectory == "" {
		return tools.Result{}, errors.New("Bash session working directory is empty")
	}
	result, executeErr := bash.Execute(ctx, params.Command, workingDirectory, timeout)
	if ctx.Err() != nil {
		return tools.Result{}, ctx.Err()
	}
	if executeErr == nil && result.CWD != "" {
		if err := tool.manager.SetCwd(result.CWD); err != nil {
			return tools.Result{}, fmt.Errorf("command ran but failed to persist cwd: %w", err)
		}
	}
	text := result.Output
	details := map[string]any{
		"command": params.Command, "cwd": nil, "exit_code": nil, "success": false,
		"timed_out": result.TimedOut, "truncated": result.Truncated,
		"timeout_ms": timeout.Milliseconds(), "duration_ms": result.Duration.Milliseconds(),
	}
	if executeErr != nil {
		text = executeErr.Error()
	} else {
		var cwd *string
		if result.CWD != "" {
			cwd = &result.CWD
		}
		details["cwd"] = cwd
		details["exit_code"] = result.ExitCode
		details["success"] = result.ExitCode != nil && *result.ExitCode == 0 && !result.Truncated
		if result.Truncated {
			text += "\n\n[Output truncated at 100KB limit]"
		}
		if result.ExitCode != nil && *result.ExitCode != 0 {
			text = strings.TrimRight(text, "\n") + fmt.Sprintf("\n\nCommand exited with code %d", *result.ExitCode)
		}
	}
	encodedDetails, err := json.Marshal(details)
	if err != nil {
		return tools.Result{}, fmt.Errorf("encode Bash details: %w", err)
	}
	return textToolResult(text, encodedDetails), nil
}

func textToolResult(text string, details json.RawMessage) tools.Result {
	return tools.Result{Content: []types.ContentBlock{types.TextBlock{Text: text}}, Details: details}
}

var readToolSchema = json.RawMessage(`{"type":"object","properties":{"file_path":{"type":"string","description":"Path to the file to read (relative or absolute)"},"offset":{"type":"number","description":"Line number to start reading from (1-indexed)"},"limit":{"type":"number","description":"Maximum number of lines to read"}},"required":["file_path"]}`)
var writeToolSchema = json.RawMessage(`{"type":"object","properties":{"file_path":{"type":"string","description":"Path to the file to write (relative or absolute)"},"content":{"type":"string","description":"Content to write to the file"}},"required":["file_path","content"]}`)
var bashToolSchema = json.RawMessage(`{"type":"object","properties":{"command":{"type":"string","description":"Bash command to execute"},"timeout":{"type":"number","description":"Timeout in milliseconds (default: 120000)"},"description":{"type":"string","description":"Optional description of what this command does"}},"required":["command"]}`)
