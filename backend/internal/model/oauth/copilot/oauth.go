// Package copilot implements GitHub Copilot OAuth device authorization.
package copilot

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"strings"
	"time"

	"rozsa/backend/internal/model/credentials"
	"rozsa/backend/internal/model/oauth/common"
)

const (
	defaultDomain     = "github.com"
	defaultClientID   = "Iv1.b507a08c87ecfe98"
	providerUserAgent = "rozsa/1.0"
)

var knownModels = []string{"claude-3.5-sonnet", "claude-sonnet-4", "claude-3.7-sonnet", "o4-mini", "o3", "o3-mini", "gpt-4o", "gpt-4.1", "gemini-2.5-pro"}

type EventKind string

const (
	EventPrompt     EventKind = "prompt"
	EventDeviceCode EventKind = "device_code"
	EventWaiting    EventKind = "waiting"
	EventProgress   EventKind = "progress"
)

// Event contains only information safe to display to a user. Device and access tokens are never included.
type Event struct {
	Kind            EventKind
	Message         string
	UserCode        string
	VerificationURI string
}

type EventHandler func(Event) error

type LoginInput struct{ EnterpriseURL string }

// Client holds the HTTP transport and provider endpoints so integrations can test against local fixtures.
type Client struct {
	HTTPClient    *http.Client
	GitHubBaseURL string
	APIBaseURL    string
	ClientID      string
	UserAgent     string
	PollInterval  time.Duration
	Wait          common.WaitFunc
}

func NewClient(httpClient *http.Client) *Client {
	if httpClient == nil {
		httpClient = http.DefaultClient
	}
	return &Client{HTTPClient: httpClient, GitHubBaseURL: "https://github.com", APIBaseURL: "https://api.github.com", ClientID: defaultClientID, UserAgent: providerUserAgent, PollInterval: 5 * time.Second}
}

func (client *Client) Login(ctx context.Context, input LoginInput, emit EventHandler) (credentials.OAuthCredential, error) {
	if err := ctx.Err(); err != nil {
		return credentials.OAuthCredential{}, err
	}
	domain, err := normalizeDomain(input.EnterpriseURL)
	if err != nil {
		return credentials.OAuthCredential{}, fmt.Errorf("invalid GitHub Enterprise URL: %w", err)
	}
	githubURL, apiURL := client.endpoints(domain)
	if err := client.emit(emit, Event{Kind: EventPrompt, Message: "Authorize GitHub Copilot with the device code below."}); err != nil {
		return credentials.OAuthCredential{}, err
	}
	device, err := client.requestDeviceCode(ctx, githubURL)
	if err != nil {
		return credentials.OAuthCredential{}, err
	}
	if err := client.emit(emit, Event{Kind: EventDeviceCode, UserCode: device.UserCode, VerificationURI: device.VerificationURI}); err != nil {
		return credentials.OAuthCredential{}, err
	}
	if err := client.emit(emit, Event{Kind: EventWaiting, Message: "Waiting for authorization..."}); err != nil {
		return credentials.OAuthCredential{}, err
	}
	interval := client.PollInterval
	if device.Interval > 0 {
		interval = time.Duration(device.Interval) * time.Second
	}
	if interval <= 0 {
		interval = 5 * time.Second
	}
	githubToken, err := common.PollDeviceCode(ctx, interval, time.Duration(device.ExpiresIn)*time.Second, func(pollContext context.Context) common.DeviceResult {
		return client.pollGitHubToken(pollContext, githubURL, device.DeviceCode)
	}, client.Wait)
	if err != nil {
		return credentials.OAuthCredential{}, err
	}
	if err := client.emit(emit, Event{Kind: EventProgress, Message: "Exchanging token for Copilot access..."}); err != nil {
		return credentials.OAuthCredential{}, err
	}
	token, err := client.copilotToken(ctx, apiURL, githubToken, false)
	if err != nil {
		return credentials.OAuthCredential{}, err
	}
	client.enableKnownModels(ctx, apiURL, githubToken)
	extra := map[string]any{}
	if domain != defaultDomain {
		extra["enterpriseUrl"] = domain
	}
	return credentials.OAuthCredential{Type: "oauth", Access: token.Token, Refresh: githubToken, Expires: token.ExpiresAt*1000 - 5*60*1000, Extra: extra}, nil
}

// Refresh exchanges the stored GitHub access token for a fresh Copilot token.
func (client *Client) Refresh(ctx context.Context, credential credentials.OAuthCredential) (credentials.OAuthCredential, error) {
	if credential.Refresh == "" {
		return credentials.OAuthCredential{}, errors.New("GitHub Copilot refresh token is empty")
	}
	domain := defaultDomain
	if value, ok := credential.Extra["enterpriseUrl"].(string); ok && value != "" {
		var err error
		domain, err = normalizeDomain(value)
		if err != nil {
			return credentials.OAuthCredential{}, fmt.Errorf("invalid stored GitHub Enterprise URL: %w", err)
		}
	}
	_, apiURL := client.endpoints(domain)
	token, err := client.copilotToken(ctx, apiURL, credential.Refresh, true)
	if err != nil {
		return credentials.OAuthCredential{}, fmt.Errorf("GitHub Copilot token refresh failed: %w", err)
	}
	credential.Type, credential.Access = "oauth", token.Token
	credential.Expires = token.ExpiresAt*1000 - 5*60*1000
	return credential, nil
}

func (client *Client) emit(handler EventHandler, event Event) error {
	if handler == nil {
		return nil
	}
	if err := handler(event); err != nil {
		return fmt.Errorf("deliver GitHub Copilot %s event: %w", event.Kind, err)
	}
	return nil
}

func (client *Client) endpoints(domain string) (string, string) {
	githubURL, apiURL := client.GitHubBaseURL, client.APIBaseURL
	if domain != defaultDomain {
		githubURL, apiURL = "https://"+domain, "https://api."+domain
	}
	if client.GitHubBaseURL != "" && client.GitHubBaseURL != "https://github.com" {
		githubURL = client.GitHubBaseURL
	}
	if client.APIBaseURL != "" && client.APIBaseURL != "https://api.github.com" {
		apiURL = client.APIBaseURL
	}
	return strings.TrimRight(githubURL, "/"), strings.TrimRight(apiURL, "/")
}

type deviceCodeResponse struct {
	DeviceCode      string `json:"device_code"`
	UserCode        string `json:"user_code"`
	VerificationURI string `json:"verification_uri"`
	Interval        int64  `json:"interval"`
	ExpiresIn       int64  `json:"expires_in"`
}
type tokenResponse struct {
	AccessToken string `json:"access_token"`
	Error       string `json:"error"`
}
type copilotTokenResponse struct {
	Token     string `json:"token"`
	ExpiresAt int64  `json:"expires_at"`
}

func (client *Client) requestDeviceCode(ctx context.Context, base string) (deviceCodeResponse, error) {
	values := url.Values{"client_id": {client.clientID()}, "scope": {"read:user"}}
	request, err := http.NewRequestWithContext(ctx, http.MethodPost, base+"/login/device/code", strings.NewReader(values.Encode()))
	if err != nil {
		return deviceCodeResponse{}, fmt.Errorf("create GitHub device code request: %w", err)
	}
	request.Header.Set("Accept", "application/json")
	request.Header.Set("Content-Type", "application/x-www-form-urlencoded")
	response, err := client.http().Do(request)
	if err != nil {
		return deviceCodeResponse{}, fmt.Errorf("GitHub device code request failed: %w", err)
	}
	defer response.Body.Close()
	if response.StatusCode < 200 || response.StatusCode >= 300 {
		return deviceCodeResponse{}, fmt.Errorf("GitHub device code request failed with HTTP %d", response.StatusCode)
	}
	var device deviceCodeResponse
	if err := json.NewDecoder(response.Body).Decode(&device); err != nil {
		return deviceCodeResponse{}, fmt.Errorf("decode GitHub device code response: %w", err)
	}
	if device.DeviceCode == "" || device.UserCode == "" || device.VerificationURI == "" || device.ExpiresIn <= 0 {
		return deviceCodeResponse{}, errors.New("GitHub device code response is incomplete")
	}
	return device, nil
}

func (client *Client) pollGitHubToken(ctx context.Context, base, deviceCode string) common.DeviceResult {
	values := url.Values{"client_id": {client.clientID()}, "device_code": {deviceCode}, "grant_type": {"urn:ietf:params:oauth:grant-type:device_code"}}
	request, err := http.NewRequestWithContext(ctx, http.MethodPost, base+"/login/oauth/access_token", strings.NewReader(values.Encode()))
	if err != nil {
		return common.DeviceResult{Kind: common.DeviceFailure, Err: fmt.Errorf("create GitHub device poll request: %w", err)}
	}
	request.Header.Set("Accept", "application/json")
	request.Header.Set("Content-Type", "application/x-www-form-urlencoded")
	response, err := client.http().Do(request)
	if err != nil {
		return common.DeviceResult{Kind: common.DeviceFailure, Err: fmt.Errorf("GitHub device poll request failed: %w", err)}
	}
	defer response.Body.Close()
	var body tokenResponse
	if err := json.NewDecoder(response.Body).Decode(&body); err != nil {
		return common.DeviceResult{Kind: common.DeviceFailure, Err: errors.New("GitHub device poll returned invalid JSON")}
	}
	switch body.Error {
	case "authorization_pending":
		return common.DeviceResult{Kind: common.DevicePending}
	case "slow_down":
		return common.DeviceResult{Kind: common.DeviceSlowDown}
	case "":
		if body.AccessToken == "" {
			return common.DeviceResult{Kind: common.DeviceFailure, Err: errors.New("GitHub device poll returned no access token")}
		}
		return common.DeviceResult{Kind: common.DeviceComplete, AccessToken: body.AccessToken}
	default:
		return common.DeviceResult{Kind: common.DeviceFailure, Err: fmt.Errorf("GitHub device authorization failed: %s", body.Error)}
	}
}

func (client *Client) copilotToken(ctx context.Context, base, githubToken string, refresh bool) (copilotTokenResponse, error) {
	request, err := http.NewRequestWithContext(ctx, http.MethodGet, base+"/copilot_internal/v2/token", nil)
	if err != nil {
		return copilotTokenResponse{}, fmt.Errorf("create Copilot token request: %w", err)
	}
	request.Header.Set("Accept", "application/json")
	request.Header.Set("Authorization", "Bearer "+githubToken)
	request.Header.Set("User-Agent", client.userAgent())
	if refresh {
		request.Header.Set("User-Agent", "GitHubCopilotChat/0.35.0")
		request.Header.Set("Editor-Version", "vscode/1.107.0")
		request.Header.Set("Editor-Plugin-Version", "copilot-chat/0.35.0")
		request.Header.Set("Copilot-Integration-Id", "vscode-chat")
	}
	response, err := client.http().Do(request)
	if err != nil {
		return copilotTokenResponse{}, fmt.Errorf("Copilot token request failed: %w", err)
	}
	defer response.Body.Close()
	if response.StatusCode < 200 || response.StatusCode >= 300 {
		return copilotTokenResponse{}, fmt.Errorf("Copilot token request failed with HTTP %d", response.StatusCode)
	}
	var token copilotTokenResponse
	if err := json.NewDecoder(response.Body).Decode(&token); err != nil {
		return copilotTokenResponse{}, fmt.Errorf("decode Copilot token response: %w", err)
	}
	if token.Token == "" || token.ExpiresAt <= 0 {
		return copilotTokenResponse{}, errors.New("Copilot token response is incomplete")
	}
	return token, nil
}

func (client *Client) enableKnownModels(ctx context.Context, base, githubToken string) {
	for _, model := range knownModels {
		request, err := http.NewRequestWithContext(ctx, http.MethodPost, base+"/models/"+model+"/policy", strings.NewReader(`{"state":"enabled"}`))
		if err != nil {
			continue
		}
		request.Header.Set("Authorization", "Bearer "+githubToken)
		request.Header.Set("User-Agent", client.userAgent())
		request.Header.Set("Content-Type", "application/json")
		response, err := client.http().Do(request)
		if err != nil {
			continue
		}
		_, _ = io.Copy(io.Discard, response.Body)
		_ = response.Body.Close()
	}
}

func (client *Client) http() *http.Client {
	if client.HTTPClient != nil {
		return client.HTTPClient
	}
	return http.DefaultClient
}
func (client *Client) clientID() string {
	if client.ClientID != "" {
		return client.ClientID
	}
	return defaultClientID
}
func (client *Client) userAgent() string {
	if client.UserAgent != "" {
		return client.UserAgent
	}
	return providerUserAgent
}

func normalizeDomain(raw string) (string, error) {
	value := strings.TrimSpace(raw)
	if value == "" {
		return defaultDomain, nil
	}
	if !strings.Contains(value, "://") {
		value = "https://" + value
	}
	parsed, err := url.Parse(value)
	if err != nil || parsed.Hostname() == "" || (parsed.Scheme != "https" && parsed.Scheme != "http") {
		return "", errors.New("expected a GitHub host or HTTP(S) URL")
	}
	if parsed.User != nil || parsed.RawQuery != "" || parsed.Fragment != "" {
		return "", errors.New("URL credentials, query, and fragment are not supported")
	}
	return strings.ToLower(parsed.Host), nil
}
