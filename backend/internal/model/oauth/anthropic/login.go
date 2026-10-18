package anthropic

import (
	"context"
	"errors"
	"fmt"
	"net/url"
	"strings"

	"rozsa/backend/internal/model/credentials"
	"rozsa/backend/internal/model/oauth/common"
)

type EventKind uint8

const (
	EventAuthorizationURL EventKind = iota + 1
	EventProgress
)

// Event contains user-presentable OAuth flow information and never tokens.
type Event struct {
	Kind         EventKind
	URL          string
	Instructions string
	Message      string
}

type InputKind uint8

const (
	InputAuthorizationResponse InputKind = iota + 1
)

// Input carries a manually pasted callback URL, query string, code#state, or
// raw authorization code.
type Input struct {
	Kind  InputKind
	Value string
}

// Login starts the PKCE flow and waits for either the loopback callback or a
// typed manual authorization response. The returned credential is not stored.
func (client *Client) Login(ctx context.Context, events chan<- Event, inputs <-chan Input) (credentials.OAuthCredential, error) {
	pkce, err := common.GeneratePKCE()
	if err != nil {
		return credentials.OAuthCredential{}, err
	}
	state, err := common.GenerateState()
	if err != nil {
		return credentials.OAuthCredential{}, err
	}
	authorizationURL, err := client.authorizationURL(pkce, state)
	if err != nil {
		return credentials.OAuthCredential{}, err
	}
	if err := sendEvent(ctx, events, Event{
		Kind:         EventAuthorizationURL,
		URL:          authorizationURL,
		Instructions: "Complete login in your browser. If the browser is on another machine, paste the final redirect URL here.",
	}); err != nil {
		return credentials.OAuthCredential{}, err
	}

	callbackContext, cancelCallback := context.WithCancel(ctx)
	defer cancelCallback()
	type callbackResult struct {
		code common.AuthorizationCode
		err  error
	}
	callback := make(chan callbackResult, 1)
	go func() {
		code, callbackErr := common.WaitForCallback(callbackContext, client.config.CallbackAddress, state)
		callback <- callbackResult{code: code, err: callbackErr}
	}()

	var authorizationCode string
	select {
	case result := <-callback:
		if result.err != nil {
			if errors.Is(result.err, context.Canceled) && ctx.Err() != nil {
				return credentials.OAuthCredential{}, ctx.Err()
			}
			return credentials.OAuthCredential{}, fmt.Errorf("receive Anthropic OAuth callback: %w", result.err)
		}
		authorizationCode = result.code.Code
	case input, ok := <-inputs:
		if !ok {
			return credentials.OAuthCredential{}, errors.New("Anthropic OAuth input channel closed")
		}
		if input.Kind != InputAuthorizationResponse {
			return credentials.OAuthCredential{}, fmt.Errorf("unsupported Anthropic OAuth input kind %d", input.Kind)
		}
		parsed, parseErr := common.ParseAuthorizationInput(input.Value, state)
		if parseErr != nil {
			return credentials.OAuthCredential{}, fmt.Errorf("parse Anthropic OAuth authorization response: %w", parseErr)
		}
		authorizationCode = parsed.Code
	case <-ctx.Done():
		return credentials.OAuthCredential{}, ctx.Err()
	}
	if err := ctx.Err(); err != nil {
		return credentials.OAuthCredential{}, err
	}
	if err := sendEvent(ctx, events, Event{Kind: EventProgress, Message: "Exchanging authorization code for tokens..."}); err != nil {
		return credentials.OAuthCredential{}, err
	}
	return client.exchange(ctx, tokenRequest{
		GrantType:    "authorization_code",
		ClientID:     clientID,
		Code:         authorizationCode,
		RedirectURI:  client.config.RedirectURI,
		CodeVerifier: pkce.Verifier,
	}, "")
}

func sendEvent(ctx context.Context, events chan<- Event, event Event) error {
	if events == nil {
		return nil
	}
	select {
	case events <- event:
		return nil
	case <-ctx.Done():
		return ctx.Err()
	}
}

// ParseAuthorizationInput is exported for callers that collect input before
// forwarding it to Login, while keeping state validation identical.
func ParseAuthorizationInput(input, expectedState string) (common.AuthorizationCode, error) {
	return common.ParseAuthorizationInput(input, expectedState)
}

// AuthorizationState extracts state from a generated authorization URL for
// UI adapters that need to validate pre-collected input. It does not expose a
// verifier or token.
func AuthorizationState(rawURL string) (string, error) {
	parsed, err := url.Parse(rawURL)
	if err != nil {
		return "", fmt.Errorf("parse Anthropic authorization URL: %w", err)
	}
	state := strings.TrimSpace(parsed.Query().Get("state"))
	if state == "" {
		return "", errors.New("Anthropic authorization URL is missing state")
	}
	return state, nil
}
