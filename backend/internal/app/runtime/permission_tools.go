package runtime

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"strings"
	"sync"

	"rozsa/backend/internal/app/permissions"
	"rozsa/backend/internal/core/hooks"
)

type permissionRuntime struct {
	mu           sync.Mutex
	policies     map[string]permissions.Policy
	roots        map[string]string
	waiters      map[permissions.Identity]*approvalWaiter
	projectStore permissions.ProjectRuleStore
}

type approvalWaiter struct {
	response chan permissions.Response
	policy   permissions.Policy
	args     map[string]any
}

func newPermissionRuntime(projectStore permissions.ProjectRuleStore) *permissionRuntime {
	return &permissionRuntime{
		policies:     make(map[string]permissions.Policy),
		roots:        make(map[string]string),
		waiters:      make(map[permissions.Identity]*approvalWaiter),
		projectStore: projectStore,
	}
}

func (runtime *Runtime) permissionHook(sessionID, workspaceRoot string) hooks.PreToolUse {
	return func(ctx context.Context, request hooks.PreToolUseContext) (*hooks.PreToolUseResult, error) {
		arguments := map[string]any{}
		if len(request.Arguments) != 0 && string(request.Arguments) != "null" {
			if err := json.Unmarshal(request.Arguments, &arguments); err != nil {
				return nil, fmt.Errorf("decode permission arguments: %w", err)
			}
		}
		policy, err := runtime.permissionPolicy(sessionID, runtime.projectRoot)
		if err != nil {
			return nil, err
		}
		verdict := policy.Evaluate(sessionID, request.ToolName, arguments)
		switch verdict.Kind {
		case permissions.Allow:
			return nil, nil
		case permissions.Block:
			return permissionBlock(verdict.Reason), nil
		case permissions.NeedApproval:
			if verdict.Info == nil {
				return nil, errors.New("permission policy requested approval without request details")
			}
			return runtime.waitForApproval(ctx, policy, permissions.Identity{SessionID: sessionID, RequestID: request.CallID}, *verdict.Info, arguments)
		default:
			return nil, fmt.Errorf("unsupported permission verdict: %d", verdict.Kind)
		}
	}
}

func permissionBlock(reason string) *hooks.PreToolUseResult {
	if reason == "" {
		reason = "Tool execution was blocked by permission policy"
	}
	return &hooks.PreToolUseResult{Block: true, Reason: &reason}
}

func (runtime *Runtime) permissionPolicy(sessionID, workspaceRoot string) (permissions.Policy, error) {
	settings := map[string]any{}
	if runtime.resolvedSettings != nil {
		settings = runtime.resolvedSettings()
	}
	mode, rules, err := permissionSettings(settings)
	if err != nil {
		return nil, err
	}
	state := runtime.permissionRuntime
	state.mu.Lock()
	defer state.mu.Unlock()
	policy := state.policies[sessionID]
	if policy == nil {
		policy, err = permissions.NewPolicy(mode, workspaceRoot, rules)
		if err != nil {
			return nil, err
		}
		state.policies[sessionID], state.roots[sessionID] = policy, workspaceRoot
		return policy, nil
	}
	root := state.roots[sessionID]
	if root == "" {
		root = workspaceRoot
		state.roots[sessionID] = root
	}
	if err := policy.Update(mode, root, rules); err != nil {
		return nil, err
	}
	return policy, nil
}

func permissionSettings(settings map[string]any) (permissions.Mode, permissions.Rules, error) {
	mode := permissions.OnRequest
	rules := permissions.Rules{Allow: permissions.DefaultAllowRules()}
	value, exists := settings["permission"]
	if !exists || value == nil {
		return mode, rules, nil
	}
	permission, ok := value.(map[string]any)
	if !ok {
		return "", permissions.Rules{}, errors.New("settings.permission must be an object")
	}
	if rawMode, exists := permission["mode"]; exists && rawMode != nil {
		modeValue, ok := rawMode.(string)
		if !ok {
			return "", permissions.Rules{}, errors.New("settings.permission.mode must be a string")
		}
		parsedMode, err := permissions.ParseMode(modeValue)
		if err != nil {
			return "", permissions.Rules{}, err
		}
		mode = parsedMode
	}
	for name, target := range map[string]*[]string{"deny": &rules.Deny, "ask": &rules.Ask, "allow": &rules.Allow} {
		if rawRules, exists := permission[name]; exists && rawRules != nil {
			parsedRules, ok := settingStrings(rawRules)
			if !ok {
				return "", permissions.Rules{}, fmt.Errorf("settings.permission.%s must be an array of strings", name)
			}
			*target = parsedRules
		}
	}
	return mode, rules, nil
}

func settingStrings(value any) ([]string, bool) {
	switch values := value.(type) {
	case []string:
		return append([]string(nil), values...), true
	case []any:
		result := make([]string, 0, len(values))
		for _, value := range values {
			text, ok := value.(string)
			if !ok {
				return nil, false
			}
			result = append(result, text)
		}
		return result, true
	default:
		return nil, false
	}
}

func (runtime *Runtime) waitForApproval(ctx context.Context, policy permissions.Policy, identity permissions.Identity, info permissions.ApprovalInfo, args map[string]any) (*hooks.PreToolUseResult, error) {
	waiter := &approvalWaiter{response: make(chan permissions.Response, 1), policy: policy, args: args}
	state := runtime.permissionRuntime
	state.mu.Lock()
	if _, exists := state.waiters[identity]; exists {
		state.mu.Unlock()
		return nil, permissions.ErrDuplicateRequest
	}
	state.waiters[identity] = waiter
	if err := runtime.approvals.Add(identity, info); err != nil {
		delete(state.waiters, identity)
		state.mu.Unlock()
		return nil, err
	}
	state.mu.Unlock()

	select {
	case <-ctx.Done():
		state.mu.Lock()
		if state.waiters[identity] == waiter {
			delete(state.waiters, identity)
			_ = runtime.approvals.Mark(identity, permissions.Cancelled)
		}
		state.mu.Unlock()
		return nil, ctx.Err()
	case response := <-waiter.response:
		if err := ctx.Err(); err != nil {
			return nil, err
		}
		switch response.Kind {
		case permissions.AllowOnce, permissions.AllowSession, permissions.AllowProject:
			return nil, nil
		case permissions.Deny:
			return permissionBlock("Permission denied by user"), nil
		case permissions.DenyWithHint:
			hint := strings.TrimSpace(response.Hint)
			if hint == "" {
				hint = permissions.SaferAlternativeHint(info.ToolName, args)
			}
			return permissionBlock("Permission denied by user. Hint: " + hint), nil
		default:
			return nil, fmt.Errorf("unsupported permission response: %d", response.Kind)
		}
	}
}

func (runtime *Runtime) RespondApproval(ctx context.Context, identity permissions.Identity, response permissions.Response) error {
	if err := ctx.Err(); err != nil {
		return err
	}
	state := runtime.permissionRuntime
	state.mu.Lock()
	request, exists := runtime.approvals.Get(identity)
	waiter := state.waiters[identity]
	if !exists || waiter == nil {
		state.mu.Unlock()
		return permissions.ErrRequestNotFound
	}
	if err := validateApprovalResponse(request.Info, response); err != nil {
		state.mu.Unlock()
		return err
	}
	if err := runtime.approvals.BeginResponse(identity, response); err != nil {
		state.mu.Unlock()
		return err
	}
	state.mu.Unlock()

	if response.Kind == permissions.DenyWithHint && strings.TrimSpace(response.Hint) == "" {
		response.Hint = permissions.SaferAlternativeHint(request.Info.ToolName, waiter.args)
	}
	if err := runtime.applyApprovalResponse(identity, response, waiter); err != nil {
		return errors.Join(err, runtime.approvals.FinishResponse(identity, false))
	}
	if err := runtime.approvals.FinishResponse(identity, true); err != nil {
		return err
	}
	state.mu.Lock()
	if state.waiters[identity] != waiter {
		state.mu.Unlock()
		return permissions.ErrRequestNotFound
	}
	delete(state.waiters, identity)
	waiter.response <- response
	state.mu.Unlock()
	return nil
}

func validateApprovalResponse(info permissions.ApprovalInfo, response permissions.Response) error {
	if response.Kind != permissions.AllowSession && response.Kind != permissions.AllowProject {
		return nil
	}
	keys := response.TrustKeys
	if len(keys) == 0 && response.TrustKey != "" {
		keys = []string{response.TrustKey}
	}
	if len(keys) == 0 {
		return errors.New("approval requires at least one trust scope")
	}
	valid := map[string]struct{}{}
	if info.TrustKey != "" {
		valid[info.TrustKey] = struct{}{}
	}
	for _, level := range info.TrustLevels {
		valid[level.Key] = struct{}{}
	}
	for _, group := range info.TrustGroups {
		for _, level := range group.Levels {
			valid[level.Key] = struct{}{}
		}
	}
	for _, key := range keys {
		if _, ok := valid[key]; !ok {
			return errors.New("Selected trust scope is not valid for this request")
		}
	}
	return nil
}

func (runtime *Runtime) applyApprovalResponse(identity permissions.Identity, response permissions.Response, waiter *approvalWaiter) error {
	keys := response.TrustKeys
	if len(keys) == 0 && response.TrustKey != "" {
		keys = []string{response.TrustKey}
	}
	switch response.Kind {
	case permissions.AllowSession:
		for _, trustKey := range keys {
			if err := waiter.policy.ApplyResponse(identity.SessionID, permissions.Response{Kind: permissions.AllowSession, TrustKey: trustKey}); err != nil {
				return err
			}
		}
		return nil
	case permissions.AllowProject:
		for _, trustKey := range keys {
			if err := waiter.policy.RecordProjectApproval(trustKey, runtime.permissionRuntime.projectStore); err != nil {
				return err
			}
		}
		return nil
	default:
		return waiter.policy.ApplyResponse(identity.SessionID, response)
	}
}
