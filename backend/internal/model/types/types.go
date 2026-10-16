// Package types contains model metadata shared by model providers and the app.
package types

import "encoding/json"

type API string

const (
	APIAnthropicMessages     API = "anthropic-messages"
	APIOpenAICompletions     API = "openai-completions"
	APIOpenAIResponses       API = "openai-responses"
	APIBedrockConverseStream API = "bedrock-converse-stream"
	APIGoogleGenerativeAI    API = "google-generative-ai"
	APIGoogleVertex          API = "google-vertex"
	APIMistralConversations  API = "mistral-conversations"
)

type Provider struct {
	ID   string `json:"id"`
	Name string `json:"name"`
}
type Cost struct {
	Input      float64 `json:"input"`
	Output     float64 `json:"output"`
	CacheRead  float64 `json:"cacheRead"`
	CacheWrite float64 `json:"cacheWrite"`
}
type ThinkingEffort string

const (
	ThinkingOff    ThinkingEffort = "off"
	ThinkingLow    ThinkingEffort = "low"
	ThinkingMedium ThinkingEffort = "medium"
	ThinkingHigh   ThinkingEffort = "high"
	ThinkingXHigh  ThinkingEffort = "xhigh"
	ThinkingMax    ThinkingEffort = "max"
)

// Model preserves a null thinking-map entry as an explicitly unsupported effort.
type Model struct {
	ID                string                     `json:"id"`
	Name              string                     `json:"name"`
	API               API                        `json:"api"`
	Provider          string                     `json:"provider"`
	BaseURL           string                     `json:"baseUrl"`
	Reasoning         bool                       `json:"reasoning"`
	ThinkingEffortMap map[ThinkingEffort]*string `json:"thinkingEffortMap,omitempty"`
	Input             []string                   `json:"input"`
	Cost              Cost                       `json:"cost"`
	ContextWindow     int                        `json:"contextWindow"`
	ContextModes      []int                      `json:"contextModes,omitempty"`
	MaxTokens         int                        `json:"maxTokens"`
	Headers           map[string]string          `json:"headers,omitempty"`
	Compat            json.RawMessage            `json:"compat,omitempty"`
}

// RateLimitSnapshot is the normalized account quota shown by the app.
type RateLimitSnapshot struct {
	PlanType     *string          `json:"planType,omitempty"`
	Allowed      bool             `json:"allowed"`
	LimitReached bool             `json:"limitReached"`
	Primary      *RateLimitWindow `json:"primary,omitempty"`
	Secondary    *RateLimitWindow `json:"secondary,omitempty"`
}

type RateLimitWindow struct {
	UsedPercent        float64 `json:"usedPercent"`
	WindowDurationSecs int32   `json:"windowDurationSecs"`
	ResetAfterSecs     int32   `json:"resetAfterSecs"`
	ResetAt            int64   `json:"resetAt"`
}

func (m Model) SupportsThinking(e ThinkingEffort) bool {
	if e == ThinkingOff {
		return true
	}
	if !m.Reasoning {
		return false
	}
	if m.ThinkingEffortMap == nil {
		return true
	}
	value, exists := m.ThinkingEffortMap[e]
	return exists && value != nil
}
