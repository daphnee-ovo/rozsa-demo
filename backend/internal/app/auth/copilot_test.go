package auth

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"testing"
	"time"

	"rozsa/backend/internal/model/credentials"
	"rozsa/backend/internal/model/oauth/copilot"
)

func TestCopilotLoginStoresOnlyProviderAndLogoutRemovesIt(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		switch request.URL.Path {
		case "/login/device/code":
			_, _ = writer.Write([]byte(`{"device_code":"device","user_code":"CODE","verification_uri":"https://github.test/device","interval":1,"expires_in":60}`))
		case "/login/oauth/access_token":
			_, _ = writer.Write([]byte(`{"access_token":"github-token"}`))
		case "/copilot_internal/v2/token":
			_, _ = writer.Write([]byte(`{"token":"copilot-token","expires_at":4102444800}`))
		default:
			writer.WriteHeader(http.StatusNoContent)
		}
	}))
	defer server.Close()

	path := filepath.Join(t.TempDir(), "models", "auth.json")
	if err := credentials.StoreOAuth(path, "anthropic", credentials.OAuthCredential{Type: "oauth", Access: "other-access", Refresh: "other-refresh", Expires: time.Now().Add(time.Hour).UnixMilli()}); err != nil {
		t.Fatal(err)
	}
	client := copilot.NewClient(server.Client())
	client.GitHubBaseURL, client.APIBaseURL = server.URL, server.URL
	client.Wait = func(context.Context, time.Duration) error { return nil }
	var changed []string
	service := Copilot{Client: client, AuthPath: path, OnModelsChanged: func(_ context.Context, provider string) error { changed = append(changed, provider); return nil }}
	if _, err := service.Login(context.Background(), copilot.LoginInput{}, nil); err != nil {
		t.Fatal(err)
	}
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	var authFile map[string]json.RawMessage
	if err := json.Unmarshal(data, &authFile); err != nil {
		t.Fatal(err)
	}
	if len(authFile) != 2 || len(changed) != 1 || changed[0] != CopilotProvider {
		t.Fatalf("providers=%v model changes=%v", keys(authFile), changed)
	}
	var stored credentials.OAuthCredential
	if err := json.Unmarshal(authFile[CopilotProvider], &stored); err != nil {
		t.Fatal(err)
	}
	if stored.Access != "copilot-token" || stored.Refresh != "github-token" {
		t.Fatalf("stored credential = %#v", stored)
	}
	refreshed, err := service.Refresh(context.Background(), stored)
	if err != nil {
		t.Fatal(err)
	}
	if refreshed.Access != "copilot-token" || len(changed) != 2 || changed[1] != CopilotProvider {
		t.Fatalf("refreshed credential=%#v model changes=%v", refreshed, changed)
	}
	removed, err := service.Logout(context.Background())
	if err != nil || !removed {
		t.Fatalf("logout = %v, %v", removed, err)
	}
	data, err = os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if string(data) == "" || containsProvider(data, CopilotProvider) || !containsProvider(data, "anthropic") {
		t.Fatalf("auth.json after logout = %s", data)
	}
	if len(changed) != 3 || changed[2] != CopilotProvider {
		t.Fatalf("model changes after logout = %v", changed)
	}
}

func keys(values map[string]json.RawMessage) []string {
	result := make([]string, 0, len(values))
	for key := range values {
		result = append(result, key)
	}
	return result
}
func containsProvider(data []byte, provider string) bool {
	var values map[string]json.RawMessage
	if json.Unmarshal(data, &values) != nil {
		return false
	}
	_, exists := values[provider]
	return exists
}
