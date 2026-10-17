package responses

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"strings"
	"time"

	"rozsa/backend/internal/model/types"
)

type Provider struct{ Client *http.Client }

func NewProvider(client *http.Client) *Provider {
	if client == nil {
		client = &http.Client{Timeout: 300 * time.Second}
	}
	return &Provider{Client: client}
}

func (*Provider) API() types.API { return types.APIOpenAIResponses }

func (provider *Provider) Stream(ctx context.Context, model types.Model, input types.Context, options types.StreamOptions, emit func(types.StreamEvent)) error {
	return provider.StreamSimple(ctx, model, input, types.SimpleStreamOptions{Base: options}, emit)
}

// StreamSimple sends one Responses API request and emits normalized events.
// It intentionally shares the Go model event contract with Completions while
// keeping request and response semantics protocol-specific.
func (provider *Provider) StreamSimple(ctx context.Context, model types.Model, input types.Context, options types.SimpleStreamOptions, emit func(types.StreamEvent)) (err error) {
	if emit == nil {
		return errors.New("Responses stream requires an event emitter")
	}
	output := types.AssistantMessage{API: types.APIOpenAIResponses, Provider: model.Provider, Model: model.ID, Timestamp: time.Now().UnixMilli(), Content: []types.ContentBlock{}}
	defer func() {
		if err != nil {
			reason := types.Error
			if errors.Is(err, context.Canceled) {
				reason = types.Aborted
			}
			failure := types.AssistantMessage{API: output.API, Provider: output.Provider, Model: output.Model, Timestamp: output.Timestamp, Content: []types.ContentBlock{}, StopReason: reason}
			message := err.Error()
			failure.ErrorMessage = &message
			emit(types.StreamError{Reason: reason, Error: failure})
			return
		}
		emit(types.Done{Reason: output.StopReason, Message: snapshot(output)})
	}()
	if provider == nil || provider.Client == nil {
		return errors.New("Responses provider requires HTTP client")
	}
	if err = ctx.Err(); err != nil {
		return err
	}
	if options.Base.Timeout > 0 {
		var cancel context.CancelFunc
		ctx, cancel = context.WithTimeout(ctx, options.Base.Timeout)
		defer cancel()
	}
	endpoint, err := resolveEndpoint(model.BaseURL)
	if err != nil {
		return err
	}
	requestBody, err := buildRequest(model, input, options)
	if err != nil {
		return err
	}
	body, err := json.Marshal(requestBody)
	if err != nil {
		return fmt.Errorf("encode Responses request: %w", err)
	}
	request, err := http.NewRequestWithContext(ctx, http.MethodPost, endpoint, bytes.NewReader(body))
	if err != nil {
		return fmt.Errorf("Responses request: %w", err)
	}
	request.Header.Set("Accept", "text/event-stream")
	request.Header.Set("Content-Type", "application/json")
	for key, value := range model.Headers {
		request.Header.Set(key, value)
	}
	for key, value := range options.Base.Headers {
		request.Header.Set(key, value)
	}
	if options.Base.APIKey != "" && request.Header.Get("Authorization") == "" {
		request.Header.Set("Authorization", "Bearer "+options.Base.APIKey)
	}
	if options.Base.SessionID != nil {
		request.Header.Set("session_id", *options.Base.SessionID)
		request.Header.Set("x-client-request-id", *options.Base.SessionID)
	}
	if accountID := options.Base.Headers["x-rozsa-account-id"]; accountID != "" {
		request.Header.Set("ChatGPT-Account-ID", accountID)
	}
	client := *provider.Client
	if options.Base.Timeout > 0 {
		client.Timeout = 0
	}
	response, err := client.Do(request)
	if err != nil {
		return fmt.Errorf("HTTP request failed: %w", err)
	}
	defer response.Body.Close()
	if response.StatusCode < 200 || response.StatusCode >= 300 {
		responseBody, readErr := io.ReadAll(response.Body)
		if readErr != nil {
			return fmt.Errorf("provider HTTP error (%d), reading body: %w", response.StatusCode, readErr)
		}
		return fmt.Errorf("Provider HTTP error (%d): %s", response.StatusCode, responseBody)
	}
	emit(types.Start{Partial: snapshot(output)})
	return consume(ctx, response.Body, model, &output, emit)
}

func resolveEndpoint(base string) (string, error) {
	base = strings.TrimRight(strings.TrimSpace(base), "/")
	if base == "" {
		base = "https://api.openai.com/v1"
	}
	if !strings.HasSuffix(base, "/responses") {
		base += "/responses"
	}
	parsed, err := url.Parse(base)
	if err != nil || parsed.Host == "" || parsed.Scheme != "http" && parsed.Scheme != "https" {
		return "", errors.New("invalid Responses endpoint: expected an http(s) URL")
	}
	return parsed.String(), nil
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
