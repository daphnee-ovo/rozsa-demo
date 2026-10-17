package common

import (
	"context"
	"crypto/sha256"
	"encoding/base64"
	"errors"
	"fmt"
	"net"
	"net/http"
	"strings"
	"testing"
	"time"
)

func TestGeneratePKCEUsesS256AndUniqueVerifier(t *testing.T) {
	first, err := GeneratePKCE()
	if err != nil {
		t.Fatal(err)
	}
	second, err := GeneratePKCE()
	if err != nil {
		t.Fatal(err)
	}
	if len(first.Verifier) != 43 || len(first.Challenge) != 43 {
		t.Fatalf("unexpected PKCE lengths: %d/%d", len(first.Verifier), len(first.Challenge))
	}
	hash := sha256.Sum256([]byte(first.Verifier))
	if first.Challenge != base64.RawURLEncoding.EncodeToString(hash[:]) {
		t.Fatal("challenge is not S256(verifier)")
	}
	if first.Verifier == second.Verifier {
		t.Fatal("verifier must be random")
	}
}

func TestAuthorizationInputBindsCallbackState(t *testing.T) {
	for _, input := range []string{
		"http://127.0.0.1:1455/callback?code=abc&state=expected",
		"code=abc&state=expected",
		"abc#expected",
	} {
		parsed, err := ParseAuthorizationInput(input, "expected")
		if err != nil || parsed.Code != "abc" || parsed.State != "expected" {
			t.Fatalf("parse %q = %#v, %v", input, parsed, err)
		}
	}
	for _, input := range []string{"http://127.0.0.1/callback?code=abc&state=wrong", "code=abc", "abc#wrong"} {
		if _, err := ParseAuthorizationInput(input, "expected"); err == nil {
			t.Errorf("accepted unbound callback %q", input)
		}
	}
	if _, err := ParseAuthorizationInput("http://127.0.0.1/callback?error=denied&state=expected", "expected"); err == nil || !strings.Contains(err.Error(), "denied") {
		t.Fatalf("provider error = %v", err)
	}
}

func TestDevicePollPendingSlowDownAndCompletion(t *testing.T) {
	ctx := context.Background()
	waits := []time.Duration{}
	results := []DeviceResult{{Kind: DevicePending}, {Kind: DeviceSlowDown}, {Kind: DeviceComplete, AccessToken: "token"}}
	index := 0
	token, err := PollDeviceCode(ctx, 2*time.Second, time.Minute, func(context.Context) DeviceResult {
		result := results[index]
		index++
		return result
	}, func(_ context.Context, duration time.Duration) error { waits = append(waits, duration); return nil })
	if err != nil || token != "token" {
		t.Fatalf("poll = %q, %v", token, err)
	}
	if len(waits) != 3 || waits[0] != 2*time.Second || waits[1] != 2*time.Second || waits[2] != 7*time.Second {
		t.Fatalf("wait schedule = %v", waits)
	}
}

func TestDevicePollExpiryAndCancellation(t *testing.T) {
	polls := 0
	_, err := PollDeviceCode(context.Background(), time.Second, time.Nanosecond, func(context.Context) DeviceResult { polls++; return DeviceResult{Kind: DevicePending} }, func(context.Context, time.Duration) error { return nil })
	if err == nil || polls != 0 {
		t.Fatalf("expiry error=%v polls=%d", err, polls)
	}
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	_, err = PollDeviceCode(ctx, time.Second, time.Minute, func(context.Context) DeviceResult { polls++; return DeviceResult{Kind: DevicePending} }, nil)
	if !errors.Is(err, context.Canceled) {
		t.Fatalf("cancel error = %v", err)
	}
}

func TestWaitForCallbackCancellation(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	result := make(chan error, 1)
	go func() { _, err := WaitForCallback(ctx, "127.0.0.1:0", "state"); result <- err }()
	cancel()
	select {
	case err := <-result:
		if !errors.Is(err, context.Canceled) {
			t.Fatalf("callback error = %v", err)
		}
	case <-time.After(time.Second):
		t.Fatal("callback listener did not stop after cancellation")
	}
}

func TestWaitForCallbackRejectsNonLoopbackAddress(t *testing.T) {
	if _, err := WaitForCallback(context.Background(), "0.0.0.0:0", "state"); err == nil {
		t.Fatal("callback server must reject non-loopback bind")
	}
}

func TestCallbackServerRejectsWrongStateAndReturnsBoundCode(t *testing.T) {
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	address := listener.Addr().String()
	result := make(chan AuthorizationCode, 1)
	failure := make(chan error, 1)
	go func() {
		code, callbackErr := serveCallback(context.Background(), listener, "expected")
		result <- code
		failure <- callbackErr
	}()
	response, err := http.Get("http://" + address + "/callback?code=bad&state=wrong")
	if err != nil {
		t.Fatal(err)
	}
	response.Body.Close()
	if response.StatusCode != http.StatusBadRequest {
		t.Fatalf("wrong-state status = %d", response.StatusCode)
	}
	if _, err := <-result, <-failure; err == nil || !strings.Contains(err.Error(), "state mismatch") {
		t.Fatalf("wrong-state callback error = %v", err)
	}

	listener, err = net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	address = listener.Addr().String()
	go func() {
		code, callbackErr := serveCallback(context.Background(), listener, "expected")
		result <- code
		failure <- callbackErr
	}()
	response, err = http.Get(fmt.Sprintf("http://%s/callback?code=ok&state=expected", address))
	if err != nil {
		t.Fatal(err)
	}
	response.Body.Close()
	if response.StatusCode != http.StatusOK {
		t.Fatalf("valid-state status = %d", response.StatusCode)
	}
	if code, err := <-result, <-failure; err != nil || code.Code != "ok" || code.State != "expected" {
		t.Fatalf("callback = %#v, %v", code, err)
	}
}
