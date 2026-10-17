package bash

import (
	"bytes"
	"context"
	"fmt"
	"io"
	"os/exec"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"rozsa/backend/internal/platform/process"
)

const (
	DefaultTimeout = 120 * time.Second
	MaxOutputBytes = 100 * 1024
	cwdMarker      = "__ROZSA_CWD__"
)

type Result struct {
	Output    string
	ExitCode  *int
	TimedOut  bool
	Truncated bool
	CWD       string
	Duration  time.Duration
}

// Execute runs a Bash command in cwd and kills its complete process tree on
// cancellation, timeout, or output overflow.
func Execute(ctx context.Context, command, cwd string, timeout time.Duration) (Result, error) {
	if timeout <= 0 {
		timeout = DefaultTimeout
	}
	started := time.Now()
	wrapped := fmt.Sprintf("{ %s; }; status=$?; printf '\\n%s%%s\\n' \"$PWD\"; exit \"$status\"", command, cwdMarker)
	child := exec.Command("bash", "-c", wrapped)
	child.Dir = cwd
	child.Stdin = nil
	process.Configure(child)
	var output cappedOutput
	child.Stdout = &output
	child.Stderr = &output
	if err := child.Start(); err != nil {
		return Result{}, fmt.Errorf("Failed to spawn bash process: %w", err)
	}

	waitResult := make(chan error, 1)
	go func() { waitResult <- child.Wait() }()
	timer := time.NewTimer(timeout)
	defer timer.Stop()
	var commandErr error
	select {
	case commandErr = <-waitResult:
	case <-ctx.Done():
		_ = process.KillTree(context.Background(), child)
		<-waitResult
		return Result{Output: output.String(), Duration: time.Since(started)}, fmt.Errorf("Command cancelled")
	case <-timer.C:
		_ = process.KillTree(context.Background(), child)
		<-waitResult
		return Result{Output: output.String(), TimedOut: true, Duration: time.Since(started)}, fmt.Errorf("%sCommand timed out after %dms", withOutputPrefix(output.String()), timeout.Milliseconds())
	case <-output.Overflow():
		_ = process.KillTree(context.Background(), child)
		commandErr = <-waitResult
	}

	text := output.String()
	truncated := output.Truncated()
	text, nextCWD := splitCWDMarker(text)
	result := Result{Output: text, Truncated: truncated, CWD: nextCWD, Duration: time.Since(started)}
	if commandErr != nil {
		if exitError, ok := commandErr.(*exec.ExitError); ok {
			code := exitError.ExitCode()
			result.ExitCode = &code
		} else {
			return Result{}, fmt.Errorf("Failed to wait for process: %w", commandErr)
		}
	} else {
		code := 0
		result.ExitCode = &code
	}
	return result, nil
}

func withOutputPrefix(output string) string {
	if output == "" {
		return ""
	}
	return output + "\n\n"
}

func splitCWDMarker(output string) (string, string) {
	marker := strings.LastIndex(output, cwdMarker)
	if marker < 0 {
		return output, ""
	}
	lineStart := strings.LastIndex(output[:marker], "\n") + 1
	valueStart := marker + len(cwdMarker)
	lineEnd := len(output)
	if newline := strings.IndexByte(output[valueStart:], '\n'); newline >= 0 {
		lineEnd = valueStart + newline + 1
	}
	path := strings.TrimSpace(output[valueStart:lineEnd])
	return output[:lineStart] + output[lineEnd:], filepath.Clean(path)
}

type cappedOutput struct {
	mu        sync.Mutex
	buffer    bytes.Buffer
	truncated bool
	overflow  chan struct{}
}

func (output *cappedOutput) Write(data []byte) (int, error) {
	output.mu.Lock()
	defer output.mu.Unlock()
	if output.overflow == nil {
		output.overflow = make(chan struct{})
	}
	remaining := MaxOutputBytes - output.buffer.Len()
	if remaining <= 0 {
		output.truncated = true
		select {
		case <-output.overflow:
		default:
			close(output.overflow)
		}
		return len(data), nil
	}
	if len(data) > remaining {
		_, _ = output.buffer.Write(data[:remaining])
		output.truncated = true
		select {
		case <-output.overflow:
		default:
			close(output.overflow)
		}
		return len(data), nil
	}
	return output.buffer.Write(data)
}

func (output *cappedOutput) Overflow() <-chan struct{} {
	output.mu.Lock()
	defer output.mu.Unlock()
	if output.overflow == nil {
		output.overflow = make(chan struct{})
	}
	return output.overflow
}

func (output *cappedOutput) Truncated() bool {
	output.mu.Lock()
	defer output.mu.Unlock()
	return output.truncated
}

func (output *cappedOutput) String() string {
	output.mu.Lock()
	defer output.mu.Unlock()
	return string(bytes.ToValidUTF8(output.buffer.Bytes(), []byte("�")))
}

var _ io.Writer = (*cappedOutput)(nil)
