// Package anthropic implements the Anthropic OAuth authorization-code flow.
package anthropic

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"net/url"
	"strings"
	"time"

	"rozsa/backend/internal/model/credentials"
	"rozsa/backend/internal/model/oauth/common"
)

const (
	clientID               = "9d1c250a-e61b-44d9-88ed-5944d1962f5e"
	defaultAuthURL         = "https://claude.ai/oauth/authorize"
	defaultTokenURL        = "https://platform.claude.com/v1/oauth/token"
	defaultRedirectURI     = "http://127.0.0.1:53692/callback"
	defaultCallbackAddress = "127.0.0.1:53692"
	scope                  = "org:create_api_key user:profile user:inference user:sessions:claude_code user:mcp_servers user:file_upload"
)

// Config contains endpoints and loopback settings. Empty fields use the
// production Anthropic defaults.
type Config struct {
	AuthorizationURL string
	TokenURL         string
	RedirectURI      string
	CallbackAddress  string
}

// Client performs Anthropic OAuth requests.
type Client struct {
	httpClient *http.Client
	config     Config
}

func NewClient(httpClient *http.Client) *Client {
	return NewClientWithConfig(httpClient, Config{})
}

func NewClientWithConfig(httpClient *http.Client, config Config) *Client {
	if httpClient == nil {
		httpClient = &http.Client{Timeout: 30 * time.Second}
	}
	if config.AuthorizationURL == "" {
		config.AuthorizationURL = defaultAuthURL
	}
	if config.TokenURL == "" {
		config.TokenURL = defaultTokenURL
	}
	if config.RedirectURI == "" {
		config.RedirectURI = defaultRedirectURI
	}
	if config.CallbackAddress == "" {
		config.CallbackAddress = defaultCallbackAddress
	}
	return &Client{httpClient: httpClient, config: config}
}

type tokenRequest struct {
	GrantType    string `json:"grant_type"`
	ClientID     string `json:"client_id"`
	Code         string `json:"code,omitempty"`
	RedirectURI  string `json:"redirect_uri,omitempty"`
	CodeVerifier string `json:"code_verifier,omitempty"`
	RefreshToken string `json:"refresh_token,omitempty"`
}

type tokenResponse struct {
	AccessToken  string `json:"access_token"`
	RefreshToken string `json:"refresh_token"`
	ExpiresIn    int64  `json:"expires_in"`
}

func (client *Client) exchange(ctx context.Context, request tokenRequest, fallbackRefresh string) (credentials.OAuthCredential, error) {
	body, err := json.Marshal(request)
	if err != nil {
		return credentials.OAuthCredential{}, fmt.Errorf("encode Anthropic OAuth token request: %w", err)
	}
	httpRequest, err := http.NewRequestWithContext(ctx, http.MethodPost, client.config.TokenURL, strings.NewReader(string(body)))
	if err != nil {
		return credentials.OAuthCredential{}, fmt.Errorf("create Anthropic OAuth token request: %w", err)
	}
	httpRequest.Header.Set("Content-Type", "application/json")
	response, err := client.httpClient.Do(httpRequest)
	if err != nil {
		return credentials.OAuthCredential{}, fmt.Errorf("send Anthropic OAuth token request: %w", err)
	}
	defer response.Body.Close()
	if response.StatusCode < http.StatusOK || response.StatusCode >= http.StatusMultipleChoices {
		return credentials.OAuthCredential{}, fmt.Errorf("Anthropic OAuth token request failed with HTTP %d", response.StatusCode)
	}
	var token tokenResponse
	if err := json.NewDecoder(response.Body).Decode(&token); err != nil {
		return credentials.OAuthCredential{}, fmt.Errorf("decode Anthropic OAuth token response: %w", err)
	}
	if token.AccessToken == "" {
		return credentials.OAuthCredential{}, errors.New("Anthropic OAuth token response is missing access_token")
	}
	refreshToken := token.RefreshToken
	if refreshToken == "" {
		refreshToken = fallbackRefresh
	}
	if refreshToken == "" {
		return credentials.OAuthCredential{}, errors.New("Anthropic OAuth token response is missing refresh_token")
	}
	if token.ExpiresIn <= 0 {
		return credentials.OAuthCredential{}, errors.New("Anthropic OAuth token response has invalid expires_in")
	}
	return credentials.OAuthCredential{
		Type:    "oauth",
		Access:  token.AccessToken,
		Refresh: refreshToken,
		Expires: time.Now().Add(time.Duration(token.ExpiresIn)*time.Second - 5*time.Minute).UnixMilli(),
	}, nil
}

// Refresh exchanges a refresh token and retains it when the provider omits a
// rotated refresh token, matching the Rust credential refresh behavior.
func (client *Client) Refresh(ctx context.Context, credential credentials.OAuthCredential) (credentials.OAuthCredential, error) {
	if credential.Refresh == "" {
		return credentials.OAuthCredential{}, errors.New("Anthropic OAuth refresh token is empty")
	}
	return client.exchange(ctx, tokenRequest{
		GrantType:    "refresh_token",
		ClientID:     clientID,
		RefreshToken: credential.Refresh,
	}, credential.Refresh)
}

func (client *Client) authorizationURL(pkce common.PKCE, state string) (string, error) {
	parsed, err := url.Parse(client.config.AuthorizationURL)
	if err != nil {
		return "", fmt.Errorf("parse Anthropic authorization URL: %w", err)
	}
	query := parsed.Query()
	query.Set("response_type", "code")
	query.Set("client_id", clientID)
	query.Set("redirect_uri", client.config.RedirectURI)
	query.Set("scope", scope)
	query.Set("state", state)
	query.Set("code_challenge", pkce.Challenge)
	query.Set("code_challenge_method", "S256")
	parsed.RawQuery = query.Encode()
	return parsed.String(), nil
}
