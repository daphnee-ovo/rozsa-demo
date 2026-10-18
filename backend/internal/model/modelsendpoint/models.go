// Package modelsendpoint refreshes the Codex OAuth model catalog.
package modelsendpoint

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"time"
)

const (
	Provider        = "codex-oauth"
	ClientVersion   = "0.164.0"
	DefaultEndpoint = "https://chatgpt.com/backend-api/wham/models"
	CacheDuration   = 24 * time.Hour
)

type Client struct {
	HTTP          *http.Client
	Endpoint      string
	ClientVersion string
	Now           func() time.Time
}

func NewClient(httpClient *http.Client) *Client {
	if httpClient == nil {
		httpClient = &http.Client{Timeout: 15 * time.Second}
	}
	return &Client{HTTP: httpClient, Endpoint: DefaultEndpoint, ClientVersion: ClientVersion, Now: time.Now}
}

func (client *Client) now() time.Time {
	if client.Now != nil {
		return client.Now()
	}
	return time.Now()
}

// RefreshIfNeeded refreshes only global models.json. Project directories are deliberately untouched;
// their later registry layer continues to override the generated global catalog.
func (client *Client) RefreshIfNeeded(ctx context.Context, path, accessToken, accountID string, force bool) (bool, error) {
	if !force {
		fresh, err := client.cacheFresh(path)
		if err != nil {
			return false, err
		}
		if fresh {
			return false, nil
		}
	}
	models, err := client.fetch(ctx, accessToken, accountID)
	if err != nil {
		return false, err
	}
	config, err := readConfig(path)
	if err != nil {
		return false, err
	}
	entries := make([]map[string]any, 0, len(models))
	for _, model := range models {
		if model.SupportedInAPI == nil || !*model.SupportedInAPI {
			continue
		}
		if model.Visibility != nil && *model.Visibility != "list" {
			continue
		}
		window := 128000
		if model.ContextWindow != nil {
			window = *model.ContextWindow
		}
		input := model.InputModalities
		if input == nil {
			input = []string{}
		}
		entries = append(entries, map[string]any{"id": model.Slug, "name": model.DisplayName, "contextWindow": window, "maxTokens": window / 2, "reasoning": model.DefaultReasoningLevel != nil, "input": input, "cost": map[string]float64{"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0}})
	}
	providers := map[string]json.RawMessage{}
	if raw, found := config["providers"]; found {
		if err := json.Unmarshal(raw, &providers); err != nil {
			return false, fmt.Errorf("parse providers in global models configuration %q: %w", path, err)
		}
	}
	providerConfig, err := json.Marshal(map[string]any{"baseUrl": "https://chatgpt.com/backend-api/codex", "api": "openai-responses", "authHeader": true, "models": entries})
	if err != nil {
		return false, fmt.Errorf("encode Codex models: %w", err)
	}
	providers[Provider] = providerConfig
	encodedProviders, err := json.Marshal(providers)
	if err != nil {
		return false, fmt.Errorf("encode model providers: %w", err)
	}
	config["providers"] = encodedProviders
	timestamp, _ := json.Marshal(client.now().Unix())
	config["_last_refreshed"] = timestamp
	data, err := json.MarshalIndent(config, "", "  ")
	if err != nil {
		return false, fmt.Errorf("encode models configuration: %w", err)
	}
	if err = os.MkdirAll(filepath.Dir(path), 0700); err != nil {
		return false, fmt.Errorf("create global models directory: %w", err)
	}
	if err = os.WriteFile(path, data, 0600); err != nil {
		return false, fmt.Errorf("write global models configuration: %w", err)
	}
	return true, nil
}

func readConfig(path string) (map[string]json.RawMessage, error) {
	data, err := os.ReadFile(path)
	if errors.Is(err, os.ErrNotExist) {
		return map[string]json.RawMessage{}, nil
	}
	if err != nil {
		return nil, fmt.Errorf("read global models configuration %q: %w", path, err)
	}
	var config map[string]json.RawMessage
	if err = json.Unmarshal(data, &config); err != nil {
		return nil, fmt.Errorf("parse global models configuration %q: %w", path, err)
	}
	if config == nil {
		return nil, fmt.Errorf("parse global models configuration %q: root must be an object", path)
	}
	return config, nil
}
func (client *Client) cacheFresh(path string) (bool, error) {
	config, err := readConfig(path)
	if err != nil {
		return false, err
	}
	var timestamp int64
	if err := json.Unmarshal(config["_last_refreshed"], &timestamp); err != nil || timestamp <= 0 {
		return false, nil
	}
	now := client.now().Unix()
	age := now - timestamp
	return age >= 0 && time.Duration(age)*time.Second < CacheDuration, nil
}

type payload struct {
	Models []remoteModel `json:"models"`
}
type remoteModel struct {
	Slug                  string   `json:"slug"`
	DisplayName           string   `json:"display_name"`
	ContextWindow         *int     `json:"context_window"`
	SupportedInAPI        *bool    `json:"supported_in_api"`
	Visibility            *string  `json:"visibility"`
	DefaultReasoningLevel *string  `json:"default_reasoning_level"`
	InputModalities       []string `json:"input_modalities"`
}

func (client *Client) fetch(ctx context.Context, accessToken, accountID string) ([]remoteModel, error) {
	endpoint := client.Endpoint
	if endpoint == "" {
		endpoint = DefaultEndpoint
	}
	parsed, err := url.Parse(endpoint)
	if err != nil {
		return nil, fmt.Errorf("parse Codex models endpoint: %w", err)
	}
	query := parsed.Query()
	version := client.ClientVersion
	if version == "" {
		version = ClientVersion
	}
	query.Set("client_version", version)
	parsed.RawQuery = query.Encode()
	request, err := http.NewRequestWithContext(ctx, http.MethodGet, parsed.String(), nil)
	if err != nil {
		return nil, fmt.Errorf("create Codex models request: %w", err)
	}
	request.Header.Set("Authorization", "Bearer "+accessToken)
	request.Header.Set("ChatGPT-Account-ID", accountID)
	httpClient := client.HTTP
	if httpClient == nil {
		httpClient = http.DefaultClient
	}
	response, err := httpClient.Do(request)
	if err != nil {
		return nil, fmt.Errorf("Codex models request failed: %w", err)
	}
	defer response.Body.Close()
	body, err := io.ReadAll(io.LimitReader(response.Body, 4<<20))
	if err != nil {
		return nil, fmt.Errorf("read Codex models response: %w", err)
	}
	if response.StatusCode < 200 || response.StatusCode >= 300 {
		return nil, fmt.Errorf("Codex models request returned HTTP %d: %s", response.StatusCode, string(body))
	}
	var result payload
	if err = json.Unmarshal(body, &result); err != nil {
		return nil, fmt.Errorf("decode Codex models response: %w", err)
	}
	return result.Models, nil
}
