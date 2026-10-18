package anthropic

import (
	"context"
	"encoding/json"
	"net"
	"net/http"
	"net/http/httptest"
	"net/url"
	"strings"
	"testing"
	"time"

	"rozsa/backend/internal/model/credentials"
)

func TestRefreshUsesAnthropicJSONContractAndRetainsRefreshToken(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		if request.Method != http.MethodPost || request.Header.Get("Content-Type") != "application/json" {
			t.Errorf("unexpected request: %s %s content-type=%q", request.Method, request.URL, request.Header.Get("Content-Type"))
		}
		var body map[string]string
		if err := json.NewDecoder(request.Body).Decode(&body); err != nil {
			t.Errorf("decode request: %v", err)
		}
		if body["grant_type"] != "refresh_token" || body["refresh_token"] != "old-refresh" || body["client_id"] != clientID {
			t.Errorf("unexpected refresh request fields: %#v", body)
		}
		writer.Header().Set("Content-Type", "application/json")
		_, _ = writer.Write([]byte(`{"access_token":"new-access","expires_in":3600}`))
	}))
	defer server.Close()

	client := NewClientWithConfig(server.Client(), Config{TokenURL: server.URL})
	updated, err := client.Refresh(context.Background(), credentials.OAuthCredential{Type: "oauth", Access: "old-access", Refresh: "old-refresh"})
	if err != nil {
		t.Fatal(err)
	}
	if updated.Access != "new-access" || updated.Refresh != "old-refresh" || updated.Type != "oauth" {
		t.Fatalf("unexpected refreshed credential: %#v", updated)
	}
	if time.Until(time.UnixMilli(updated.Expires)) < 54*time.Minute {
		t.Fatalf("refresh expiry did not retain the five-minute safety margin: %d", updated.Expires)
	}
}

func TestLoginBuildsPKCERequestFromTypedManualInput(t *testing.T) {
	requestReceived := make(chan map[string]string, 1)
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		var body map[string]string
		if err := json.NewDecoder(request.Body).Decode(&body); err != nil {
			t.Errorf("decode token request: %v", err)
		}
		requestReceived <- body
		writer.Header().Set("Content-Type", "application/json")
		_, _ = writer.Write([]byte(`{"access_token":"access","refresh_token":"refresh","expires_in":3600}`))
	}))
	defer server.Close()
	client := NewClientWithConfig(server.Client(), Config{
		AuthorizationURL: "https://auth.example.test/oauth/authorize",
		TokenURL:         server.URL,
		RedirectURI:      "http://127.0.0.1:53692/callback",
		CallbackAddress:  "127.0.0.1:0",
	})
	events := make(chan Event, 2)
	inputs := make(chan Input, 1)
	result := make(chan error, 1)
	go func() {
		_, err := client.Login(context.Background(), events, inputs)
		result <- err
	}()
	authorization := <-events
	if authorization.Kind != EventAuthorizationURL || strings.Contains(authorization.URL, "access_token") {
		t.Fatalf("unexpected authorization event: %#v", authorization)
	}
	parsedURL, err := url.Parse(authorization.URL)
	if err != nil {
		t.Fatal(err)
	}
	query := parsedURL.Query()
	if query.Get("response_type") != "code" || query.Get("code_challenge_method") != "S256" || query.Get("redirect_uri") != "http://127.0.0.1:53692/callback" {
		t.Fatalf("authorization URL lost required fields: %s", authorization.URL)
	}
	if len(query.Get("code_challenge")) != 43 || query.Get("state") == "" {
		t.Fatalf("authorization URL lacks PKCE/state: %s", authorization.URL)
	}
	inputs <- Input{Kind: InputAuthorizationResponse, Value: "manual-code#" + query.Get("state")}
	if err := <-result; err != nil {
		t.Fatal(err)
	}
	body := <-requestReceived
	if body["code"] != "manual-code" || body["grant_type"] != "authorization_code" || body["redirect_uri"] != query.Get("redirect_uri") {
		t.Fatalf("unexpected code exchange fields: %#v", body)
	}
	if len(body["code_verifier"]) != 43 || body["client_id"] != clientID {
		t.Fatalf("missing PKCE verifier/client id: %#v", body)
	}
}

func TestLoginAcceptsValidatedLoopbackCallback(t *testing.T) {
	requestReceived := make(chan map[string]string, 1)
	tokenServer := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		var body map[string]string
		if err := json.NewDecoder(request.Body).Decode(&body); err != nil {
			t.Errorf("decode token request: %v", err)
		}
		requestReceived <- body
		writer.Header().Set("Content-Type", "application/json")
		_, _ = writer.Write([]byte(`{"access_token":"access","refresh_token":"refresh","expires_in":3600}`))
	}))
	defer tokenServer.Close()
	reservation, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	callbackAddress := reservation.Addr().String()
	if err := reservation.Close(); err != nil {
		t.Fatal(err)
	}
	client := NewClientWithConfig(tokenServer.Client(), Config{
		TokenURL:        tokenServer.URL,
		RedirectURI:     "http://" + callbackAddress + "/callback",
		CallbackAddress: callbackAddress,
	})
	events := make(chan Event, 2)
	result := make(chan error, 1)
	go func() {
		_, loginErr := client.Login(context.Background(), events, nil)
		result <- loginErr
	}()
	authorization := <-events
	state, err := AuthorizationState(authorization.URL)
	if err != nil {
		t.Fatal(err)
	}
	callbackURL := "http://" + callbackAddress + "/callback?code=callback-code&state=" + url.QueryEscape(state)
	var response *http.Response
	for attempt := 0; attempt < 50; attempt++ {
		response, err = http.Get(callbackURL)
		if err == nil {
			break
		}
		time.Sleep(10 * time.Millisecond)
	}
	if err != nil {
		t.Fatalf("send loopback callback: %v", err)
	}
	response.Body.Close()
	if response.StatusCode != http.StatusOK {
		t.Fatalf("callback status = %d", response.StatusCode)
	}
	if err := <-result; err != nil {
		t.Fatal(err)
	}
	if body := <-requestReceived; body["code"] != "callback-code" {
		t.Fatalf("callback code exchanged = %#v", body)
	}
}

func TestLoginRejectsWrongStateAndHonorsCancellation(t *testing.T) {
	client := NewClientWithConfig(nil, Config{CallbackAddress: "127.0.0.1:0"})
	inputs := make(chan Input, 1)
	events := make(chan Event, 1)
	inputs <- Input{Kind: InputAuthorizationResponse, Value: "code=bad&state=wrong"}
	_, err := client.Login(context.Background(), events, inputs)
	if err == nil || !strings.Contains(err.Error(), "state mismatch") {
		t.Fatalf("wrong state error = %v", err)
	}

	cancelled, stop := context.WithCancel(context.Background())
	defer stop()
	go func() {
		<-events
		stop()
	}()
	_, err = client.Login(cancelled, events, make(chan Input))
	if err == nil || err != context.Canceled {
		t.Fatalf("cancelled login error = %v", err)
	}
}

func TestRefreshErrorDoesNotEchoResponseBody(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, _ *http.Request) {
		writer.WriteHeader(http.StatusUnauthorized)
		_, _ = writer.Write([]byte(`{"refresh_token":"must-not-leak"}`))
	}))
	defer server.Close()
	client := NewClientWithConfig(server.Client(), Config{TokenURL: server.URL})
	_, err := client.Refresh(context.Background(), credentials.OAuthCredential{Refresh: "refresh"})
	if err == nil || strings.Contains(err.Error(), "must-not-leak") {
		t.Fatalf("unsafe refresh error: %v", err)
	}
}
