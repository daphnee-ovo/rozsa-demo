// Package main tests use the private coordinator only for the approved setup in
// TestRuntimeCoordinatorServesSessionsAndAdoptsSelectionBeforeReady. Other
// process behavior tests use package main_test and the built product executable.
// The public process interface cannot pause initialization before it completes,
// so it cannot deterministically expose the pre-ready selection window.
package main

import (
	"context"
	"os"
	"path/filepath"
	goruntime "runtime"
	"strings"
	"testing"

	appruntime "rozsa/backend/internal/app/runtime"
	"rozsa/backend/internal/app/session"
	"rozsa/backend/internal/model/types"
)

type coordinatorTestProvider struct{}

func (coordinatorTestProvider) StreamSimple(context.Context, types.Model, types.Context, types.SimpleStreamOptions, func(types.StreamEvent)) error {
	return nil
}

func useProjectTestCWD(t *testing.T) string {
	t.Helper()
	previous, err := os.Getwd()
	if err != nil {
		t.Fatal(err)
	}
	_, sourceFile, _, ok := goruntime.Caller(0)
	if !ok {
		t.Fatal("could not locate coordinator test source")
	}
	repositoryRoot := filepath.Clean(filepath.Join(filepath.Dir(sourceFile), "../../.."))
	name := strings.NewReplacer("/", "-", " ", "-", ":", "-").Replace(t.Name())
	workingDirectory := filepath.Join(repositoryRoot, "testdata", "project-test-dirs", name)
	if err := os.Chdir(workingDirectory); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() {
		if err := os.Chdir(previous); err != nil {
			t.Error(err)
		}
	})
	return workingDirectory
}

func TestRuntimeCoordinatorServesSessionsAndAdoptsSelectionBeforeReady(t *testing.T) {
	// Prevent a regression where selection made before runtime readiness is lost
	// when initialization completes. Public process startup cannot deterministically
	// hold initialization between the selection and completion, so this test uses
	// only newRuntimeCoordinator and complete for setup and asserts public results.
	workingDirectory := useProjectTestCWD(t)
	sessionDirectory := filepath.Join(workingDirectory, "sessions")
	selectedPath := filepath.Join(sessionDirectory, "selected.jsonl")
	otherPath := filepath.Join(sessionDirectory, "other.jsonl")
	sessions := session.NewService(session.Catalog{Directories: []string{sessionDirectory}})
	// Private setup is required because process startup cannot guarantee that
	// initialization remains pending until after a session selection.
	coordinator := newRuntimeCoordinator(sessions)
	entries, err := coordinator.GetSessions(t.Context())
	if err != nil || len(entries) != 2 {
		t.Fatalf("session catalog before runtime readiness = %d entries, err=%v", len(entries), err)
	}
	selected, err := coordinator.Switch(t.Context(), selectedPath, "selected")
	if err != nil || selected.Path != selectedPath {
		t.Fatalf("switch before runtime readiness = %+v err=%v", selected, err)
	}
	if status := coordinator.RuntimeStatus(); status.State != "initializing" {
		t.Fatalf("status before completion = %+v", status)
	}
	app, err := appruntime.New(appruntime.Config{
		ProjectRoot: workingDirectory, SessionDir: sessionDirectory, SessionDirs: []string{sessionDirectory}, SessionService: sessions,
		Model: types.Model{ID: "test", Provider: "test", API: types.APIAnthropicMessages}, Provider: coordinatorTestProvider{},
	})
	if err != nil {
		t.Fatal(err)
	}
	// Deliver initialization only after selection so the assertion checks whether
	// the runtime adopts the earlier public session selection.
	coordinator.complete(app, nil)
	if status := coordinator.RuntimeStatus(); status.State != "ready" {
		t.Fatalf("status after completion = %+v", status)
	}
	active, err := app.Snapshot(t.Context())
	if err != nil || active.Active == nil || active.Active.Path != selectedPath {
		t.Fatalf("runtime active session after initialization = %+v err=%v, want %s", active.Active, err, selectedPath)
	}
	if _, err := coordinator.Switch(t.Context(), otherPath, "other"); err != nil {
		t.Fatal(err)
	}
	shared, err := coordinator.Snapshot(t.Context())
	if err != nil || shared.Active == nil || shared.Active.Path != otherPath {
		t.Fatalf("runtime session switch was not published through the early session service: %+v err=%v", shared.Active, err)
	}
}
