// Package productsettings composes the settings and themes module contracts for
// the desktop product API. Callers never construct the concrete storage types.
package productsettings

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"strconv"
	"strings"
	"sync"
	"time"

	"rozsa/backend/internal/app/modelcatalog"
	"rozsa/backend/internal/app/modelregistry"
	"rozsa/backend/internal/app/settings"
	"rozsa/backend/internal/app/themes"
	"rozsa/backend/internal/model/types"
)

type Scope = settings.Scope

const (
	Global  = settings.Global
	Project = settings.Project
)

type ThemeDefinition = themes.Definition
type ThemeSummary = themes.Summary

type Snapshot struct {
	Scope            Scope                     `json:"scope"`
	Settings         map[string]any            `json:"settings"`
	Appearance       themes.AppearanceSnapshot `json:"appearance"`
	Themes           []themes.Summary          `json:"themes"`
	Models           []ModelOption             `json:"models"`
	ModelDiagnostics []modelcatalog.Diagnostic `json:"modelDiagnostics"`
}

type ModelOption struct {
	ID            string `json:"id"`
	Name          string `json:"name"`
	Provider      string `json:"provider"`
	Reasoning     bool   `json:"reasoning"`
	ContextWindow int    `json:"contextWindow"`
}

// SettingsOwner is the layered settings contract required by the product API.
type SettingsOwner interface {
	Resolved() map[string]any
	Path(settings.Scope) (string, bool)
	Overrides(settings.Scope) (map[string]any, error)
	Set(settings.Scope, string, any) error
	SetMany(settings.Scope, map[string]any) error
	Reset(settings.Scope, string) error
}

type ModelSelector interface {
	SetModel(types.Model, string, types.ThinkingEffort) error
	SetThinkingEffort(types.ThinkingEffort) (types.ThinkingEffort, error)
}

type Service interface {
	SnapshotContext(context.Context, Scope) (Snapshot, error)
	Snapshot(Scope) (Snapshot, error)
	Update(Scope, string, any) (Snapshot, error)
	Reset(Scope, string) (Snapshot, error)
	GetTheme(string, themes.Mode) (themes.Definition, error)
	SaveTheme(themes.Definition) error
}

type service struct {
	mu         sync.Mutex
	settings   SettingsOwner
	themeStore themes.Store
	appearance themes.Appearance
	registry   modelregistry.Reader
	catalog    modelcatalog.Service
	selector   ModelSelector
}

func New(manager SettingsOwner, themeStore themes.Store) Service {
	return &service{settings: manager, themeStore: themeStore, appearance: themes.NewAppearance(themeStore, manager)}
}

func NewWithModelCatalog(manager SettingsOwner, themeStore themes.Store, catalog modelcatalog.Service, selector ModelSelector) Service {
	return &service{settings: manager, themeStore: themeStore, appearance: themes.NewAppearance(themeStore, manager), registry: catalog, catalog: catalog, selector: selector}
}

func (service *service) Snapshot(scope Scope) (Snapshot, error) {
	return service.SnapshotContext(context.Background(), scope)
}

func (service *service) SnapshotContext(ctx context.Context, scope Scope) (Snapshot, error) {
	service.mu.Lock()
	defer service.mu.Unlock()
	return service.snapshot(scope, ctx)
}

func (service *service) snapshot(scope Scope, ctx context.Context) (Snapshot, error) {
	if scope != Global && scope != Project {
		return Snapshot{}, fmt.Errorf("unsupported settings scope %q", scope)
	}
	if _, exists := service.settings.Path(scope); !exists {
		return Snapshot{}, fmt.Errorf("settings path for %s scope is unavailable", scope)
	}
	appearance, err := service.appearance.Snapshot()
	if err != nil {
		return Snapshot{}, err
	}
	available, err := service.themeStore.List()
	if err != nil {
		return Snapshot{}, err
	}
	update, err := service.models(ctx)
	if err != nil {
		return Snapshot{}, err
	}
	return Snapshot{Scope: scope, Settings: service.settings.Resolved(), Appearance: appearance, Themes: available, Models: update.Models, ModelDiagnostics: update.Diagnostics}, nil
}

type ModelUpdate struct {
	Models      []ModelOption             `json:"models"`
	Diagnostics []modelcatalog.Diagnostic `json:"diagnostics"`
}

func modelUpdate(snapshot modelcatalog.Snapshot) ModelUpdate {
	update := ModelUpdate{Models: []ModelOption{}, Diagnostics: snapshot.Diagnostics}
	for _, model := range snapshot.Models {
		update.Models = append(update.Models, ModelOption{ID: model.ID, Name: model.Name, Provider: model.Provider, Reasoning: model.Reasoning, ContextWindow: model.ContextWindow})
	}
	return update
}
func (service *service) models(ctx context.Context) (ModelUpdate, error) {
	if service.catalog == nil {
		return ModelUpdate{Models: []ModelOption{}, Diagnostics: []modelcatalog.Diagnostic{}}, nil
	}
	ctx, cancel := context.WithTimeout(ctx, 25*time.Second)
	defer cancel()
	snapshot, err := service.catalog.Available(ctx)
	return modelUpdate(snapshot), err
}
func (service *service) WatchModels(ctx context.Context, deliver func(ModelUpdate)) error {
	if service.catalog == nil {
		<-ctx.Done()
		return ctx.Err()
	}
	return service.catalog.Watch(ctx, func(snapshot modelcatalog.Snapshot) { deliver(modelUpdate(snapshot)) })
}

func (service *service) Update(scope Scope, key string, value any) (Snapshot, error) {
	service.mu.Lock()
	defer service.mu.Unlock()
	if key == "" || strings.TrimSpace(key) != key {
		return Snapshot{}, errors.New("setting key must be non-empty and trimmed")
	}
	if key == "model" {
		return service.selectModel(scope, value)
	}
	if key == "small_model" {
		modelID, ok := value.(string)
		if !ok {
			return Snapshot{}, errors.New("small model must be a model id")
		}
		if modelID != "" {
			if service.registry == nil {
				return Snapshot{}, errors.New("model catalog is unavailable")
			}
			found := false
			available, err := service.models(context.Background())
			if err != nil {
				return Snapshot{}, err
			}
			for _, model := range available.Models {
				if model.ID == modelID {
					found = true
					break
				}
			}
			if !found {
				return Snapshot{}, fmt.Errorf("small model %q is not configured", modelID)
			}
		}
	}
	if strings.HasPrefix(key, "appearance_") {
		field := map[string]string{
			"appearance_theme_mode": "themeMode", "appearance_font_size": "fontSize",
			"appearance_translucent_sidebar": "translucentSidebar", "appearance_show_rate_limits": "showRateLimits",
			"appearance_show_hourly_rate_limit": "showHourlyRateLimit", "appearance_show_weekly_rate_limit": "showWeeklyRateLimit",
			"appearance_rate_limit_display_mode": "rateLimitDisplayMode", "appearance_light_theme": "lightTheme",
			"appearance_dark_theme": "darkTheme",
		}[key]
		if field == "" {
			return Snapshot{}, fmt.Errorf("unknown setting: %s", key)
		}
		if field == "lightTheme" || field == "darkTheme" {
			id, ok := value.(string)
			if !ok {
				return Snapshot{}, fmt.Errorf("appearance.%s must be a string", field)
			}
			mode := themes.Light
			if field == "darkTheme" {
				mode = themes.Dark
			}
			if _, err := service.themeStore.Get(id, mode); err != nil {
				return Snapshot{}, err
			}
			if _, err := service.appearance.Select(scope, mode, id); err != nil {
				return Snapshot{}, err
			}
		} else if field == "themeMode" || field == "fontSize" || field == "translucentSidebar" {
			if _, err := service.appearance.Update(scope, field, value); err != nil {
				return Snapshot{}, err
			}
		} else if err := service.settings.Set(scope, "appearance."+field, value); err != nil {
			return Snapshot{}, err
		}
	} else {
		path, valid := settingPath(key)
		if !valid {
			return Snapshot{}, fmt.Errorf("unknown setting: %s", key)
		}
		normalized, err := normalizeValue(value)
		if err != nil {
			return Snapshot{}, err
		}
		if err := validateValue(key, normalized); err != nil {
			return Snapshot{}, err
		}
		if key == "transport" && normalized != "auto" && normalized != "sse" {
			return Snapshot{}, fmt.Errorf("transport %q is not implemented", normalized)
		}
		var previousThinking any
		if key == "thinking" {
			overrides, err := service.settings.Overrides(scope)
			if err != nil {
				return Snapshot{}, err
			}
			previousThinking = overrides["defaultThinkingEffort"]
		}
		if err := service.settings.Set(scope, path, normalized); err != nil {
			return Snapshot{}, err
		}
		if key == "thinking" && service.selector != nil {
			effort, err := service.selector.SetThinkingEffort(types.ThinkingEffort(normalized.(string)))
			if err != nil {
				return Snapshot{}, errors.Join(err, service.settings.SetMany(scope, map[string]any{"defaultThinkingEffort": previousThinking}))
			}
			if string(effort) != normalized {
				if err := service.settings.Set(scope, path, string(effort)); err != nil {
					return Snapshot{}, err
				}
			}
		}
	}
	return service.snapshot(scope, context.Background())
}

func (service *service) selectModel(scope Scope, value any) (Snapshot, error) {
	selection, ok := value.(map[string]any)
	if !ok {
		return Snapshot{}, errors.New("model selection must include provider and id")
	}
	provider, providerOK := selection["provider"].(string)
	id, idOK := selection["id"].(string)
	if !providerOK || !idOK || provider == "" || id == "" || len(selection) != 2 {
		return Snapshot{}, errors.New("model selection must include provider and id")
	}
	if service.registry == nil || service.selector == nil {
		return Snapshot{}, errors.New("model selection is unavailable")
	}
	available, err := service.models(context.Background())
	if err != nil {
		return Snapshot{}, err
	}
	found := false
	for _, option := range available.Models {
		if option.Provider == provider && option.ID == id {
			found = true
			break
		}
	}
	if !found {
		return Snapshot{}, fmt.Errorf("model %s/%s has no available provider credentials", provider, id)
	}
	model, exists := service.registry.Find(provider, id)
	if !exists {
		return Snapshot{}, fmt.Errorf("model %s/%s is not configured", provider, id)
	}
	previous, err := service.settings.Overrides(scope)
	if err != nil {
		return Snapshot{}, err
	}
	effortText, _ := service.settings.Resolved()["defaultThinkingEffort"].(string)
	effort := types.ThinkingEffort(effortText)
	if effort == "" {
		effort = types.ThinkingOff
	}
	if !model.SupportsThinking(effort) {
		effort = types.ThinkingOff
	}
	updates := map[string]any{"defaultProvider": provider, "defaultModel": id, "defaultThinkingEffort": string(effort)}
	if err := service.settings.SetMany(scope, updates); err != nil {
		return Snapshot{}, err
	}
	apiKey, _ := service.registry.APIKey(provider)
	if err := service.selector.SetModel(model, apiKey, effort); err != nil {
		rollbackErr := service.settings.SetMany(scope, map[string]any{"defaultProvider": previous["defaultProvider"], "defaultModel": previous["defaultModel"], "defaultThinkingEffort": previous["defaultThinkingEffort"]})
		return Snapshot{}, errors.Join(err, rollbackErr)
	}
	return service.snapshot(scope, context.Background())
}

func (service *service) Reset(scope Scope, key string) (Snapshot, error) {
	service.mu.Lock()
	defer service.mu.Unlock()
	path, valid := settingPath(key)
	if strings.HasPrefix(key, "appearance_") {
		field := map[string]string{
			"appearance_theme_mode": "themeMode", "appearance_font_size": "fontSize",
			"appearance_translucent_sidebar": "translucentSidebar", "appearance_show_rate_limits": "showRateLimits",
			"appearance_show_hourly_rate_limit": "showHourlyRateLimit", "appearance_show_weekly_rate_limit": "showWeeklyRateLimit",
			"appearance_rate_limit_display_mode": "rateLimitDisplayMode", "appearance_light_theme": "lightTheme", "appearance_dark_theme": "darkTheme",
		}[key]
		if field == "" {
			return Snapshot{}, fmt.Errorf("unknown setting: %s", key)
		}
		path, valid = "appearance."+field, true
	}
	if !valid {
		return Snapshot{}, fmt.Errorf("unknown setting: %s", key)
	}
	if err := service.settings.Reset(scope, path); err != nil {
		return Snapshot{}, err
	}
	return service.snapshot(scope, context.Background())
}

func (service *service) GetTheme(id string, mode themes.Mode) (themes.Definition, error) {
	service.mu.Lock()
	defer service.mu.Unlock()
	return service.themeStore.Get(id, mode)
}

func (service *service) SaveTheme(theme themes.Definition) error {
	service.mu.Lock()
	defer service.mu.Unlock()
	return service.themeStore.Save(theme)
}

func settingPath(key string) (string, bool) {
	paths := map[string]string{
		"thinking": "defaultThinkingEffort", "auto_compact": "compaction.enabled", "compaction_trigger_ratio": "compaction.triggerRatio",
		"compaction_target_ratio": "compaction.targetRatio", "retry_timeout_ms": "retry.timeoutMs", "retry_max_retries": "retry.maxRetries",
		"retry_max_delay_ms": "retry.maxRetryDelayMs", "auto_session_naming": "autoSessionNaming", "steering_mode": "steeringMode",
		"follow_up_mode": "followUpMode", "running_send_mode": "runningSendMode", "transport": "transport",
		"block_images": "blockImages", "hide_thinking": "hideThinking", "small_model": "smallModel",
	}
	path, exists := paths[key]
	return path, exists
}

func normalizeValue(value any) (any, error) {
	switch typed := value.(type) {
	case json.Number:
		if parsed, err := typed.Float64(); err == nil {
			return parsed, nil
		}
		if integer, err := typed.Int64(); err == nil {
			return integer, nil
		}
		return nil, fmt.Errorf("setting number must be an integer: %s", typed)
	case float64:
		return typed, nil
	case string:
		if typed == "" {
			return nil, nil
		}
		return typed, nil
	default:
		return value, nil
	}
}

func validateValue(key string, value any) error {
	switch key {
	case "thinking":
		text, ok := value.(string)
		if !ok || !oneOf(text, "off", "low", "medium", "high", "xhigh", "max") {
			return fmt.Errorf("invalid thinking effort: %v", value)
		}
	case "steering_mode":
		text, ok := value.(string)
		if !ok || !oneOf(text, "one-at-a-time", "all") {
			return fmt.Errorf("invalid steering mode: %v", value)
		}
	case "follow_up_mode":
		text, ok := value.(string)
		if !ok || !oneOf(text, "one-at-a-time", "all") {
			return fmt.Errorf("invalid follow-up mode: %v", value)
		}
	case "running_send_mode":
		text, ok := value.(string)
		if !ok || !oneOf(text, "queue", "steer") {
			return fmt.Errorf("invalid running send mode: %v", value)
		}
	case "transport":
		text, ok := value.(string)
		if !ok || !oneOf(text, "auto", "sse", "websocket", "websocket-cached") {
			return fmt.Errorf("invalid transport: %v", value)
		}
	case "auto_compact", "auto_session_naming", "block_images", "hide_thinking":
		if _, ok := value.(bool); !ok {
			return fmt.Errorf("%s must be a boolean", key)
		}
	case "appearance_show_rate_limits", "appearance_show_hourly_rate_limit", "appearance_show_weekly_rate_limit":
		if _, ok := value.(bool); !ok {
			return fmt.Errorf("%s must be a boolean", key)
		}
	case "appearance_rate_limit_display_mode":
		text, ok := value.(string)
		if !ok || !oneOf(text, "used", "remained") {
			return fmt.Errorf("invalid rate limit display mode: %v", value)
		}
	case "compaction_trigger_ratio", "compaction_target_ratio":
		number, ok := decimal(value)
		if !ok || number <= 0 || number > 1 {
			return fmt.Errorf("%s must be greater than 0 and at most 1", key)
		}
	case "retry_timeout_ms", "retry_max_retries", "retry_max_delay_ms":
		if value != nil {
			if _, ok := integer(value); !ok {
				return fmt.Errorf("%s must be an integer or empty", key)
			}
		}
	}
	return nil
}

func integer(value any) (int64, bool) {
	switch typed := value.(type) {
	case int:
		return int64(typed), true
	case int64:
		return typed, true
	case uint64:
		if typed <= uint64(^uint64(0)>>1) {
			return int64(typed), true
		}
	case string:
		parsed, err := strconv.ParseInt(typed, 10, 64)
		return parsed, err == nil
	}
	return 0, false
}

func decimal(value any) (float64, bool) {
	switch typed := value.(type) {
	case float64:
		return typed, true
	case int:
		return float64(typed), true
	case int64:
		return float64(typed), true
	case json.Number:
		parsed, err := typed.Float64()
		return parsed, err == nil
	}
	return 0, false
}

func oneOf(value string, allowed ...string) bool {
	for _, candidate := range allowed {
		if value == candidate {
			return true
		}
	}
	return false
}
