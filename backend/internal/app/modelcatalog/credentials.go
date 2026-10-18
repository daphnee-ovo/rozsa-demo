package modelcatalog

import (
	"context"
	"fmt"
	"net/http"
	"time"

	"rozsa/backend/internal/model/credentials"
	"rozsa/backend/internal/model/oauth/anthropic"
	"rozsa/backend/internal/model/oauth/codex"
	"rozsa/backend/internal/model/oauth/copilot"
)

type RequestCredentials struct {
	APIKey  string
	Headers map[string]string
}
type MissingCredentialsError struct{ Provider string }

func (failure *MissingCredentialsError) Error() string {
	return "No API key for provider: " + failure.Provider
}

type CredentialSource interface {
	Resolve(context.Context, string, string) (RequestCredentials, error)
}
type Auth struct {
	Path     string
	Resolver credentials.Resolver
	Refresh  credentials.RefreshFunc
}

func NewAuth(path string, client *http.Client) *Auth {
	if client == nil {
		client = &http.Client{Timeout: 20 * time.Second}
	}
	anthropicClient, codexClient, copilotClient := anthropic.NewClient(client), codex.NewClient(client), copilot.NewClient(client)
	return &Auth{Path: path, Resolver: credentials.NewResolver(), Refresh: func(ctx context.Context, provider string, current credentials.OAuthCredential) (credentials.OAuthCredential, error) {
		switch provider {
		case "anthropic":
			return anthropicClient.Refresh(ctx, current)
		case "codex-oauth":
			return codexClient.Refresh(ctx, current)
		case "github-copilot":
			return copilotClient.Refresh(ctx, current)
		default:
			return credentials.OAuthCredential{}, fmt.Errorf("OAuth refresh is unsupported for provider %q", provider)
		}
	}}
}

func (auth *Auth) Stored(ctx context.Context, provider string) (credentials.OAuthCredential, bool, error) {
	if auth.Path == "" {
		return credentials.OAuthCredential{}, false, nil
	}
	refreshed := false
	var refresh credentials.RefreshFunc
	if auth.Refresh != nil {
		refresh = func(ctx context.Context, provider string, current credentials.OAuthCredential) (credentials.OAuthCredential, error) {
			updated, err := auth.Refresh(ctx, provider, current)
			refreshed = err == nil
			return updated, err
		}
	}
	current, err := credentials.ResolveAuth(ctx, auth.Path, provider, time.Now(), refresh)
	if err == nil && current.Type == "" && current.Access != "" {
		current.Access, err = auth.Resolver.ResolveConfigValue(current.Access)
	}
	return current, refreshed, err
}

// Resolve runs per request so existing sessions observe rotation and logout.
func (auth *Auth) Resolve(ctx context.Context, provider, configured string) (RequestCredentials, error) {
	var result RequestCredentials
	var err error
	if configured != "" {
		result.APIKey, err = auth.Resolver.ResolveConfigValue(configured)
	} else {
		result.APIKey, err = auth.Resolver.ProviderAPIKey(provider)
	}
	if err != nil {
		return result, err
	}
	if result.APIKey == "" || provider == "codex-oauth" {
		stored, _, err := auth.Stored(ctx, provider)
		if err != nil {
			return result, err
		}
		if result.APIKey == "" {
			result.APIKey = stored.Access
		}
		if provider == "codex-oauth" && result.APIKey != "" {
			account := credentials.AccountID(stored)
			if account == "" {
				return result, fmt.Errorf("codex-oauth credential is missing accountId; sign in again")
			}
			result.Headers = map[string]string{"x-rozsa-account-id": account}
		}
	}
	if result.APIKey == "" {
		return result, &MissingCredentialsError{Provider: provider}
	}
	return result, nil
}
