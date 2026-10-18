// Package backend provides build-time configuration resources for the Go backend.
package backend

import _ "embed"

//go:embed config.yaml
var BuiltinConfig []byte
