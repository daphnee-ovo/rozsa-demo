// Package modelcatalog owns layered model metadata and provider availability.
package modelcatalog

import (
	"context"
	"fmt"
	"sort"
	"sync"
	"time"

	"rozsa/backend/internal/app/modelregistry"
	"rozsa/backend/internal/model/credentials"
	"rozsa/backend/internal/model/types"
)

type Diagnostic struct {
	Provider string `json:"provider"`
	Message  string `json:"message"`
}
type Snapshot struct {
	Models      []types.Model
	Diagnostics []Diagnostic
}
type DynamicRefresh func(context.Context, credentials.OAuthCredential, bool) (bool, error)
type Options struct {
	Directories [2]string
	Auth        *Auth
	Dynamic     map[string]DynamicRefresh
}
type Service interface {
	modelregistry.Reader
	Available(context.Context) (Snapshot, error)
	Refresh(context.Context, string, bool) error
	Watch(context.Context, func(Snapshot)) error
}
type catalog struct {
	options      Options
	operations   sync.Mutex
	mu           sync.RWMutex
	registry     *modelregistry.Registry
	dynamicState map[string]discoveryState
}
type discoveryState struct {
	access, account string
	err             error
	nextAttempt     time.Time
}

func New(options Options) (Service, error) {
	if options.Directories[0] == "" || options.Directories[1] == "" || options.Auth == nil {
		return nil, fmt.Errorf("model catalog requires global/project model directories and credentials")
	}
	registry, _, err := modelregistry.Load([]byte("{}"), options.Directories[:]...)
	if err != nil {
		return nil, err
	}
	return &catalog{options: options, registry: registry, dynamicState: map[string]discoveryState{}}, nil
}
func (catalog *catalog) All() []types.Model {
	catalog.mu.RLock()
	defer catalog.mu.RUnlock()
	return catalog.registry.All()
}
func (catalog *catalog) Find(provider, id string) (types.Model, bool) {
	catalog.mu.RLock()
	defer catalog.mu.RUnlock()
	return catalog.registry.Find(provider, id)
}
func (catalog *catalog) APIKey(provider string) (string, bool) {
	catalog.mu.RLock()
	defer catalog.mu.RUnlock()
	return catalog.registry.APIKey(provider)
}
func (catalog *catalog) IsConfigured(provider, id string) bool {
	catalog.mu.RLock()
	defer catalog.mu.RUnlock()
	return catalog.registry.IsConfigured(provider, id)
}

func (catalog *catalog) Available(ctx context.Context) (Snapshot, error) {
	catalog.operations.Lock()
	defer catalog.operations.Unlock()
	return catalog.available(ctx)
}
func (catalog *catalog) available(ctx context.Context) (Snapshot, error) {
	result := Snapshot{Models: []types.Model{}, Diagnostics: []Diagnostic{}}
	registry, diagnostics, err := modelregistry.Load([]byte("{}"), catalog.options.Directories[:]...)
	if err != nil {
		return result, err
	}
	for _, diagnostic := range diagnostics {
		result.Diagnostics = append(result.Diagnostics, Diagnostic{Provider: diagnostic.Provider, Message: diagnostic.Message})
	}
	available := map[string]bool{}
	for _, model := range registry.All() {
		provider := model.Provider
		if _, checked := available[provider]; checked {
			continue
		}
		if err := ctx.Err(); err != nil {
			return result, err
		}
		available[provider] = false
		var key string
		var err error
		var current credentials.OAuthCredential
		var refreshed bool
		if credentials.IsOAuthProvider(provider) {
			current, refreshed, err = catalog.options.Auth.Stored(ctx, provider)
			key = current.Access
			if err == nil && key != "" && provider == "codex-oauth" && credentials.AccountID(current) == "" {
				err = fmt.Errorf("codex-oauth credential is missing accountId; sign in again")
			}
		} else {
			configured, exists := registry.APIKey(provider)
			if exists && configured != "" {
				key, err = catalog.options.Auth.Resolver.ResolveConfigValue(configured)
			} else {
				key, err = catalog.options.Auth.Resolver.ProviderAPIKey(provider)
			}
			current.Access = key
		}
		if err == nil && key != "" {
			err = catalog.discover(ctx, provider, current, refreshed)
		}
		if key == "" {
			delete(catalog.dynamicState, provider)
		}
		if err != nil {
			result.Diagnostics = append(result.Diagnostics, Diagnostic{Provider: provider, Message: err.Error()})
			continue
		}
		available[provider] = key != ""
	}
	registry, _, err = modelregistry.Load([]byte("{}"), catalog.options.Directories[:]...)
	if err != nil {
		return result, err
	}
	for _, model := range registry.All() {
		if available[model.Provider] {
			result.Models = append(result.Models, model)
		}
	}
	sort.Slice(result.Diagnostics, func(i, j int) bool {
		return result.Diagnostics[i].Provider+result.Diagnostics[i].Message < result.Diagnostics[j].Provider+result.Diagnostics[j].Message
	})
	catalog.mu.Lock()
	catalog.registry = registry
	catalog.mu.Unlock()
	return result, nil
}

func (catalog *catalog) discover(ctx context.Context, provider string, current credentials.OAuthCredential, force bool) error {
	refresh := catalog.options.Dynamic[provider]
	if refresh == nil {
		return nil
	}
	state, exists := catalog.dynamicState[provider]
	account := credentials.AccountID(current)
	if !force && exists && state.access == current.Access && state.account == account && time.Now().Before(state.nextAttempt) {
		return state.err
	}
	_, err := refresh(ctx, current, force || exists && (state.access != current.Access || state.account != account))
	catalog.dynamicState[provider] = discoveryState{current.Access, account, err, time.Now().Add(time.Minute)}
	return err
}

// Refresh invokes only the named provider's registered dynamic discovery.
func (catalog *catalog) Refresh(ctx context.Context, provider string, force bool) error {
	if provider == "" {
		return fmt.Errorf("model refresh requires a provider")
	}
	catalog.operations.Lock()
	defer catalog.operations.Unlock()
	if refresh := catalog.options.Dynamic[provider]; refresh != nil {
		var current credentials.OAuthCredential
		var err error
		if credentials.IsOAuthProvider(provider) {
			current, _, err = catalog.options.Auth.Stored(ctx, provider)
		} else {
			configured, _ := catalog.APIKey(provider)
			var resolved RequestCredentials
			resolved, err = catalog.options.Auth.Resolve(ctx, provider, configured)
			current.Access = resolved.APIKey
		}
		if err != nil {
			return err
		}
		if current.Access != "" {
			if err := catalog.discover(ctx, provider, current, force); err != nil {
				return err
			}
		}
	}
	snapshot, err := catalog.available(ctx)
	if err != nil {
		return err
	}
	for _, diagnostic := range snapshot.Diagnostics {
		if diagnostic.Provider == provider {
			return fmt.Errorf("refresh %s models: %s", provider, diagnostic.Message)
		}
	}
	return nil
}
