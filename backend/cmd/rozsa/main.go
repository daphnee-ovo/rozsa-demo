package main

import (
	"context"
	"flag"
	"fmt"
	"os"
	"os/signal"
	"syscall"

	"rozsa/backend/internal/app/bootstrap"
	"rozsa/backend/internal/app/runtime"
	"rozsa/backend/internal/app/settings"
	"rozsa/backend/internal/app/skills"
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
	cwd, err := os.Getwd()
	if err != nil {
		fail(err.Error(), 1)
	}
	roots, err := configpaths.Discover(cwd)
	if err != nil {
		fail(err.Error(), 1)
	}
	settingsPaths := roots.SettingsPaths()
	projectSettingsPath := settingsPaths[1]
	settingsManager, err := settings.Load(settingsPaths[0], &projectSettingsPath, nil)
	if err != nil {
		fail(err.Error(), 1)
	}
	resolveRoots := func(projectDir string) (configpaths.Roots, error) { return configpaths.Discover(projectDir) }
	skillApplication := skills.NewApplication(settingsManager, resolveRoots)
	model, configuredKey, authPath, err := bootstrap.ConfiguredModel(providerID, modelID)
	if err != nil {
		fail(err.Error(), 1)
	}
	if baseURL != "" {
		model.BaseURL = baseURL
	}
	if apiKey == "" {
		apiKey = configuredKey
	}
	app, err := runtime.New(runtime.Config{
		SessionDir: sessionDir, Model: model, APIKey: apiKey, AuthPath: authPath, Output: os.Stdout,
		ResolvedSettings: settingsManager.Resolved, Skills: skillApplication, ResolveRoots: resolveRoots,
	})
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

func fail(message string, code int) {
	_, _ = fmt.Fprintln(os.Stderr, message)
	os.Exit(code)
}
