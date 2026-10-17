package main

import (
	"context"
	"errors"
	"flag"
	"fmt"
	"os"
	"os/signal"
	"path/filepath"
	"syscall"

	"rozsa/backend/internal/app/modelregistry"
	"rozsa/backend/internal/app/runtime"
	"rozsa/backend/internal/app/settings"
	"rozsa/backend/internal/model/types"
	"rozsa/backend/internal/platform/configpaths"
)

func main() {
	var sessionDir, apiKey, baseURL, providerID, modelID string
	var continueSession bool
	flag.StringVar(&sessionDir, "session-dir", "", "directory for the active JSONL session")
	flag.StringVar(&apiKey, "api-key", "", "override the configured provider API key")
	flag.StringVar(&baseURL, "base-url", "", "override the configured provider base URL")
	flag.StringVar(&providerID, "provider", "", "override the configured default provider")
	flag.StringVar(&modelID, "model", "", "override the configured default model")
	flag.BoolVar(&continueSession, "continue", false, "continue the latest session")
	flag.Parse()
	if sessionDir == "" {
		fail("--session-dir is required", 2)
	}
	if flag.NArg() > 1 {
		fail("expected at most one prompt argument", 2)
	}
	if continueSession == (flag.NArg() == 1) {
		fail("provide a prompt or --continue, but not both", 2)
	}
	model, configuredKey, authPath, err := configuredModel(providerID, modelID)
	if err != nil {
		fail(err.Error(), 1)
	}
	if baseURL != "" {
		model.BaseURL = baseURL
	}
	if apiKey == "" {
		apiKey = configuredKey
	}
	app, err := runtime.New(runtime.Config{SessionDir: sessionDir, Model: model, APIKey: apiKey, AuthPath: authPath, Output: os.Stdout})
	if err != nil {
		fail(err.Error(), 1)
	}
	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()
	if continueSession {
		err = app.Continue(ctx)
	} else {
		err = app.Prompt(ctx, flag.Arg(0))
	}
	if err != nil {
		fail(err.Error(), 1)
	}
}

func configuredModel(providerOverride, modelOverride string) (types.Model, string, string, error) {
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
	model, _ := resolved["defaultModel"].(string)
	if providerOverride != "" {
		provider = providerOverride
	}
	if modelOverride != "" {
		model = modelOverride
	}
	if provider == "" || model == "" {
		available := registry.All()
		if len(available) != 1 {
			return types.Model{}, "", "", errors.New("select a configured defaultProvider/defaultModel or pass --provider and --model")
		}
		provider, model = available[0].Provider, available[0].ID
	}
	selected, exists := registry.Find(provider, model)
	if !exists {
		return types.Model{}, "", "", fmt.Errorf("configured model %s/%s was not found", provider, model)
	}
	key, _ := registry.APIKey(provider)
	return selected, key, filepath.Join(roots.Global(), "auth.json"), nil
}

func fail(message string, code int) {
	_, _ = fmt.Fprintln(os.Stderr, message)
	os.Exit(code)
}
