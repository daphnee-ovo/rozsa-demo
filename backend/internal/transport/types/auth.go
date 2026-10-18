package types

import (
	"encoding/json"
	"go.lsp.dev/jsonrpc2"
)

const (
	AuthLogin  Method = "auth_login"
	AuthInput  Method = "auth_input"
	AuthNext   Method = "auth_next"
	AuthCancel Method = "auth_cancel"
	AuthLogout Method = "auth_logout"
)

func AuthError(message string) *jsonrpc2.Error {
	data, err := json.Marshal(ErrorData{Kind: "auth"})
	if err != nil { panic(err) }
	return &jsonrpc2.Error{Code: -32011, Message: message, Data: jsonrpc2.RawMessage(data)}
}
