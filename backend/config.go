// Package backend provides build-time configuration resources for the Go backend.
package backend

import _ "embed"

//go:embed config.yaml
var builtinConfig []byte

// BuiltinConfig returns the configuration compiled into the backend binary.
func BuiltinConfig() []byte {
	return append([]byte(nil), builtinConfig...)
}
