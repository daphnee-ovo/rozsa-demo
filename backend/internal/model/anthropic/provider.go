package anthropic

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"os"
	"strings"
	"time"

	"rozsa/backend/internal/model/types"
)

// Provider accepts an HTTP client and an environment resolver for isolated local
// tests. Credential file/OAuth refresh ownership remains in the app layer.
// Configure these fields before use; a Provider can then serve concurrent calls.
type Provider struct {
	Client    *http.Client
	LookupEnv func(string) (string, bool)
}

func NewProvider(client *http.Client) *Provider {
	if client == nil {
		client = &http.Client{Timeout: 300 * time.Second}
	}
	return &Provider{Client: client, LookupEnv: os.LookupEnv}
}

func (*Provider) API() types.API { return types.APIAnthropicMessages }

// StreamSimple runs synchronously and emits exactly one terminal event when an
// emitter is provided. The caller owns scheduling and must keep emit responsive
// (including after cancellation). Snapshots never share mutable block storage.
func (provider *Provider) StreamSimple(ctx context.Context, model types.Model, input types.Context, options types.SimpleStreamOptions, emit func(types.StreamEvent)) (err error) {
	if emit == nil {
		return errors.New("Anthropic stream requires an event emitter")
	}
	if options.Base.Timeout > 0 {
		var cancel context.CancelFunc
		ctx, cancel = context.WithTimeout(ctx, options.Base.Timeout)
		defer cancel()
	}
	output := types.AssistantMessage{API: types.APIAnthropicMessages, Provider: model.Provider, Model: model.ID, Timestamp: time.Now().UnixMilli(), Content: []types.ContentBlock{}}
	defer func() {
		if err == nil {
			err = ctx.Err()
		}
		if err != nil {
			reason := types.Error
			if errors.Is(err, context.Canceled) {
				reason = types.Aborted
			}
			// Rust recreates an empty output on failure, rather than persisting an
			// incomplete tool call. Already emitted partial events remain visible.
			failure := types.AssistantMessage{API: output.API, Provider: output.Provider, Model: output.Model, Timestamp: output.Timestamp, Content: []types.ContentBlock{}, StopReason: reason}
			message := err.Error()
			failure.ErrorMessage = &message
			emit(types.StreamError{Reason: reason, Error: failure})
		} else {
			emit(types.Done{Reason: output.StopReason, Message: snapshot(output)})
		}
	}()
	if provider.Client == nil || provider.LookupEnv == nil {
		return errors.New("Anthropic provider requires HTTP client and environment resolver")
	}
	if err := ctx.Err(); err != nil {
		return err
	}
	key := options.Base.APIKey
	if key == "" {
		variables := []string{}
		switch model.Provider {
		case "anthropic":
			variables = []string{"ANTHROPIC_OAUTH_TOKEN", "ANTHROPIC_API_KEY"}
		case "cloudflare-ai-gateway":
			variables = []string{"CLOUDFLARE_API_KEY"}
		}
		for _, variable := range variables {
			if value, ok := provider.LookupEnv(variable); ok {
				key = value
				break
			}
		}
	}
	if key == "" {
		return fmt.Errorf("No API key for provider: %s", model.Provider)
	}
	oauth := strings.Contains(key, "sk-ant-oat")
	compat, err := resolveCompatibility(model)
	if err != nil {
		return err
	}
	if options.Base.CacheRetention == types.CacheDefault {
		if retention, _ := provider.LookupEnv("ROZSA_CACHE_RETENTION"); retention == "long" {
			options.Base.CacheRetention = types.CacheLong
		}
	}
	payload, err := buildPayload(model, input, options, oauth, compat)
	if err != nil {
		return err
	}
	endpoint, err := resolveEndpoint(model.BaseURL)
	if err != nil {
		return err
	}
	request, err := http.NewRequestWithContext(ctx, http.MethodPost, endpoint, bytes.NewReader(payload))
	if err != nil {
		return fmt.Errorf("Anthropic request: %w", err)
	}
	request.Header = buildHeaders(model, options, oauth, compat, input)
	switch {
	case oauth || model.Provider == "github-copilot":
		request.Header.Set("authorization", "Bearer "+key)
	case model.Provider == "cloudflare-ai-gateway":
		request.Header.Set("cf-aig-authorization", "Bearer "+key)
	default:
		request.Header.Set("x-api-key", key)
	}
	// Copy the client so an explicit timeout never changes concurrent requests.
	client := *provider.Client
	if options.Base.Timeout > 0 {
		client.Timeout = 0
	} // context owns the explicit override
	response, err := client.Do(request)
	if err != nil {
		return fmt.Errorf("HTTP request failed: %w", err)
	}
	defer response.Body.Close()
	if response.StatusCode < 200 || response.StatusCode >= 300 {
		body, readErr := io.ReadAll(response.Body)
		if readErr != nil {
			return fmt.Errorf("Provider HTTP error (%d), reading body: %w", response.StatusCode, readErr)
		}
		return &HTTPError{StatusCode: response.StatusCode, Body: string(body)}
	}
	emit(types.Start{Partial: snapshot(output)})
	return consumeStream(ctx, response.Body, model, &output, oauth, input.Tools, emit)
}

type HTTPError struct {
	StatusCode int
	Body       string
}

func (err *HTTPError) Error() string {
	return fmt.Sprintf("Provider HTTP error (%d): %s", err.StatusCode, err.Body)
}

func resolveEndpoint(base string) (string, error) {
	base = strings.TrimRight(strings.TrimSpace(base), "/")
	if base == "" {
		base = "https://api.anthropic.com/v1/messages"
	}
	if !strings.HasSuffix(base, "/messages") {
		if strings.Contains(base, "/v1") {
			base += "/messages"
		} else {
			base += "/v1/messages"
		}
	}
	parsed, err := url.Parse(base)
	if err != nil || parsed.Host == "" || parsed.Scheme != "https" && parsed.Scheme != "http" {
		return "", errors.New("invalid Anthropic endpoint: expected an http(s) URL")
	}
	return base, nil
}

func snapshot(output types.AssistantMessage) types.AssistantMessage {
	cloneString := func(value *string) *string {
		if value == nil {
			return nil
		}
		copy := *value
		return &copy
	}
	output.ResponseID = cloneString(output.ResponseID)
	output.ResponseModel = cloneString(output.ResponseModel)
	output.ErrorMessage = cloneString(output.ErrorMessage)
	content := make([]types.ContentBlock, len(output.Content))
	for i, block := range output.Content {
		switch block := block.(type) {
		case types.TextBlock:
			block.Signature = cloneString(block.Signature)
			content[i] = block
		case types.ThinkingBlock:
			block.Signature = cloneString(block.Signature)
			content[i] = block
		case types.ToolCallBlock:
			block.Arguments = append([]byte(nil), block.Arguments...)
			content[i] = block
		default:
			content[i] = block
		}
	}
	output.Content = content
	return output
}
