// Package common provides provider-independent OAuth primitives.
package common

import (
	"context"
	"crypto/rand"
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"errors"
	"fmt"
	"net"
	"net/http"
	"net/url"
	"strings"
	"time"
)

type PKCE struct {
	Verifier  string
	Challenge string
}

func GeneratePKCE() (PKCE, error) {
	bytes := make([]byte, 32)
	if _, err := rand.Read(bytes); err != nil {
		return PKCE{}, fmt.Errorf("generate PKCE verifier: %w", err)
	}
	verifier := base64.RawURLEncoding.EncodeToString(bytes)
	hash := sha256.Sum256([]byte(verifier))
	return PKCE{Verifier: verifier, Challenge: base64.RawURLEncoding.EncodeToString(hash[:])}, nil
}

func GenerateState() (string, error) {
	bytes := make([]byte, 16)
	if _, err := rand.Read(bytes); err != nil {
		return "", fmt.Errorf("generate OAuth state: %w", err)
	}
	return hex.EncodeToString(bytes), nil
}

type AuthorizationCode struct {
	Code  string
	State string
}

// ParseAuthorizationInput accepts provider redirect URLs, query strings,
// code#state input, and a manually pasted raw code for legacy CLI flows.
func ParseAuthorizationInput(input, expectedState string) (AuthorizationCode, error) {
	input = strings.TrimSpace(input)
	if input == "" {
		return AuthorizationCode{}, errors.New("authorization code is empty")
	}
	if strings.Contains(input, "://") {
		parsed, err := url.Parse(input)
		if err != nil {
			return AuthorizationCode{}, fmt.Errorf("parse authorization callback URL: %w", err)
		}
		values := parsed.Query()
		if parsed.Fragment != "" {
			fragment, _ := url.ParseQuery(parsed.Fragment)
			for key, items := range fragment {
				if _, exists := values[key]; !exists {
					values[key] = items
				}
			}
		}
		return validateCallbackValues(values, expectedState)
	}
	if strings.Contains(input, "=") {
		values, err := url.ParseQuery(strings.TrimPrefix(input, "?"))
		if err != nil {
			return AuthorizationCode{}, fmt.Errorf("parse authorization callback parameters: %w", err)
		}
		return validateCallbackValues(values, expectedState)
	}
	if code, state, found := strings.Cut(input, "#"); found {
		return validateCallback(code, state, expectedState)
	}
	return AuthorizationCode{Code: input, State: expectedState}, nil
}

func validateCallbackValues(values url.Values, expectedState string) (AuthorizationCode, error) {
	if providerError := values.Get("error"); providerError != "" {
		return AuthorizationCode{}, fmt.Errorf("OAuth provider error: %s", providerError)
	}
	return validateCallback(values.Get("code"), values.Get("state"), expectedState)
}
func validateCallback(code, state, expectedState string) (AuthorizationCode, error) {
	if code == "" {
		return AuthorizationCode{}, errors.New("OAuth callback is missing code")
	}
	if state == "" {
		return AuthorizationCode{}, errors.New("OAuth callback is missing state")
	}
	if state != expectedState {
		return AuthorizationCode{}, errors.New("OAuth callback state mismatch")
	}
	return AuthorizationCode{Code: code, State: state}, nil
}

// WaitForCallback binds only to the loopback interface and returns a code only
// after the callback's state matches this flow. Cancellation closes the server.
func WaitForCallback(ctx context.Context, address, expectedState string) (AuthorizationCode, error) {
	listener, err := net.Listen("tcp", address)
	if err != nil {
		return AuthorizationCode{}, fmt.Errorf("bind OAuth callback listener: %w", err)
	}
	if host, _, splitErr := net.SplitHostPort(listener.Addr().String()); splitErr != nil || net.ParseIP(host) == nil || !net.ParseIP(host).IsLoopback() {
		listener.Close()
		return AuthorizationCode{}, errors.New("OAuth callback listener must bind to loopback")
	}
	return serveCallback(ctx, listener, expectedState)
}

func serveCallback(ctx context.Context, listener net.Listener, expectedState string) (AuthorizationCode, error) {
	result := make(chan AuthorizationCode, 1)
	failure := make(chan error, 1)
	mux := http.NewServeMux()
	mux.HandleFunc("/callback", func(writer http.ResponseWriter, request *http.Request) {
		if request.Method != http.MethodGet {
			http.Error(writer, "method not allowed", http.StatusMethodNotAllowed)
			return
		}
		code, parseErr := validateCallbackValues(request.URL.Query(), expectedState)
		if parseErr != nil {
			http.Error(writer, "OAuth callback rejected", http.StatusBadRequest)
			if flusher, ok := writer.(http.Flusher); ok {
				flusher.Flush()
			}
			select {
			case failure <- parseErr:
			default:
			}
			return
		}
		writer.Header().Set("Content-Type", "text/html; charset=utf-8")
		_, _ = writer.Write([]byte("<!doctype html><title>Login complete</title><p>Login complete. You can close this tab.</p>"))
		if flusher, ok := writer.(http.Flusher); ok {
			flusher.Flush()
		}
		select {
		case result <- code:
		default:
		}
	})
	server := &http.Server{Handler: mux, ReadHeaderTimeout: 5 * time.Second}
	go func() {
		if serveErr := server.Serve(listener); serveErr != nil && !errors.Is(serveErr, http.ErrServerClosed) {
			select {
			case failure <- serveErr:
			default:
			}
		}
	}()
	defer server.Close()
	select {
	case code := <-result:
		return code, nil
	case err := <-failure:
		return AuthorizationCode{}, err
	case <-ctx.Done():
		return AuthorizationCode{}, ctx.Err()
	}
}

type DeviceResultKind uint8

const (
	DevicePending DeviceResultKind = iota
	DeviceComplete
	DeviceSlowDown
	DeviceFailure
)

type DeviceResult struct {
	Kind        DeviceResultKind
	AccessToken string
	Err         error
}
type DevicePoll func(context.Context) DeviceResult
type WaitFunc func(context.Context, time.Duration) error

// PollDeviceCode follows RFC 8628 timing: first poll after interval, slow_down
// adds five seconds, expiry and cancellation stop without returning a token.
func PollDeviceCode(ctx context.Context, interval, expires time.Duration, poll DevicePoll, wait WaitFunc) (string, error) {
	if poll == nil {
		return "", errors.New("device code poll function is required")
	}
	if interval <= 0 {
		interval = 5 * time.Second
	}
	if expires <= 0 {
		return "", errors.New("device code expiry must be positive")
	}
	if wait == nil {
		wait = timerWait
	}
	deadline := time.Now().Add(expires)
	for {
		remaining := time.Until(deadline)
		if remaining <= 0 {
			return "", errors.New("device authorization expired")
		}
		delay := interval
		if delay > remaining {
			delay = remaining
		}
		if err := wait(ctx, delay); err != nil {
			return "", err
		}
		if err := ctx.Err(); err != nil {
			return "", err
		}
		if time.Until(deadline) <= 0 {
			return "", errors.New("device authorization expired")
		}
		result := poll(ctx)
		switch result.Kind {
		case DeviceComplete:
			if result.AccessToken == "" {
				return "", errors.New("device authorization returned an empty access token")
			}
			return result.AccessToken, nil
		case DevicePending:
		case DeviceSlowDown:
			interval += 5 * time.Second
		case DeviceFailure:
			if result.Err != nil {
				return "", result.Err
			}
			return "", errors.New("device authorization failed")
		default:
			return "", errors.New("device authorization returned an unknown result")
		}
	}
}

func timerWait(ctx context.Context, duration time.Duration) error {
	timer := time.NewTimer(duration)
	defer timer.Stop()
	select {
	case <-timer.C:
		return nil
	case <-ctx.Done():
		return ctx.Err()
	}
}
