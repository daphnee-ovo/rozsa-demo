// Package modelregistry loads generated metadata and layered models.json files.
package modelregistry

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"sort"

	modeltypes "rozsa/backend/internal/model/types"
)

const (
	defaultContextWindow = 128000
	defaultMaxTokens     = 16384
)

type Diagnostic struct {
	Path      string `json:"path"`
	Severity  string `json:"severity"`
	Provider  string `json:"provider,omitempty"`
	Reference string `json:"reference,omitempty"`
	Message   string `json:"message"`
	Hint      string `json:"hint"`
}
type ConfigError struct {
	Path string
	Err  error
}

func (configError *ConfigError) Error() string {
	if configError.Path == "" {
		return "model registry: " + configError.Err.Error()
	}
	return fmt.Sprintf("model registry at %s: %v", configError.Path, configError.Err)
}
func (configError *ConfigError) Unwrap() error { return configError.Err }

type Reader interface {
	All() []modeltypes.Model
	Find(string, string) (modeltypes.Model, bool)
	APIKey(string) (string, bool)
	IsConfigured(string, string) bool
}

type Registry struct {
	models     []modeltypes.Model
	configured map[string]bool
	sources    map[string]string
	apiKeys    map[string]string
}

func Empty() *Registry {
	return &Registry{models: []modeltypes.Model{}, configured: map[string]bool{}, sources: map[string]string{}, apiKeys: map[string]string{}}
}
func FromGeneratedJSON(data []byte) (*Registry, error) {
	var providers map[string]map[string]modeltypes.Model
	if err := json.Unmarshal(data, &providers); err != nil {
		return nil, &ConfigError{Err: fmt.Errorf("parse generated models: %w", err)}
	}
	registry := Empty()
	for provider, models := range providers {
		for id, model := range models {
			if model.ID == "" {
				model.ID = id
			}
			if model.Provider == "" {
				model.Provider = provider
			}
			registry.models = append(registry.models, model)
		}
	}
	return registry, nil
}
func (registry *Registry) All() []modeltypes.Model {
	models := make([]modeltypes.Model, len(registry.models))
	copy(models, registry.models)
	return models
}
func (registry *Registry) Find(provider, id string) (modeltypes.Model, bool) {
	for _, model := range registry.models {
		if model.Provider == provider && model.ID == id {
			return model, true
		}
	}
	return modeltypes.Model{}, false
}
func (registry *Registry) IsConfigured(provider, id string) bool {
	return registry.configured[key(provider, id)]
}
func (registry *Registry) Source(provider, id string) (string, bool) {
	source, exists := registry.sources[key(provider, id)]
	return source, exists
}
func (registry *Registry) APIKey(provider string) (string, bool) {
	value, exists := registry.apiKeys[provider]
	return value, exists
}
func key(provider, id string) string { return provider + "\x00" + id }

// Load reads user scope first and project scope second; project entries win.
func Load(generated []byte, dirs ...string) (*Registry, []Diagnostic, error) {
	registry, err := FromGeneratedJSON(generated)
	if err != nil {
		return nil, nil, err
	}
	diagnostics := []Diagnostic{}
	for _, dir := range dirs {
		entries, err := os.ReadDir(dir)
		if errors.Is(err, fs.ErrNotExist) {
			continue
		}
		if err != nil {
			diagnostics = append(diagnostics, Diagnostic{Path: dir, Severity: "error", Message: err.Error(), Hint: "Check that the model configuration directory can be read."})
			continue
		}
		for _, entry := range entries {
			if entry.IsDir() || filepath.Ext(entry.Name()) != ".json" || entry.Name() == "auth.json" {
				continue
			}
			path := filepath.Join(dir, entry.Name())
			data, err := os.ReadFile(path)
			if err == nil {
				err = registry.apply(path, data)
			}
			if err != nil {
				diagnostics = append(diagnostics, Diagnostic{Path: path, Severity: "error", Message: err.Error(), Hint: "Fix the JSON/schema error. Other valid model configurations remain available."})
			}
		}
	}
	sort.SliceStable(registry.models, func(i, j int) bool {
		return registry.models[i].Provider+"/"+registry.models[i].ID < registry.models[j].Provider+"/"+registry.models[j].ID
	})
	return registry, diagnostics, nil
}

// Refresh is transactional: a failed load leaves this registry unchanged.
func (registry *Registry) Refresh(generated []byte, dirs ...string) ([]Diagnostic, error) {
	next, diagnostics, err := Load(generated, dirs...)
	if err != nil {
		return nil, err
	}
	*registry = *next
	return diagnostics, nil
}

type config struct {
	Providers map[string]providerConfig `json:"providers"`
}
type providerConfig struct {
	BaseURL    *string                  `json:"baseUrl"`
	APIKey     *string                  `json:"apiKey"`
	API        *string                  `json:"api"`
	AuthHeader *bool                    `json:"authHeader"`
	Headers    map[string]string        `json:"headers"`
	Compat     json.RawMessage          `json:"compat"`
	Models     []modelDefinition        `json:"models"`
	Overrides  map[string]modelOverride `json:"modelOverrides"`
}
type modelDefinition struct {
	ID             string            `json:"id"`
	Name           *string           `json:"name"`
	API            *string           `json:"api"`
	BaseURL        *string           `json:"baseUrl"`
	Reasoning      *bool             `json:"reasoning"`
	Thinking       json.RawMessage   `json:"thinkingEffortMap"`
	LegacyThinking json.RawMessage   `json:"thinkingLevelMap"`
	Input          []string          `json:"input"`
	Cost           *modeltypes.Cost  `json:"cost"`
	ContextWindow  *int              `json:"contextWindow"`
	ContextModes   []int             `json:"contextModes"`
	MaxTokens      *int              `json:"maxTokens"`
	Headers        map[string]string `json:"headers"`
	Compat         json.RawMessage   `json:"compat"`
}
type modelOverride struct {
	Name          *string            `json:"name"`
	Reasoning     *bool              `json:"reasoning"`
	Thinking      json.RawMessage    `json:"thinkingEffortMap"`
	Input         []string           `json:"input"`
	Cost          map[string]float64 `json:"cost"`
	ContextWindow *int               `json:"contextWindow"`
	ContextModes  []int              `json:"contextModes"`
	MaxTokens     *int               `json:"maxTokens"`
	Headers       map[string]string  `json:"headers"`
	Compat        json.RawMessage    `json:"compat"`
}

func (registry *Registry) apply(path string, data []byte) error {
	var parsedConfig config
	jsonDecoder := json.NewDecoder(bytes.NewReader(data))
	jsonDecoder.DisallowUnknownFields()
	if err := jsonDecoder.Decode(&parsedConfig); err != nil {
		return &ConfigError{Path: path, Err: fmt.Errorf("parse models.json: %w", err)}
	}
	for provider, providerConfig := range parsedConfig.Providers {
		if providerConfig.APIKey != nil {
			registry.apiKeys[provider] = *providerConfig.APIKey
		}
		for i := range registry.models {
			model := &registry.models[i]
			if model.Provider != provider {
				continue
			}
			if providerConfig.BaseURL != nil {
				model.BaseURL = *providerConfig.BaseURL
			}
			if len(providerConfig.Headers) > 0 {
				if model.Headers == nil {
					model.Headers = map[string]string{}
				}
				for headerName, headerValue := range providerConfig.Headers {
					model.Headers[headerName] = headerValue
				}
			}
			if len(providerConfig.Compat) > 0 && string(providerConfig.Compat) != "null" {
				model.Compat = mergeJSON(model.Compat, providerConfig.Compat)
			}
		}
		for _, definition := range providerConfig.Models {
			if definition.ID == "" {
				return &ConfigError{Path: path, Err: errors.New("model id is required")}
			}
			oldModel, found := registry.Find(provider, definition.ID)
			apiName, baseURL := "", ""
			if found {
				apiName, baseURL = string(oldModel.API), oldModel.BaseURL
			}
			if providerConfig.API != nil {
				apiName = *providerConfig.API
			}
			if definition.API != nil {
				apiName = *definition.API
			}
			if providerConfig.BaseURL != nil {
				baseURL = *providerConfig.BaseURL
			}
			if definition.BaseURL != nil {
				baseURL = *definition.BaseURL
			}
			if apiName == "" || baseURL == "" {
				return &ConfigError{Path: path, Err: fmt.Errorf("provider %s model %s requires api and baseUrl", provider, definition.ID)}
			}
			model := modeltypes.Model{ID: definition.ID, Name: definition.ID, API: modeltypes.API(apiName), Provider: provider, BaseURL: baseURL, Input: []string{"text"}, ContextWindow: defaultContextWindow, MaxTokens: defaultMaxTokens}
			if definition.Name != nil {
				model.Name = *definition.Name
			}
			if definition.Reasoning != nil {
				model.Reasoning = *definition.Reasoning
			}
			if definition.Input != nil {
				model.Input = definition.Input
			}
			if definition.Cost != nil {
				model.Cost = *definition.Cost
			}
			if definition.ContextWindow != nil {
				model.ContextWindow = *definition.ContextWindow
			}
			if definition.ContextModes != nil {
				model.ContextModes = definition.ContextModes
			}
			if definition.MaxTokens != nil {
				model.MaxTokens = *definition.MaxTokens
			}
			model.Headers = cloneMap(definition.Headers)
			if model.Headers == nil {
				model.Headers = cloneMap(providerConfig.Headers)
			}
			model.Compat = rawOr(definition.Compat, providerConfig.Compat)
			if err := setThinking(&model, definition.Thinking, definition.LegacyThinking); err != nil {
				return &ConfigError{Path: path, Err: err}
			}
			registry.put(model)
			registry.configured[key(provider, definition.ID)] = true
			registry.sources[key(provider, definition.ID)] = path
		}
		for id, override := range providerConfig.Overrides {
			model, found := registry.Find(provider, id)
			if !found {
				continue
			}
			if override.Name != nil {
				model.Name = *override.Name
			}
			if override.Reasoning != nil {
				model.Reasoning = *override.Reasoning
			}
			if override.Input != nil {
				model.Input = override.Input
			}
			if override.ContextWindow != nil {
				model.ContextWindow = *override.ContextWindow
			}
			if override.ContextModes != nil {
				model.ContextModes = override.ContextModes
			}
			if override.MaxTokens != nil {
				model.MaxTokens = *override.MaxTokens
			}
			if override.Headers != nil {
				model.Headers = cloneMap(override.Headers)
			}
			for costField, costValue := range override.Cost {
				switch costField {
				case "input":
					model.Cost.Input = costValue
				case "output":
					model.Cost.Output = costValue
				case "cacheRead":
					model.Cost.CacheRead = costValue
				case "cacheWrite":
					model.Cost.CacheWrite = costValue
				default:
					return &ConfigError{Path: path, Err: fmt.Errorf("unknown cost field %q", costField)}
				}
			}
			if err := setThinking(&model, override.Thinking, nil); err != nil {
				return &ConfigError{Path: path, Err: err}
			}
			if len(override.Compat) > 0 {
				model.Compat = mergeJSON(model.Compat, override.Compat)
			}
			registry.put(model)
			registry.configured[key(provider, id)] = true
			registry.sources[key(provider, id)] = path
		}
	}
	return nil
}
func (registry *Registry) put(model modeltypes.Model) {
	for i := range registry.models {
		if registry.models[i].Provider == model.Provider && registry.models[i].ID == model.ID {
			registry.models[i] = model
			return
		}
	}
	registry.models = append(registry.models, model)
}
func setThinking(model *modeltypes.Model, current, legacy json.RawMessage) error {
	thinkingMap := current
	if len(thinkingMap) == 0 {
		thinkingMap = legacy
	}
	if len(thinkingMap) == 0 {
		return nil
	}
	if string(thinkingMap) == "null" {
		model.ThinkingEffortMap = nil
		return nil
	}
	var values map[modeltypes.ThinkingEffort]*string
	if err := json.Unmarshal(thinkingMap, &values); err != nil {
		return fmt.Errorf("invalid thinkingEffortMap: %w", err)
	}
	model.ThinkingEffortMap = values
	return nil
}
func rawOr(modelValue, providerValue json.RawMessage) json.RawMessage {
	if len(modelValue) > 0 {
		return modelValue
	}
	return providerValue
}
func mergeJSON(baseJSON, overrideJSON json.RawMessage) json.RawMessage {
	if len(baseJSON) == 0 || string(baseJSON) == "null" {
		return append(json.RawMessage(nil), overrideJSON...)
	}
	var baseProperties, overrideProperties map[string]json.RawMessage
	if json.Unmarshal(baseJSON, &baseProperties) != nil || json.Unmarshal(overrideJSON, &overrideProperties) != nil {
		return append(json.RawMessage(nil), overrideJSON...)
	}
	for propertyName, propertyValue := range overrideProperties {
		baseProperties[propertyName] = propertyValue
	}
	mergedJSON, _ := json.Marshal(baseProperties)
	return mergedJSON
}
func cloneMap(sourceHeaders map[string]string) map[string]string {
	if sourceHeaders == nil {
		return nil
	}
	clonedHeaders := map[string]string{}
	for headerName, headerValue := range sourceHeaders {
		clonedHeaders[headerName] = headerValue
	}
	return clonedHeaders
}
