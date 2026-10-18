package copilot

import (
	"context"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"rozsa/backend/internal/model/credentials"
)

func TestLoginDeviceFlowExchangesTokenAndPreservesEnterpriseDomain(t *testing.T) {
	var server *httptest.Server
	var calls []string
	server = httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		calls = append(calls, request.Method+" "+request.URL.Path)
		switch request.URL.Path {
		case "/login/device/code":
			_, _ = writer.Write([]byte(`{"device_code":"device-secret","user_code":"ABCD-EFGH","verification_uri":"https://github.test/login/device","interval":1,"expires_in":60}`))
		case "/login/oauth/access_token":
			_, _ = writer.Write([]byte(`{"access_token":"github-secret"}`))
		case "/copilot_internal/v2/token":
			if request.Header.Get("Authorization") != "Bearer github-secret" {
				t.Errorf("authorization = %q", request.Header.Get("Authorization"))
			}
			if request.Header.Get("User-Agent") != providerUserAgent {
				t.Errorf("login User-Agent = %q", request.Header.Get("User-Agent"))
			}
			_, _ = writer.Write([]byte(`{"token":"copilot-secret","expires_at":4102444800}`))
		default:
			if request.Method != http.MethodPost || !strings.HasSuffix(request.URL.Path, "/policy") {
				t.Errorf("unexpected request %s %s", request.Method, request.URL.Path)
			}
			writer.WriteHeader(http.StatusForbidden)
		}
	}))
	defer server.Close()

	client := NewClient(server.Client())
	client.GitHubBaseURL, client.APIBaseURL = server.URL, server.URL
	client.PollInterval = time.Millisecond
	client.Wait = func(context.Context, time.Duration) error { return nil }
	var events []Event
	credential, err := client.Login(context.Background(), LoginInput{EnterpriseURL: "https://github.test/"}, func(event Event) error {
		events = append(events, event)
		return nil
	})
	if err != nil {
		t.Fatal(err)
	}
	if credential.Access != "copilot-secret" || credential.Refresh != "github-secret" || credential.Expires != 4102444800*1000-5*60*1000 {
		t.Fatalf("credential = %#v", credential)
	}
	if credential.Extra["enterpriseUrl"] != "github.test" {
		t.Fatalf("enterpriseUrl = %#v", credential.Extra["enterpriseUrl"])
	}
	if strings.Contains(strings.Join(calls, " "), "device-secret") || strings.Contains(strings.Join(calls, " "), "github-secret") {
		t.Fatal("request path leaked a token")
	}
	if len(events) < 3 || events[0].Kind != EventPrompt || events[1].Kind != EventDeviceCode {
		t.Fatalf("events = %#v", events)
	}
}

func TestRefreshRetainsEnterpriseMetadata(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		if request.URL.Path != "/copilot_internal/v2/token" || request.Header.Get("Authorization") != "Bearer github-secret" {
			t.Errorf("refresh request = %s %s auth=%q", request.Method, request.URL.Path, request.Header.Get("Authorization"))
		}
		if request.Header.Get("User-Agent") != "GitHubCopilotChat/0.35.0" || request.Header.Get("Editor-Version") != "vscode/1.107.0" {
			t.Errorf("refresh headers = %#v", request.Header)
		}
		_, _ = writer.Write([]byte(`{"token":"renewed","expires_at":4102444800}`))
	}))
	defer server.Close()
	client := NewClient(server.Client())
	client.APIBaseURL = server.URL
	original := credentials.OAuthCredential{Type: "oauth", Access: "expired", Refresh: "github-secret", Extra: map[string]any{"enterpriseUrl": "github.test", "unrelated": "keep"}}
	updated, err := client.Refresh(context.Background(), original)
	if err != nil {
		t.Fatal(err)
	}
	if updated.Access != "renewed" || updated.Refresh != original.Refresh || updated.Expires != 4102444800*1000-5*60*1000 || updated.Extra["enterpriseUrl"] != "github.test" || updated.Extra["unrelated"] != "keep" {
		t.Fatalf("updated credential = %#v", updated)
	}
}

func TestLoginCancellationAndProviderDenialReturnErrorsWithoutTokens(t *testing.T) {
	t.Run("cancel", func(t *testing.T) {
		server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
			_, _ = writer.Write([]byte(`{"device_code":"secret","user_code":"CODE","verification_uri":"https://github.test/device","interval":1,"expires_in":60}`))
		}))
		defer server.Close()
		client := NewClient(server.Client())
		client.GitHubBaseURL = server.URL
		client.PollInterval = time.Second
		ctx, cancel := context.WithCancel(context.Background())
		cancel()
		_, err := client.Login(ctx, LoginInput{}, nil)
		if err == nil || strings.Contains(err.Error(), "secret") {
			t.Fatalf("error = %v", err)
		}
	})
	t.Run("denied", func(t *testing.T) {
		server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
			if request.URL.Path == "/login/device/code" {
				_, _ = writer.Write([]byte(`{"device_code":"device","user_code":"CODE","verification_uri":"https://github.test/device","interval":1,"expires_in":60}`))
				return
			}
			_, _ = writer.Write([]byte(`{"error":"access_denied","error_description":"user denied authorization"}`))
		}))
		defer server.Close()
		client := NewClient(server.Client())
		client.GitHubBaseURL = server.URL
		client.PollInterval = time.Millisecond
		client.Wait = func(context.Context, time.Duration) error { return nil }
		_, err := client.Login(context.Background(), LoginInput{}, nil)
		if err == nil || !strings.Contains(err.Error(), "access_denied") {
			t.Fatalf("error = %v", err)
		}
	})
}

func TestLoginHandlesPendingAndSlowDownByIncreasingPollInterval(t *testing.T) {
	var polls int
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		switch request.URL.Path {
		case "/login/device/code":
			_, _ = writer.Write([]byte(`{"device_code":"secret","user_code":"CODE","verification_uri":"https://github.test/device","interval":1,"expires_in":60}`))
		case "/login/oauth/access_token":
			polls++
			if polls == 1 {
				_, _ = writer.Write([]byte(`{"error":"authorization_pending"}`))
			} else if polls == 2 {
				_, _ = writer.Write([]byte(`{"error":"slow_down"}`))
			} else {
				_, _ = writer.Write([]byte(`{"access_token":"github-secret"}`))
			}
		default:
			_, _ = writer.Write([]byte(`{"token":"copilot-secret","expires_at":4102444800}`))
		}
	}))
	defer server.Close()
	client := NewClient(server.Client())
	client.GitHubBaseURL, client.APIBaseURL = server.URL, server.URL
	var waits []time.Duration
	client.Wait = func(_ context.Context, duration time.Duration) error {
		waits = append(waits, duration)
		return nil
	}
	if _, err := client.Login(context.Background(), LoginInput{}, nil); err != nil {
		t.Fatal(err)
	}
	if polls != 3 || len(waits) != 3 || waits[0] != time.Second || waits[1] != time.Second || waits[2] != 6*time.Second {
		t.Fatalf("polls=%d waits=%v", polls, waits)
	}
}

func TestLoginReportsExpiredDeviceAuthorization(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		_, _ = writer.Write([]byte(`{"device_code":"secret","user_code":"CODE","verification_uri":"https://github.test/device","interval":1,"expires_in":1}`))
	}))
	defer server.Close()
	client := NewClient(server.Client())
	client.GitHubBaseURL = server.URL
	client.Wait = func(ctx context.Context, _ time.Duration) error {
		select {
		case <-ctx.Done():
			return ctx.Err()
		case <-time.After(1100 * time.Millisecond):
			return nil
		}
	}
	_, err := client.Login(context.Background(), LoginInput{}, nil)
	if err == nil || !strings.Contains(err.Error(), "expired") || strings.Contains(err.Error(), "secret") {
		t.Fatalf("error = %v", err)
	}
}
