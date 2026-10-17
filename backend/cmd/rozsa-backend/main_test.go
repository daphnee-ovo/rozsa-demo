package main

import (
	"bytes"
	"context"
	"flag"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"testing"
	"time"

	"go.lsp.dev/jsonrpc2"
)

// Execute the actual main in an owned child, including its stderr diagnostics
// and exit status. No external build/runner or user session directory is used.
func TestBackendHelperProcess(t *testing.T) {
	if os.Getenv("ROZSA_TEST_BACKEND_PROCESS") != "1" {
		return
	}
	os.Args = []string{os.Args[0]}
	flag.CommandLine = flag.NewFlagSet(os.Args[0], flag.ExitOnError)
	main()
	os.Exit(0)
}

type processIO struct {
	io.Reader
	io.Writer
}

func (processIO) Close() error { return nil }

func TestStdioProcessProtocolAndDiagnostics(t *testing.T) {
	configRoot := t.TempDir()
	global := filepath.Join(configRoot, "global")
	project := filepath.Join(configRoot, "project")
	if err := os.MkdirAll(filepath.Join(global, "models"), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(project, 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(global, "settings.json"), []byte(`{"defaultProvider":"test","defaultModel":"test"}`), 0o600); err != nil {
		t.Fatal(err)
	}
	modelConfig := `{"providers":{"test":{"api":"openai-completions","apiKey":"test-key","baseUrl":"http://127.0.0.1:1/v1","models":[{"id":"test","contextWindow":4096,"maxTokens":256}]}}}`
	if err := os.WriteFile(filepath.Join(global, "models", "test.json"), []byte(modelConfig), 0o600); err != nil {
		t.Fatal(err)
	}
	for _, malformed := range []bool{false, true} {
		t.Run(map[bool]string{false: "clean shutdown", true: "malformed input"}[malformed], func(t *testing.T) {
			executable, err := os.Executable()
			if err != nil {
				t.Fatal(err)
			}
			ctx, cancel := context.WithTimeout(t.Context(), 8*time.Second)
			defer cancel()
			cmd := exec.CommandContext(ctx, executable, "-test.run=^TestBackendHelperProcess$")
			cmd.Env = append(os.Environ(), "ROZSA_TEST_BACKEND_PROCESS=1", "ROZSA_CONFIG_DIR="+global, "ROZSA_PROJECT_CONFIG_DIR="+project)
			input, err := cmd.StdinPipe()
			if err != nil {
				t.Fatal(err)
			}
			output, err := cmd.StdoutPipe()
			if err != nil {
				t.Fatal(err)
			}
			var diagnostics bytes.Buffer
			cmd.Stderr = &diagnostics
			if err := cmd.Start(); err != nil {
				t.Fatal(err)
			}
			stream := jsonrpc2.NewStream(processIO{output, input})
			send := func(id int64, method string, params string) {
				t.Helper()
				if _, err := stream.Write(ctx, jsonrpc2.NewCall(jsonrpc2.NewNumberID(id), method, jsonrpc2.RawMessage(params))); err != nil {
					t.Fatal(err)
				}
			}
			read := func() *jsonrpc2.Response {
				t.Helper()
				message, _, err := stream.Read(ctx)
				if err != nil {
					t.Fatal(err)
				}
				response, ok := message.(*jsonrpc2.Response)
				if !ok {
					t.Fatalf("stdout contained %T", message)
				}
				return response
			}
			send(1, "initialize", `{"protocolVersion":1}`)
			if err := read().Err(); err != nil {
				t.Fatal(err)
			}
			send(2, "get_sessions", `{}`)
			if response := read(); response.Err() != nil || string(response.Result()) != "[]" {
				t.Fatalf("get_sessions result = %s, error=%v; want empty configured catalog", response.Result(), response.Err())
			}
			if malformed {
				frames := stream.(interface {
					WriteFrame(context.Context, []byte) (int64, error)
				})
				if _, err := frames.WriteFrame(ctx, []byte(`{`)); err != nil {
					t.Fatal(err)
				}
				if _, _, err := stream.Read(ctx); err != io.EOF {
					t.Fatalf("parser failure wrote a non-protocol stdout message or wire error response: %v", err)
				}
			} else {
				send(3, "shutdown", `{}`)
				if err := read().Err(); err != nil {
					t.Fatal(err)
				}
			}
			input.Close()
			err = cmd.Wait()
			if malformed {
				if err == nil || diagnostics.Len() == 0 {
					t.Fatalf("want exit failure and stderr diagnostic: %v %q", err, diagnostics.String())
				}
			} else if err != nil || diagnostics.Len() != 0 {
				t.Fatalf("clean shutdown: %v %q", err, diagnostics.String())
			}
		})
	}
}
