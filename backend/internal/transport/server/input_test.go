package server_test

import (
	"context"
	"encoding/json"
	"net"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"go.lsp.dev/jsonrpc2"
	"rozsa/backend/internal/app/productsettings"
	"rozsa/backend/internal/app/session"
	"rozsa/backend/internal/app/settings"
	"rozsa/backend/internal/app/skills"
	"rozsa/backend/internal/app/themes"
	"rozsa/backend/internal/transport/server"
)

type inputSessionFixture struct{ state session.StateSnapshot }

func (fixture inputSessionFixture) GetSessions(context.Context) ([]session.ListEntry, error) {
	return []session.ListEntry{}, nil
}
func (fixture inputSessionFixture) Switch(_ context.Context, _, _ string) (session.LoadedSnapshot, error) {
	if fixture.state.Active == nil {
		return session.LoadedSnapshot{}, nil
	}
	return *fixture.state.Active, nil
}
func (inputSessionFixture) Subscribe() session.EventSubscription { return nil }
func (fixture inputSessionFixture) Snapshot(context.Context) (session.StateSnapshot, error) {
	return fixture.state, nil
}

type inputSkillCatalog struct{ cwd string }

func (catalog *inputSkillCatalog) Enabled(string, map[string]any) (skills.LoadResult, error) {
	return skills.LoadResult{}, nil
}
func (catalog *inputSkillCatalog) EnabledLocal(cwd string, _ map[string]any) (skills.LoadResult, error) {
	catalog.cwd = cwd
	return skills.LoadResult{Skills: []skills.Skill{{Name: "deploy", Description: "Deploy the current project"}}}, nil
}
func (catalog *inputSkillCatalog) Catalog(cwd string) (skills.CatalogSnapshot, error) {
	catalog.cwd = cwd
	return skills.CatalogSnapshot{GlobalSkills: []skills.CatalogItem{{Name: "deploy", Description: "Deploy the current project", Effective: true}}}, nil
}
func (catalog *inputSkillCatalog) SetEnabled(string, settings.Scope, string, *bool) (skills.CatalogSnapshot, error) {
	return skills.CatalogSnapshot{}, nil
}

func inputRPCExchange(t *testing.T, app session.ApplicationService, service productsettings.Service, requests ...string) []json.RawMessage {
	t.Helper()
	client, backend := net.Pipe()
	t.Cleanup(func() { _ = client.Close(); _ = backend.Close() })
	_ = client.SetDeadline(time.Now().Add(5 * time.Second))
	ended := make(chan error, 1)
	go func() {
		ended <- server.ServeProductApplication(backend, backend, app, nil, service)
		_ = backend.Close()
	}()
	frames := jsonrpc2.NewStream(client).(rawFrames)
	responses := make([]json.RawMessage, 0, len(requests))
	for _, request := range requests {
		if _, err := frames.WriteFrame(t.Context(), []byte(request)); err != nil {
			t.Fatal(err)
		}
		message, err := jsonrpc2.DecodeMessage([]byte(request))
		if err != nil {
			t.Fatal(err)
		}
		if _, call := message.(*jsonrpc2.Call); call {
			body, _, err := frames.ReadFrame(t.Context())
			if err != nil {
				t.Fatal(err)
			}
			responses = append(responses, append(json.RawMessage(nil), body...))
		}
	}
	_ = client.Close()
	if err := <-ended; err != nil {
		t.Fatalf("server exit: %v", err)
	}
	return responses
}

func TestAutocompleteInputUsesEffectiveSkillsForActiveSessionCWD(t *testing.T) {
	root := t.TempDir()
	cwd := filepath.Join(root, "active-workspace")
	if err := os.MkdirAll(cwd, 0o700); err != nil {
		t.Fatal(err)
	}
	globalPath := filepath.Join(root, "global", "settings.json")
	projectPath := filepath.Join(root, "project", "settings.json")
	manager, err := settings.Load(globalPath, &projectPath, nil)
	if err != nil {
		t.Fatal(err)
	}
	catalog := &inputSkillCatalog{}
	product := productsettings.NewWithModelCatalogAndSkills(manager,
		themes.NewStore(filepath.Join(root, "global", "themes"), filepath.Join(root, "project", "themes")), nil, nil, catalog, filepath.Join(root, "startup-cwd"))
	app := inputSessionFixture{state: session.StateSnapshot{Epoch: "epoch", Active: &session.LoadedSnapshot{SessionID: "active", Cwd: cwd}}}
	responses := inputRPCExchange(t, app, product,
		initRequest,
		`{"jsonrpc":"2.0","id":2,"method":"autocomplete_input","params":{"text":"/depl","cursor":5}}`,
		`{"jsonrpc":"2.0","id":3,"method":"autocomplete_input","params":{"text":"/skill:deploy ","cursor":14}}`,
		`{"jsonrpc":"2.0","id":4,"method":"shutdown","params":{}}`)
	if len(responses) != 4 || !bytesContain(responses[1], `"value":"/skill:deploy "`) {
		t.Fatalf("effective skill suggestion missing: %s", responses)
	}
	if !bytesContain(responses[2], `"highlightRanges":[{"start":0,"end":13}]`) {
		t.Fatalf("completed skill command was not highlighted after the caret: %s", responses[2])
	}
	if catalog.cwd != cwd {
		t.Fatalf("skill catalog cwd = %q, want active session cwd %q", catalog.cwd, cwd)
	}
}

func TestAutocompleteInputUsesActiveSessionCWDOverProductRPC(t *testing.T) {
	root := t.TempDir()
	cwd := filepath.Join(root, "workspace")
	if err := os.MkdirAll(cwd, 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(cwd, "document.md"), []byte("fixture"), 0o600); err != nil {
		t.Fatal(err)
	}
	globalPath := filepath.Join(root, "global", "settings.json")
	projectPath := filepath.Join(root, "project", "settings.json")
	manager, err := settings.Load(globalPath, &projectPath, nil)
	if err != nil {
		t.Fatal(err)
	}
	product := productsettings.New(manager, themes.NewStore(filepath.Join(root, "global", "themes"), filepath.Join(root, "project", "themes")))
	app := inputSessionFixture{state: session.StateSnapshot{Epoch: "epoch", Active: &session.LoadedSnapshot{SessionID: "active", Cwd: cwd}}}
	responses := inputRPCExchange(t, app, product,
		initRequest,
		`{"jsonrpc":"2.0","id":2,"method":"autocomplete_input","params":{"text":"@doc","cursor":4}}`,
		`{"jsonrpc":"2.0","id":3,"method":"shutdown","params":{}}`)
	if len(responses) != 3 {
		t.Fatalf("responses: %s", responses)
	}
	if !bytesContain(responses[0], `"autocomplete_input"`) {
		t.Fatalf("autocomplete capability missing from handshake: %s", responses[0])
	}
	if !bytesContain(responses[1], `"prefix":"@doc"`) || !bytesContain(responses[1], `"value":"@document.md "`) {
		t.Fatalf("active-session file completion missing from response: %s", responses[1])
	}
	if !bytesContain(responses[2], `"result":true`) {
		t.Fatalf("shutdown failed: %s", responses[2])
	}
}

func TestAutocompleteInputRejectsMissingSessionAndInvalidCursor(t *testing.T) {
	root := t.TempDir()
	globalPath := filepath.Join(root, "global", "settings.json")
	projectPath := filepath.Join(root, "project", "settings.json")
	manager, err := settings.Load(globalPath, &projectPath, nil)
	if err != nil {
		t.Fatal(err)
	}
	product := productsettings.New(manager, themes.NewStore(filepath.Join(root, "global", "themes"), filepath.Join(root, "project", "themes")))
	app := inputSessionFixture{state: session.StateSnapshot{Epoch: "epoch"}}
	responses := inputRPCExchange(t, app, product,
		initRequest,
		`{"jsonrpc":"2.0","id":2,"method":"autocomplete_input","params":{"text":"@doc","cursor":4}}`,
		`{"jsonrpc":"2.0","id":3,"method":"autocomplete_input","params":{"text":"@doc","cursor":-1}}`,
		`{"jsonrpc":"2.0","id":4,"method":"shutdown","params":{}}`)
	if len(responses) != 4 {
		t.Fatalf("responses: %s", responses)
	}
	if !bytesContain(responses[1], `"kind":"session"`) {
		t.Fatalf("missing session error was hidden: %s", responses[1])
	}
	if !bytesContain(responses[2], `"kind":"invalid_params"`) {
		t.Fatalf("negative cursor was accepted: %s", responses[2])
	}
}

func bytesContain(data []byte, fragment string) bool {
	return json.Valid(data) && strings.Contains(string(data), fragment)
}
