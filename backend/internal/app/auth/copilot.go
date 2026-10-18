// Package auth adapts the Copilot OAuth client to auth.json lifecycle operations.
package auth

import (
	"context"
	"errors"
	"fmt"

	"rozsa/backend/internal/model/credentials"
	"rozsa/backend/internal/model/oauth/copilot"
)

const CopilotProvider = "github-copilot"

// Copilot owns only the github-copilot entry in the explicitly supplied auth file.
type Copilot struct {
	Client          *copilot.Client
	AuthPath        string
	OnModelsChanged func(context.Context, string) error
}

func (service *Copilot) Login(ctx context.Context, input copilot.LoginInput, emit copilot.EventHandler) (credentials.OAuthCredential, error) {
	if service.AuthPath == "" {
		return credentials.OAuthCredential{}, errors.New("GitHub Copilot auth.json path is required")
	}
	client := service.client()
	credential, err := client.Login(ctx, input, emit)
	if err != nil {
		return credentials.OAuthCredential{}, err
	}
	if err := ctx.Err(); err != nil {
		return credentials.OAuthCredential{}, err
	}
	if err := credentials.StoreOAuth(service.AuthPath, CopilotProvider, credential); err != nil {
		return credentials.OAuthCredential{}, err
	}
	return credential, service.modelsChanged(ctx)
}

func (service *Copilot) Refresh(ctx context.Context, credential credentials.OAuthCredential) (credentials.OAuthCredential, error) {
	if service.AuthPath == "" {
		return credentials.OAuthCredential{}, errors.New("GitHub Copilot auth.json path is required")
	}
	updated, err := service.client().Refresh(ctx, credential)
	if err != nil {
		return credentials.OAuthCredential{}, err
	}
	if err := ctx.Err(); err != nil {
		return credentials.OAuthCredential{}, err
	}
	if err := credentials.StoreOAuth(service.AuthPath, CopilotProvider, updated); err != nil {
		return credentials.OAuthCredential{}, err
	}
	return updated, service.modelsChanged(ctx)
}

func (service *Copilot) Logout(ctx context.Context) (bool, error) {
	if service.AuthPath == "" {
		return false, errors.New("GitHub Copilot auth.json path is required")
	}
	if err := ctx.Err(); err != nil {
		return false, err
	}
	removed, err := credentials.RemoveStored(service.AuthPath, CopilotProvider)
	if err != nil {
		return false, err
	}
	if removed {
		if err := service.modelsChanged(ctx); err != nil {
			return true, err
		}
	}
	return removed, nil
}

func (service *Copilot) client() *copilot.Client {
	if service.Client != nil {
		return service.Client
	}
	return copilot.NewClient(nil)
}
func (service *Copilot) modelsChanged(ctx context.Context) error {
	if service.OnModelsChanged != nil {
		if err := ctx.Err(); err != nil {
			return err
		}
		if err := service.OnModelsChanged(ctx, CopilotProvider); err != nil {
			return fmt.Errorf("refresh GitHub Copilot model availability: %w", err)
		}
	}
	return nil
}
