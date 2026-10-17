// Package model owns the public protocol boundary and API-to-implementation
// construction mapping.
package model

import (
	"fmt"

	"rozsa/backend/internal/model/anthropic"
	"rozsa/backend/internal/model/completions"
	"rozsa/backend/internal/model/responses"
	"rozsa/backend/internal/model/types"
)

// NewProtocol constructs the implementation registered for api.
func NewProtocol(api types.API) (types.Protocol, error) {
	switch api {
	case types.APIAnthropicMessages:
		return anthropic.NewProvider(nil), nil
	case types.APIOpenAICompletions:
		return completions.NewProvider(nil), nil
	case types.APIOpenAIResponses:
		return responses.NewProvider(nil), nil
	default:
		return nil, fmt.Errorf("unsupported model API %q", api)
	}
}

var _ types.Protocol = (*anthropic.Provider)(nil)
var _ types.Protocol = (*completions.Provider)(nil)
var _ types.Protocol = (*responses.Provider)(nil)
