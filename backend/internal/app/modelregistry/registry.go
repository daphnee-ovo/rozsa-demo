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

func (e *ConfigError) Error() string {
	if e.Path == "" {
		return "model registry: " + e.Err.Error()
	}
	return fmt.Sprintf("model registry at %s: %v", e.Path, e.Err)
}
func (e *ConfigError) Unwrap() error { return e.Err }

type Registry struct {
	models     []modeltypes.Model
	configured map[string]bool
	sources    map[string]string
}

func Empty() *Registry {
	return &Registry{models: []modeltypes.Model{}, configured: map[string]bool{}, sources: map[string]string{}}
}
func FromGeneratedJSON(data []byte) (*Registry, error) {
	var providers map[string]map[string]modeltypes.Model
	if err := json.Unmarshal(data, &providers); err != nil {
		return nil, &ConfigError{Err: fmt.Errorf("parse generated models: %w", err)}
	}
	r := Empty()
	for provider, models := range providers {
		for id, m := range models {
			if m.ID == "" {
				m.ID = id
			}
			if m.Provider == "" {
				m.Provider = provider
			}
			r.models = append(r.models, m)
		}
	}
	return r, nil
}
func (r *Registry) All() []modeltypes.Model {
	out := make([]modeltypes.Model, len(r.models))
	copy(out, r.models)
	return out
}
func (r *Registry) Find(provider, id string) (modeltypes.Model, bool) {
	for _, m := range r.models {
		if m.Provider == provider && m.ID == id {
			return m, true
		}
	}
	return modeltypes.Model{}, false
}
func (r *Registry) IsConfigured(provider, id string) bool { return r.configured[key(provider, id)] }
func (r *Registry) Source(provider, id string) (string, bool) {
	v, ok := r.sources[key(provider, id)]
	return v, ok
}
func key(provider, id string) string { return provider + "\x00" + id }

// Load reads user scope first and project scope second; project entries win.
func Load(generated []byte, dirs ...string) (*Registry, []Diagnostic, error) {
	r, err := FromGeneratedJSON(generated)
	if err != nil {
		return nil, nil, err
	}
	ds := []Diagnostic{}
	for _, dir := range dirs {
		entries, e := os.ReadDir(dir)
		if errors.Is(e, fs.ErrNotExist) {
			continue
		}
		if e != nil {
			ds = append(ds, Diagnostic{Path: dir, Severity: "error", Message: e.Error(), Hint: "Check that the model configuration directory can be read."})
			continue
		}
		for _, entry := range entries {
			if entry.IsDir() || filepath.Ext(entry.Name()) != ".json" || entry.Name() == "auth.json" {
				continue
			}
			path := filepath.Join(dir, entry.Name())
			data, e := os.ReadFile(path)
			if e == nil {
				e = r.apply(path, data)
			}
			if e != nil {
				ds = append(ds, Diagnostic{Path: path, Severity: "error", Message: e.Error(), Hint: "Fix the JSON/schema error. Other valid model configurations remain available."})
			}
		}
	}
	sort.SliceStable(r.models, func(i, j int) bool {
		return r.models[i].Provider+"/"+r.models[i].ID < r.models[j].Provider+"/"+r.models[j].ID
	})
	return r, ds, nil
}

// Refresh is transactional: a failed load leaves this registry unchanged.
func (r *Registry) Refresh(generated []byte, dirs ...string) ([]Diagnostic, error) {
	next, ds, err := Load(generated, dirs...)
	if err != nil {
		return nil, err
	}
	*r = *next
	return ds, nil
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

func (r *Registry) apply(path string, data []byte) error {
	var c config
	dec := json.NewDecoder(bytes.NewReader(data))
	dec.DisallowUnknownFields()
	if err := dec.Decode(&c); err != nil {
		return &ConfigError{Path: path, Err: fmt.Errorf("parse models.json: %w", err)}
	}
	for provider, p := range c.Providers {
		for i := range r.models {
			m := &r.models[i]
			if m.Provider != provider {
				continue
			}
			if p.BaseURL != nil {
				m.BaseURL = *p.BaseURL
			}
			if len(p.Headers) > 0 {
				if m.Headers == nil {
					m.Headers = map[string]string{}
				}
				for k, v := range p.Headers {
					m.Headers[k] = v
				}
			}
			if len(p.Compat) > 0 && string(p.Compat) != "null" {
				m.Compat = mergeJSON(m.Compat, p.Compat)
			}
		}
		for _, d := range p.Models {
			if d.ID == "" {
				return &ConfigError{Path: path, Err: errors.New("model id is required")}
			}
			old, ok := r.Find(provider, d.ID)
			api, base := "", ""
			if ok {
				api, base = string(old.API), old.BaseURL
			}
			if p.API != nil {
				api = *p.API
			}
			if d.API != nil {
				api = *d.API
			}
			if p.BaseURL != nil {
				base = *p.BaseURL
			}
			if d.BaseURL != nil {
				base = *d.BaseURL
			}
			if api == "" || base == "" {
				return &ConfigError{Path: path, Err: fmt.Errorf("provider %s model %s requires api and baseUrl", provider, d.ID)}
			}
			m := modeltypes.Model{ID: d.ID, Name: d.ID, API: modeltypes.API(api), Provider: provider, BaseURL: base, Input: []string{"text"}, ContextWindow: defaultContextWindow, MaxTokens: defaultMaxTokens}
			if d.Name != nil {
				m.Name = *d.Name
			}
			if d.Reasoning != nil {
				m.Reasoning = *d.Reasoning
			}
			if d.Input != nil {
				m.Input = d.Input
			}
			if d.Cost != nil {
				m.Cost = *d.Cost
			}
			if d.ContextWindow != nil {
				m.ContextWindow = *d.ContextWindow
			}
			if d.ContextModes != nil {
				m.ContextModes = d.ContextModes
			}
			if d.MaxTokens != nil {
				m.MaxTokens = *d.MaxTokens
			}
			m.Headers = cloneMap(d.Headers)
			if m.Headers == nil {
				m.Headers = cloneMap(p.Headers)
			}
			m.Compat = rawOr(d.Compat, p.Compat)
			if err := setThinking(&m, d.Thinking, d.LegacyThinking); err != nil {
				return &ConfigError{Path: path, Err: err}
			}
			r.put(m)
			r.configured[key(provider, d.ID)] = true
			r.sources[key(provider, d.ID)] = path
		}
		for id, o := range p.Overrides {
			m, ok := r.Find(provider, id)
			if !ok {
				continue
			}
			if o.Name != nil {
				m.Name = *o.Name
			}
			if o.Reasoning != nil {
				m.Reasoning = *o.Reasoning
			}
			if o.Input != nil {
				m.Input = o.Input
			}
			if o.ContextWindow != nil {
				m.ContextWindow = *o.ContextWindow
			}
			if o.ContextModes != nil {
				m.ContextModes = o.ContextModes
			}
			if o.MaxTokens != nil {
				m.MaxTokens = *o.MaxTokens
			}
			if o.Headers != nil {
				m.Headers = cloneMap(o.Headers)
			}
			for k, v := range o.Cost {
				switch k {
				case "input":
					m.Cost.Input = v
				case "output":
					m.Cost.Output = v
				case "cacheRead":
					m.Cost.CacheRead = v
				case "cacheWrite":
					m.Cost.CacheWrite = v
				default:
					return &ConfigError{Path: path, Err: fmt.Errorf("unknown cost field %q", k)}
				}
			}
			if err := setThinking(&m, o.Thinking, nil); err != nil {
				return &ConfigError{Path: path, Err: err}
			}
			if len(o.Compat) > 0 {
				m.Compat = mergeJSON(m.Compat, o.Compat)
			}
			r.put(m)
			r.configured[key(provider, id)] = true
			r.sources[key(provider, id)] = path
		}
	}
	return nil
}
func (r *Registry) put(m modeltypes.Model) {
	for i := range r.models {
		if r.models[i].Provider == m.Provider && r.models[i].ID == m.ID {
			r.models[i] = m
			return
		}
	}
	r.models = append(r.models, m)
}
func setThinking(m *modeltypes.Model, current, legacy json.RawMessage) error {
	v := current
	if len(v) == 0 {
		v = legacy
	}
	if len(v) == 0 {
		return nil
	}
	if string(v) == "null" {
		m.ThinkingEffortMap = nil
		return nil
	}
	var values map[modeltypes.ThinkingEffort]*string
	if err := json.Unmarshal(v, &values); err != nil {
		return fmt.Errorf("invalid thinkingEffortMap: %w", err)
	}
	m.ThinkingEffortMap = values
	return nil
}
func rawOr(a, b json.RawMessage) json.RawMessage {
	if len(a) > 0 {
		return a
	}
	return b
}
func mergeJSON(a, b json.RawMessage) json.RawMessage {
	if len(a) == 0 || string(a) == "null" {
		return append(json.RawMessage(nil), b...)
	}
	var x, y map[string]json.RawMessage
	if json.Unmarshal(a, &x) != nil || json.Unmarshal(b, &y) != nil {
		return append(json.RawMessage(nil), b...)
	}
	for k, v := range y {
		x[k] = v
	}
	out, _ := json.Marshal(x)
	return out
}
func cloneMap(in map[string]string) map[string]string {
	if in == nil {
		return nil
	}
	out := map[string]string{}
	for k, v := range in {
		out[k] = v
	}
	return out
}
