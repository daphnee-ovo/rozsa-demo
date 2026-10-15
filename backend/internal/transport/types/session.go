// Package types owns implemented business payloads and errors, not the wire protocol.
package types

import (
	"bytes"
	"encoding/json"
	"go.lsp.dev/jsonrpc2"
)

type Method string

const (
	Initialize    Method = "initialize"
	GetSessions   Method = "get_sessions"
	SwitchSession Method = "switch_session"
	Shutdown      Method = "shutdown"
)

type InitializeParams struct {
	ProtocolVersion int `json:"protocolVersion"`
}
type InitializeResult struct {
	ProtocolVersion int      `json:"protocolVersion"`
	Capabilities    []string `json:"capabilities"`
}
type EmptyParams struct{}
type SwitchParams struct {
	Path      string `json:"path"`
	CatalogID string `json:"catalogId"`
}
type ErrorKind string

const (
	InvalidParams  ErrorKind = "invalid_params"
	MethodNotFound ErrorKind = "method_not_found"
	NotInitialized ErrorKind = "not_initialized"
	Catalog        ErrorKind = "catalog"
	Session        ErrorKind = "session"
	Closing        ErrorKind = "closing"
)

type ErrorData struct {
	Kind ErrorKind `json:"kind"`
}

func Error(kind ErrorKind, message string) *jsonrpc2.Error {
	var code jsonrpc2.Code
	switch kind {
	case InvalidParams:
		code = jsonrpc2.InvalidParams
	case MethodNotFound:
		code = jsonrpc2.MethodNotFound
	case NotInitialized:
		code = -32002
	case Catalog, Session:
		code = -32001
	case Closing:
		code = -32003
	default:
		panic("unmapped RPC error kind")
	}
	data, err := json.Marshal(ErrorData{Kind: kind})
	if err != nil {
		panic(err)
	} // ErrorData contains only an enumerated string.
	return &jsonrpc2.Error{Code: code, Message: message, Data: jsonrpc2.RawMessage(data)}
}

// Only decodes business payloads. The library normalizes missing/null to nil.
func Params(data jsonrpc2.RawMessage, dst any) bool {
	if len(data) == 0 {
		data = []byte("{}")
	}
	if !bytes.HasPrefix(bytes.TrimSpace(data), []byte("{")) {
		return false
	}
	decoder := json.NewDecoder(bytes.NewReader(data))
	decoder.DisallowUnknownFields()
	return decoder.Decode(dst) == nil
}
