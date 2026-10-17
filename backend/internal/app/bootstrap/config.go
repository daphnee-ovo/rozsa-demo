// Package bootstrap resolves the shared model, settings, and credential
// configuration used by the CLI and desktop backend entry points.
package bootstrap

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"

	"rozsa/backend/internal/app/modelregistry"
	"rozsa/backend/internal/app/settings"
	"rozsa/backend/internal/model/types"
	"rozsa/backend/internal/platform/configpaths"
)

// ConfiguredModel resolves the configured default model unless explicit
// provider/model overrides are supplied. Credentials remain unresolved until
// Runtime.New, which applies the existing provider-specific resolver.
func ConfiguredModel(providerOverride, modelOverride string) (types.Model, string, string, error) {
	cwd, err := os.Getwd()
	if err != nil {
		return types.Model{}, "", "", fmt.Errorf("get working directory: %w", err)
	}
	roots, err := configpaths.Discover(cwd)
	if err != nil {
		return types.Model{}, "", "", err
	}
	settingsPaths := roots.SettingsPaths()
	projectSettings := settingsPaths[1]
	settingsManager, err := settings.Load(settingsPaths[0], &projectSettings, nil)
	if err != nil {
		return types.Model{}, "", "", err
	}
	modelDirs := roots.ModelDirs()
	registry, diagnostics, err := modelregistry.Load([]byte("{}"), modelDirs[:]...)
	if err != nil {
		return types.Model{}, "", "", err
	}
	for _, diagnostic := range diagnostics {
		if diagnostic.Severity == "error" {
			return types.Model{}, "", "", fmt.Errorf("model configuration %s: %s", diagnostic.Path, diagnostic.Message)
		}
	}
	resolved := settingsManager.Resolved()
	provider, _ := resolved["defaultProvider"].(string)
	modelID, _ := resolved["defaultModel"].(string)
	if providerOverride != "" {
		provider = providerOverride
	}
	if modelOverride != "" {
		modelID = modelOverride
	}
	if provider == "" || modelID == "" {
		available := registry.All()
		if len(available) != 1 {
			return types.Model{}, "", "", errors.New("select a configured defaultProvider/defaultModel or pass --provider and --model")
		}
		provider, modelID = available[0].Provider, available[0].ID
	}
	selected, exists := registry.Find(provider, modelID)
	if !exists {
		return types.Model{}, "", "", fmt.Errorf("configured model %s/%s was not found", provider, modelID)
	}
	key, _ := registry.APIKey(provider)
	return selected, key, filepath.Join(roots.Global(), "auth.json"), nil
}

// SessionDirectories returns the writable global directory and the layered
// global/project catalog directories for the selected project working path.
func SessionDirectories(projectDir string) (string, []string, error) {
	roots, err := configpaths.Discover(projectDir)
	if err != nil {
		return "", nil, err
	}
	directories := roots.SessionDirs(projectDir)
	return roots.WritableSessionDir(projectDir), directories[:], nil
}

