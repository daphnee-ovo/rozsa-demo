package server

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"

	"rozsa/backend/internal/app/permissions"
	"rozsa/backend/internal/app/questions"
	"rozsa/backend/internal/transport/types"
)

func (server *rpcServer) interruptionHandle(
	ctx context.Context,
	method types.Method,
	stateParams types.GetInterruptionStateParams,
	permissionParams types.PermissionResponseParams,
	questionParams types.UserQuestionResponseParams,
	cancelParams types.CancelUserQuestionParams,
) (any, error) {
	application := server.interruptions
	if application == nil {
		return nil, errors.New("interruption operations are unavailable")
	}
	switch method {
	case types.GetInterruptionState:
		pending, err := application.PendingApprovals(ctx, stateParams.SessionID)
		if err != nil {
			return nil, err
		}
		state := types.InterruptionState{Approvals: make([]types.ApprovalRequest, 0, len(pending))}
		for _, request := range pending {
			if request.Status == permissions.Cancelled || request.Status == permissions.Stale {
				continue
			}
			state.Approvals = append(state.Approvals, approvalRequest(request))
		}
		question, exists, err := application.NextQuestion(ctx, stateParams.SessionID)
		if err != nil {
			return nil, err
		}
		if exists {
			state.Question = userQuestionRequest(question)
		}
		return state, nil
	case types.RespondPermission:
		response, err := permissionResponse(permissionParams)
		if err != nil {
			return nil, err
		}
		identity := permissions.Identity{SessionID: permissionParams.SessionID, RequestID: permissionParams.RequestID}
		if err := application.RespondApproval(ctx, identity, response); err != nil {
			return nil, err
		}
		return true, nil
	case types.RespondUserQuestion:
		answers, err := questionAnswers(questionParams.Answers)
		if err != nil {
			return nil, err
		}
		if err := application.RespondQuestion(ctx, questionParams.RequestID, answers); err != nil {
			return nil, err
		}
		return true, nil
	case types.CancelUserQuestion:
		if err := application.CancelQuestion(ctx, cancelParams.RequestID); err != nil {
			return nil, err
		}
		return true, nil
	default:
		return nil, errors.New("unsupported interruption operation")
	}
}

func permissionResponse(params types.PermissionResponseParams) (permissions.Response, error) {
	trustKeys := append([]string(nil), params.TrustKeys...)
	if len(trustKeys) == 0 && params.TrustKey != "" {
		trustKeys = []string{params.TrustKey}
	}
	switch params.Choice {
	case "allow", "allow-once":
		return permissions.Response{Kind: permissions.AllowOnce}, nil
	case "allow-session":
		return permissions.Response{Kind: permissions.AllowSession, TrustKey: params.TrustKey, TrustKeys: trustKeys}, nil
	case "allow-project":
		return permissions.Response{Kind: permissions.AllowProject, TrustKey: params.TrustKey, TrustKeys: trustKeys}, nil
	case "deny":
		return permissions.Response{Kind: permissions.Deny}, nil
	case "deny-hint":
		return permissions.Response{Kind: permissions.DenyWithHint, Hint: params.Hint}, nil
	default:
		return permissions.Response{}, types.Error(types.InvalidParams, fmt.Sprintf("unsupported permission choice: %q", params.Choice))
	}
}

func questionAnswers(raw map[string]json.RawMessage) (questions.Answers, error) {
	answers := make(questions.Answers, len(raw))
	for header, value := range raw {
		var single string
		if err := json.Unmarshal(value, &single); err == nil {
			answers[header] = questions.Single(single)
			continue
		}
		var multiple []string
		if err := json.Unmarshal(value, &multiple); err == nil && multiple != nil {
			answers[header] = questions.Multiple(multiple...)
			continue
		}
		return nil, types.Error(types.InvalidParams, fmt.Sprintf("answer for %q must be a string or string list", header))
	}
	return answers, nil
}

func approvalRequest(request permissions.Request) types.ApprovalRequest {
	levels := make([]types.ApprovalTrustLevel, 0, len(request.Info.TrustLevels))
	for _, level := range request.Info.TrustLevels {
		levels = append(levels, types.ApprovalTrustLevel{Key: level.Key, Label: level.Label})
	}
	groups := make([]types.ApprovalTrustGroup, 0, len(request.Info.TrustGroups))
	for _, group := range request.Info.TrustGroups {
		groupLevels := make([]types.ApprovalTrustLevel, 0, len(group.Levels))
		for _, level := range group.Levels {
			groupLevels = append(groupLevels, types.ApprovalTrustLevel{Key: level.Key, Label: level.Label})
		}
		groups = append(groups, types.ApprovalTrustGroup{Target: group.Target, Levels: groupLevels})
	}
	return types.ApprovalRequest{
		SessionID: request.Identity.SessionID, RequestID: request.Identity.RequestID,
		Tool: request.Info.ToolName, Summary: request.Info.ArgsSummary, Command: request.Info.Command, Risk: string(request.Info.Risk),
		TrustKey: request.Info.TrustKey, TrustLevels: levels, TrustGroups: groups, Status: approvalStatus(request.Status),
	}
}

func approvalStatus(status permissions.RequestStatus) string {
	switch status {
	case permissions.Pending:
		return "pending"
	case permissions.Queued:
		return "queued"
	case permissions.Responding:
		return "responding"
	case permissions.Failed:
		return "failed"
	case permissions.Cancelled:
		return "cancelled"
	case permissions.Stale:
		return "stale"
	default:
		return "unknown"
	}
}

func userQuestionRequest(request questions.Request) *types.UserQuestionRequest {
	items := make([]types.UserQuestion, 0, len(request.Questions))
	for _, question := range request.Questions {
		options := make([]types.QuestionOption, 0, len(question.Options))
		for _, option := range question.Options {
			options = append(options, types.QuestionOption{Label: option.Label, Description: option.Description})
		}
		items = append(items, types.UserQuestion{
			Question: question.Question, Header: question.Header, Options: options, MultiSelect: question.MultiSelect,
		})
	}
	return &types.UserQuestionRequest{
		SessionID: request.SessionID, RequestID: request.RequestID, TransportID: request.TransportID, Questions: items,
	}
}
