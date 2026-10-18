package server

import (
	"context"

	"rozsa/backend/internal/app/auth"
	"rozsa/backend/internal/transport/types"
)

type AuthApplication interface{ AuthService() auth.Service }
type authParams struct {
	Provider    string `json:"provider,omitempty"`
	OperationID string `json:"operationId,omitempty"`
	Value       string `json:"value,omitempty"`
}

func (server *rpcServer) authHandle(ctx context.Context, method types.Method, params authParams) (any, error) {
	application, ok := server.messages.(AuthApplication)
	if !ok || application.AuthService() == nil {
		return nil, types.Error(types.MethodNotFound, "OAuth is unavailable")
	}
	service := application.AuthService()
	var result any = true
	var err error
	switch method {
	case types.AuthLogin:
		var id string
		id, err = service.Start(params.Provider)
		result = map[string]string{"operationId": id}
	case types.AuthInput:
		err = service.Input(params.OperationID, params.Value)
	case types.AuthNext:
		result, err = service.Next(ctx, params.OperationID)
	case types.AuthCancel:
		err = service.Cancel(params.OperationID)
	case types.AuthLogout:
		err = service.Logout(ctx, params.Provider)
	}
	if err != nil {
		return nil, types.AuthError(err.Error())
	}
	return result, nil
}
