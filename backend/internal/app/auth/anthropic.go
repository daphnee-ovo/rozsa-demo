// Package auth adapts Anthropic OAuth flows to app credential persistence.
package auth

import (
	"context"
	"errors"
	"fmt"
	"sync"

	"rozsa/backend/internal/model/credentials"
	"rozsa/backend/internal/model/oauth/anthropic"
)

type ModelChangeFunc func(context.Context, string) error

// Anthropic owns one provider's OAuth lifecycle. The auth.json path is supplied
// by the caller so shared directory discovery remains outside this package.
type Anthropic struct {
	path          string
	client        *anthropic.Client
	onModelChange ModelChangeFunc

	mu         sync.Mutex
	generation uint64
	activeStop context.CancelFunc
}

func NewAnthropic(path string, client *anthropic.Client, onModelChange ModelChangeFunc) (*Anthropic, error) {
	if path == "" {
		return nil, errors.New("Anthropic auth.json path is required")
	}
	if client == nil {
		return nil, errors.New("Anthropic OAuth client is required")
	}
	return &Anthropic{path: path, client: client, onModelChange: onModelChange}, nil
}

func (manager *Anthropic) Login(ctx context.Context, events chan<- anthropic.Event, inputs <-chan anthropic.Input) (credentials.OAuthCredential, error) {
	operationContext, generation, finish := manager.begin(ctx)
	defer finish()
	credential, err := manager.client.Login(operationContext, events, inputs)
	if err != nil {
		return credentials.OAuthCredential{}, err
	}
	if err := manager.storeIfCurrent(operationContext, generation, credential); err != nil {
		return credentials.OAuthCredential{}, err
	}
	return credential, manager.notify(operationContext)
}

// Refresh refreshes and persists the supplied provider credential. Callers
// obtain it through the shared credentials directory service.
func (manager *Anthropic) Refresh(ctx context.Context, current credentials.OAuthCredential) (credentials.OAuthCredential, error) {
	operationContext, generation, finish := manager.begin(ctx)
	defer finish()
	updated, err := manager.client.Refresh(operationContext, current)
	if err != nil {
		return credentials.OAuthCredential{}, err
	}
	if err := manager.storeIfCurrent(operationContext, generation, updated); err != nil {
		return credentials.OAuthCredential{}, err
	}
	return updated, manager.notify(operationContext)
}

func (manager *Anthropic) Logout(ctx context.Context) error {
	manager.mu.Lock()
	manager.generation++
	if manager.activeStop != nil {
		manager.activeStop()
		manager.activeStop = nil
	}
	if err := ctx.Err(); err != nil {
		manager.mu.Unlock()
		return err
	}
	_, err := credentials.RemoveStored(manager.path, "anthropic")
	manager.mu.Unlock()
	if err != nil {
		return fmt.Errorf("remove Anthropic OAuth credential: %w", err)
	}
	return manager.notify(ctx)
}

func (manager *Anthropic) begin(ctx context.Context) (context.Context, uint64, func()) {
	manager.mu.Lock()
	if manager.activeStop != nil {
		manager.activeStop()
	}
	operationContext, cancel := context.WithCancel(ctx)
	manager.generation++
	generation := manager.generation
	manager.activeStop = cancel
	manager.mu.Unlock()
	finish := func() {
		manager.mu.Lock()
		if manager.generation == generation {
			manager.activeStop = nil
		}
		manager.mu.Unlock()
		cancel()
	}
	return operationContext, generation, finish
}

func (manager *Anthropic) storeIfCurrent(ctx context.Context, generation uint64, credential credentials.OAuthCredential) error {
	manager.mu.Lock()
	defer manager.mu.Unlock()
	if err := ctx.Err(); err != nil {
		return err
	}
	if generation != manager.generation {
		return errors.New("Anthropic OAuth request was superseded")
	}
	if err := credentials.StoreOAuth(manager.path, "anthropic", credential); err != nil {
		return fmt.Errorf("store Anthropic OAuth credential: %w", err)
	}
	return nil
}

func (manager *Anthropic) notify(ctx context.Context) error {
	if manager.onModelChange == nil {
		return nil
	}
	if err := ctx.Err(); err != nil {
		return err
	}
	if err := manager.onModelChange(ctx, "anthropic"); err != nil {
		return fmt.Errorf("refresh Anthropic model availability: %w", err)
	}
	return nil
}
