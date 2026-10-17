package server

import (
	"bytes"
	"encoding/json"
	"net"
	"path/filepath"
	"testing"
	"time"

	"go.lsp.dev/jsonrpc2"
	"rozsa/backend/internal/app/productsettings"
	"rozsa/backend/internal/app/session"
	"rozsa/backend/internal/app/settings"
	"rozsa/backend/internal/app/themes"
)

func settingsExchange(t *testing.T, service productsettings.Service, requests ...string) []json.RawMessage {
	t.Helper()
	client, backend := net.Pipe()
	t.Cleanup(func() { _ = client.Close(); _ = backend.Close() })
	_ = client.SetDeadline(time.Now().Add(5 * time.Second))
	ended := make(chan error, 1)
	go func() {
		ended <- serveProduct(backend, backend, session.NewService(session.Catalog{}), nil, []productsettings.Service{service}, requestTimeout)
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

func TestSettingsMethodsUseProductOwnerAndReturnLayeredThemeSnapshot(t *testing.T) {
	root := t.TempDir()
	global := filepath.Join(root, "global", "settings.json")
	project := filepath.Join(root, "project", "settings.json")
	manager, err := settings.Load(global, &project, nil)
	if err != nil {
		t.Fatal(err)
	}
	service := productsettings.New(manager, themes.NewStore(filepath.Join(root, "global", "themes"), filepath.Join(root, "project", "themes")))
	responses := settingsExchange(t, service,
		initRequest,
		`{"jsonrpc":"2.0","id":2,"method":"get_settings","params":{"scope":"project"}}`,
		`{"jsonrpc":"2.0","id":3,"method":"update_setting","params":{"scope":"project","key":"appearance_font_size","value":18}}`,
		`{"jsonrpc":"2.0","id":4,"method":"get_settings","params":{"scope":"project"}}`,
		`{"jsonrpc":"2.0","id":5,"method":"reset_setting","params":{"scope":"project","key":"appearance_font_size"}}`,
		`{"jsonrpc":"2.0","id":6,"method":"update_setting","params":{"scope":"global","key":"appearance_theme_mode","value":"sepia"}}`,
		`{"jsonrpc":"2.0","id":7,"method":"shutdown","params":{}}`)
	if len(responses) != 7 {
		t.Fatalf("responses: %s", responses)
	}
	if !bytes.Contains(responses[0], []byte(`"get_settings"`)) || !bytes.Contains(responses[0], []byte(`"save_theme"`)) {
		t.Fatalf("settings methods absent from handshake: %s", responses[0])
	}
	if !bytes.Contains(responses[1], []byte(`"themeMode":"system"`)) || !bytes.Contains(responses[1], []byte(`"scope":"project"`)) {
		t.Fatalf("invalid initial snapshot: %s", responses[1])
	}
	if !bytes.Contains(responses[2], []byte(`"fontSize":18`)) {
		t.Fatalf("update result missing persisted appearance: %s", responses[2])
	}
	if !bytes.Contains(responses[3], []byte(`"fontSize":18`)) {
		t.Fatalf("settings read missed write: %s", responses[3])
	}
	if !bytes.Contains(responses[4], []byte(`"fontSize":14`)) {
		t.Fatalf("reset did not inherit default: %s", responses[4])
	}
	if !bytes.Contains(responses[5], []byte(`"kind":"settings"`)) {
		t.Fatalf("invalid setting error owner: %s", responses[5])
	}
	if !bytes.Contains(responses[6], []byte(`"result":true`)) {
		t.Fatalf("shutdown failed: %s", responses[6])
	}
}
