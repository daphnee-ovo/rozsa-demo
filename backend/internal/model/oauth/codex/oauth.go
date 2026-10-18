// Package codex implements the OpenAI Codex OAuth flow.
package codex

import (
	"context"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net"
	"net/http"
	"net/url"
	"strings"
	"time"

	"rozsa/backend/internal/model/credentials"
	"rozsa/backend/internal/model/oauth/common"
)

const (
	ClientID               = "app_EMoamEEZ73f0CkXaXp7hrann"
	DefaultAuthorizeURL    = "https://auth.openai.com/oauth/authorize"
	DefaultTokenURL        = "https://auth.openai.com/oauth/token"
	DefaultRedirectURI     = "http://localhost:1455/auth/callback"
	DefaultCallbackAddress = "127.0.0.1:1455"
	Scope                  = "openid profile email offline_access api.connectors.read api.connectors.invoke"
)

type Client struct {
	HTTP            *http.Client
	AuthorizeURL    string
	TokenURL        string
	RedirectURI     string
	CallbackAddress string
	ClientID        string
	Now             func() time.Time
}

func NewClient(httpClient *http.Client) *Client {
	if httpClient == nil {
		httpClient = &http.Client{Timeout: 30 * time.Second}
	}
	return &Client{HTTP: httpClient, AuthorizeURL: DefaultAuthorizeURL, TokenURL: DefaultTokenURL, RedirectURI: DefaultRedirectURI, CallbackAddress: DefaultCallbackAddress, ClientID: ClientID, Now: time.Now}
}

type Event struct {
	Kind    string
	URL     string
	Message string
}

// Login emits the authorization URL and accepts an optional manually pasted callback value.
// The callback listener binds only to loopback and stops when ctx is cancelled.
func (client *Client) Login(ctx context.Context, input <-chan string, events chan<- Event) (credentials.OAuthCredential, error) {
	pkce, err := common.GeneratePKCE()
	if err != nil {
		return credentials.OAuthCredential{}, err
	}
	state, err := common.GenerateState()
	if err != nil {
		return credentials.OAuthCredential{}, err
	}
	authorizeURL := client.authorizationURL(pkce.Challenge, state)
	if events != nil {
		select {
		case events <- Event{Kind: "authorization_url", URL: authorizeURL, Message: "Complete sign-in in your browser, then return here."}:
		case <-ctx.Done():
			return credentials.OAuthCredential{}, ctx.Err()
		}
	}
	callbackCtx, cancelCallback := context.WithCancel(ctx)
	defer cancelCallback()
	type callbackResult struct {
		code string
		err  error
	}
	callback := make(chan callbackResult, 1)
	go func() {
		code, waitErr := waitCallback(callbackCtx, client.CallbackAddress, state)
		callback <- callbackResult{code: code, err: waitErr}
	}()
	var code string
	select {
	case result := <-callback:
		if result.err != nil {
			return credentials.OAuthCredential{}, result.err
		}
		code = result.code
	case pasted, ok := <-input:
		if !ok {
			return credentials.OAuthCredential{}, errors.New("Codex OAuth input channel closed")
		}
		parsed, parseErr := common.ParseAuthorizationInput(pasted, state)
		if parseErr != nil {
			return credentials.OAuthCredential{}, parseErr
		}
		code = parsed.Code
	case <-ctx.Done():
		return credentials.OAuthCredential{}, ctx.Err()
	}
	return client.exchange(ctx, url.Values{"grant_type": {"authorization_code"}, "client_id": {client.clientID()}, "code": {code}, "redirect_uri": {client.redirectURI()}, "code_verifier": {pkce.Verifier}}, nil)
}

func (client *Client) Refresh(ctx context.Context, current credentials.OAuthCredential) (credentials.OAuthCredential, error) {
	form := url.Values{"grant_type": {"refresh_token"}, "refresh_token": {current.Refresh}, "client_id": {client.clientID()}}
	return client.exchange(ctx, form, &current)
}

func (client *Client) authorizationURL(challenge, state string) string {
	query := url.Values{"response_type": {"code"}, "client_id": {client.clientID()}, "redirect_uri": {client.redirectURI()}, "scope": {Scope}, "state": {state}, "code_challenge": {challenge}, "code_challenge_method": {"S256"}, "id_token_add_organizations": {"true"}, "codex_cli_simplified_flow": {"true"}, "originator": {"pi"}}
	return strings.TrimRight(client.authorizeURL(), "?") + "?" + query.Encode()
}
func (client *Client) clientID() string {
	if client.ClientID != "" {
		return client.ClientID
	}
	return ClientID
}
func (client *Client) tokenURL() string {
	if client.TokenURL != "" {
		return client.TokenURL
	}
	return DefaultTokenURL
}
func (client *Client) authorizeURL() string {
	if client.AuthorizeURL != "" {
		return client.AuthorizeURL
	}
	return DefaultAuthorizeURL
}
func (client *Client) redirectURI() string {
	if client.RedirectURI != "" {
		return client.RedirectURI
	}
	return DefaultRedirectURI
}
func (client *Client) now() time.Time {
	if client.Now != nil {
		return client.Now()
	}
	return time.Now()
}

type tokenResponse struct {
	AccessToken  string `json:"access_token"`
	RefreshToken string `json:"refresh_token"`
	ExpiresIn    int64  `json:"expires_in"`
	IDToken      string `json:"id_token"`
}

func (client *Client) exchange(ctx context.Context, form url.Values, previous *credentials.OAuthCredential) (credentials.OAuthCredential, error) {
	request, err := http.NewRequestWithContext(ctx, http.MethodPost, client.tokenURL(), strings.NewReader(form.Encode()))
	if err != nil {
		return credentials.OAuthCredential{}, fmt.Errorf("create Codex token request: %w", err)
	}
	request.Header.Set("Content-Type", "application/x-www-form-urlencoded")
	httpClient := client.HTTP
	if httpClient == nil {
		httpClient = http.DefaultClient
	}
	response, err := httpClient.Do(request)
	if err != nil {
		return credentials.OAuthCredential{}, fmt.Errorf("Codex token request failed: %w", err)
	}
	defer response.Body.Close()
	body, err := io.ReadAll(io.LimitReader(response.Body, 1<<20))
	if err != nil {
		return credentials.OAuthCredential{}, fmt.Errorf("read Codex token response: %w", err)
	}
	if response.StatusCode < 200 || response.StatusCode >= 300 {
		return credentials.OAuthCredential{}, fmt.Errorf("Codex token request returned HTTP %d: %s", response.StatusCode, strings.TrimSpace(string(body)))
	}
	var token tokenResponse
	if err = json.Unmarshal(body, &token); err != nil {
		return credentials.OAuthCredential{}, fmt.Errorf("decode Codex token response: %w", err)
	}
	if token.AccessToken == "" {
		return credentials.OAuthCredential{}, errors.New("Codex token response is missing access_token")
	}
	credential := credentials.OAuthCredential{Type: "oauth", Access: token.AccessToken, Refresh: token.RefreshToken, Expires: client.now().UnixMilli() + token.ExpiresIn*1000, Extra: map[string]any{}}
	if previous != nil {
		credential = *previous
		credential.Access = token.AccessToken
		credential.Expires = client.now().UnixMilli() + token.ExpiresIn*1000
		if token.RefreshToken != "" {
			credential.Refresh = token.RefreshToken
		}
	}
	if credential.Refresh == "" {
		return credentials.OAuthCredential{}, errors.New("Codex token response is missing refresh_token")
	}
	if credential.Extra == nil {
		credential.Extra = map[string]any{}
	}
	if token.IDToken != "" {
		credential.Extra["idToken"] = token.IDToken
	}
	accountID := AccountID(token.IDToken)
	if accountID == "" {
		accountID = AccountID(token.AccessToken)
	}
	if accountID != "" {
		credential.Extra["accountId"] = accountID
	}
	return credential, nil
}

// AccountID reads the Codex account claim from an unverified JWT, matching the Rust credential behavior.
func AccountID(token string) string {
	parts := strings.Split(token, ".")
	if len(parts) != 3 {
		return ""
	}
	payload, err := base64.RawURLEncoding.DecodeString(parts[1])
	if err != nil {
		return ""
	}
	var claims map[string]any
	if json.Unmarshal(payload, &claims) != nil {
		return ""
	}
	auth, ok := claims["https://api.openai.com/auth"].(map[string]any)
	if !ok {
		return ""
	}
	account, _ := auth["chatgpt_account_id"].(string)
	return account
}

// ListenAddress validates the callback bind target before the flow starts.
func ListenAddress(address string) error {
	host, _, err := net.SplitHostPort(address)
	if err != nil {
		return err
	}
	ip := net.ParseIP(host)
	if ip == nil || !ip.IsLoopback() {
		return errors.New("Codex OAuth callback must bind to loopback")
	}
	return nil
}

func waitCallback(ctx context.Context, address, state string) (string, error) {
	if err := ListenAddress(address); err != nil {
		return "", err
	}
	listener, err := net.Listen("tcp", address)
	if err != nil {
		return "", fmt.Errorf("bind Codex OAuth callback listener: %w", err)
	}
	result := make(chan string, 1)
	failure := make(chan error, 1)
	mux := http.NewServeMux()
	mux.HandleFunc("/auth/callback", func(writer http.ResponseWriter, request *http.Request) {
		if request.Method != http.MethodGet {
			http.Error(writer, "method not allowed", http.StatusMethodNotAllowed)
			return
		}
		parsed, parseErr := common.ParseAuthorizationInput(request.URL.RawQuery, state)
		if parseErr != nil {
			http.Error(writer, "OAuth callback rejected", http.StatusBadRequest)
			select {
			case failure <- parseErr:
			default:
			}
			return
		}
		writer.Header().Set("Content-Type", "text/html; charset=utf-8")
		_, _ = writer.Write([]byte("<!doctype html><title>Login complete</title><p>Login complete. You can close this tab.</p>"))
		select {
		case result <- parsed.Code:
		default:
		}
	})
	server := &http.Server{Handler: mux, ReadHeaderTimeout: 5 * time.Second}
	go func() {
		if serveErr := server.Serve(listener); serveErr != nil && !errors.Is(serveErr, http.ErrServerClosed) {
			select {
			case failure <- serveErr:
			default:
			}
		}
	}()
	defer server.Close()
	select {
	case code := <-result:
		return code, nil
	case callbackErr := <-failure:
		return "", callbackErr
	case <-ctx.Done():
		return "", ctx.Err()
	}
}
