// Package auth adapts Codex OAuth to the application credential lifecycle.
package auth

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"os"

	"rozsa/backend/internal/model/credentials"
	"rozsa/backend/internal/model/modelsendpoint"
	"rozsa/backend/internal/model/oauth/codex"
)

type Codex struct {
	Client        *codex.Client
	AuthPath      string
	ModelsPath    string
	RefreshModels func(context.Context, credentials.OAuthCredential) error
	Changed       func(provider string)
}

func NewCodex(authPath, globalModelsPath string, httpClient *http.Client) *Codex {
	return &Codex{Client: codex.NewClient(httpClient), AuthPath: authPath, ModelsPath: globalModelsPath}
}

func (service *Codex) Login(ctx context.Context, input <-chan string, events chan<- codex.Event) error {
	credential, err := service.client().Login(ctx, input, events)
	if err != nil {
		return fmt.Errorf("Codex OAuth login: %w", err)
	}
	if err = credentials.StoreOAuth(service.AuthPath, "codex-oauth", credential); err != nil {
		return fmt.Errorf("store Codex OAuth credential: %w", err)
	}
	if service.Changed != nil {
		service.Changed("codex-oauth")
	}
	return service.refreshModels(ctx, credential)
}
func (service *Codex) Refresh(ctx context.Context) (credentials.OAuthCredential, error) {
	current, err := readCredential(ctx, service.AuthPath)
	if err != nil {
		return credentials.OAuthCredential{}, err
	}
	refreshed, err := service.client().Refresh(ctx, current)
	if err != nil {
		return credentials.OAuthCredential{}, fmt.Errorf("refresh Codex OAuth credential: %w", err)
	}
	if err = credentials.StoreOAuth(service.AuthPath, "codex-oauth", refreshed); err != nil {
		return credentials.OAuthCredential{}, fmt.Errorf("store refreshed Codex OAuth credential: %w", err)
	}
	if service.Changed != nil {
		service.Changed("codex-oauth")
	}
	if err = service.refreshModels(ctx, refreshed); err != nil {
		return credentials.OAuthCredential{}, err
	}
	return refreshed, nil
}
func (service *Codex) Logout() (bool, error) {
	removed, err := credentials.RemoveStored(service.AuthPath, "codex-oauth")
	if err != nil {
		return false, fmt.Errorf("remove Codex OAuth credential: %w", err)
	}
	if removed && service.Changed != nil {
		service.Changed("codex-oauth")
	}
	return removed, nil
}
func (service *Codex) client() *codex.Client {
	if service.Client == nil {
		service.Client = codex.NewClient(nil)
	}
	return service.Client
}
func (service *Codex) refreshModels(ctx context.Context, credential credentials.OAuthCredential) error {
	if service.RefreshModels != nil {
		return service.RefreshModels(ctx, credential)
	}
	if service.ModelsPath == "" {
		return nil
	}
	if credential.Access == "" {
		return errors.New("Codex OAuth credential has no access token")
	}
	accountID, _ := credential.Extra["accountId"].(string)
	if accountID == "" {
		return errors.New("Codex OAuth credential has no accountId for model discovery")
	}
	if err := ctx.Err(); err != nil {
		return err
	}
	_, err := modelsendpoint.NewClient(nil).RefreshIfNeeded(ctx, service.ModelsPath, credential.Access, accountID, true)
	if err != nil {
		return fmt.Errorf("refresh Codex model catalog: %w", err)
	}
	return nil
}
func readCredential(ctx context.Context, path string) (credentials.OAuthCredential, error) {
	if err := ctx.Err(); err != nil {
		return credentials.OAuthCredential{}, err
	}
	data, err := os.ReadFile(path)
	if err != nil {
		return credentials.OAuthCredential{}, fmt.Errorf("read Codex OAuth credential file: %w", err)
	}
	var auth map[string]json.RawMessage
	if err = json.Unmarshal(data, &auth); err != nil {
		return credentials.OAuthCredential{}, fmt.Errorf("parse auth.json: %w", err)
	}
	raw, ok := auth["codex-oauth"]
	if !ok {
		return credentials.OAuthCredential{}, errors.New("Codex OAuth credential is missing")
	}
	var credential credentials.OAuthCredential
	if err = json.Unmarshal(raw, &credential); err != nil {
		return credentials.OAuthCredential{}, fmt.Errorf("decode Codex OAuth credential: %w", err)
	}
	if credential.Access == "" || credential.Refresh == "" {
		return credentials.OAuthCredential{}, errors.New("Codex OAuth credential is incomplete")
	}
	return credential, nil
}
