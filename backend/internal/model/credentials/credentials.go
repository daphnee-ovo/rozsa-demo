// Package credentials resolves provider secrets and stores OAuth credentials.
package credentials

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"regexp"
	"runtime"
	"strings"
	"sync"
	"time"
)

var environmentNamePattern = regexp.MustCompile(`^[A-Za-z_][A-Za-z0-9_]*$`)

type Resolver struct {
	LookupEnv func(string) (string, bool)
	ReadFile  func(string) ([]byte, error)
	HomeDir   func() (string, error)
	Shell     string
	Now       func() time.Time
}

func NewResolver() Resolver {
	return Resolver{LookupEnv: os.LookupEnv, ReadFile: os.ReadFile, HomeDir: os.UserHomeDir, Shell: os.Getenv("SHELL"), Now: time.Now}
}

// ResolveAPIKey applies configured model value, provider environment, then OAuth auth.json.
// A configured $NAME reference is resolved strictly; an unavailable reference is an error.
func (resolver Resolver) ResolveAPIKey(ctx context.Context, provider, configuredValue, providerEnv, authPath string, refresh RefreshFunc) (string, error) {
	if configuredValue != "" {
		value, err := resolver.ResolveConfigValue(configuredValue)
		if err != nil {
			return "", fmt.Errorf("resolve configured API key for %q: %w", provider, err)
		}
		return value, nil
	}
	if providerEnv != "" {
		value, err := resolver.ResolveEnvironmentVariable(providerEnv)
		if err != nil {
			return "", err
		}
		if value != "" {
			return value, nil
		}
	}
	if authPath == "" {
		return "", nil
	}
	credential, err := ResolveAuth(ctx, authPath, provider, resolver.now(), refresh)
	if err != nil {
		return "", err
	}
	if credential.Type == "" && credential.Access != "" {
		return resolver.ResolveConfigValue(credential.Access)
	}
	return credential.Access, nil
}

func (resolver Resolver) ResolveConfigValue(value string) (string, error) {
	if value == "" {
		return "", errors.New("configuration value must not be empty")
	}
	if strings.HasPrefix(value, "!") {
		return "", errors.New("shell command credential references are disabled; use $NAME or a literal value")
	}
	if !strings.HasPrefix(value, "$") {
		return value, nil
	}
	name := strings.TrimPrefix(value, "$")
	if !environmentNamePattern.MatchString(name) {
		return "", fmt.Errorf("invalid environment variable name %q", name)
	}
	resolved, err := resolver.ResolveEnvironmentVariable(name)
	if err != nil {
		return "", err
	}
	if resolved == "" {
		return "", fmt.Errorf("environment variable %q is not set in process environment, private environment file, or shell startup files", name)
	}
	return resolved, nil
}

func (resolver Resolver) ResolveEnvironmentVariable(name string) (string, error) {
	if !environmentNamePattern.MatchString(name) {
		return "", fmt.Errorf("invalid environment variable name %q", name)
	}
	lookup := resolver.lookupEnv()
	if value, exists := lookup(name); exists && value != "" {
		return value, nil
	}
	configDir, isSet := lookup("ROZSA_CONFIG_DIR")
	if isSet && configDir == "" {
		return "", errors.New("ROZSA_CONFIG_DIR must not be empty")
	}
	if !isSet {
		home, err := resolver.homeDir()
		if err != nil || home == "" {
			return "", errors.New("cannot determine the Rózsa private environment path")
		}
		configDir = filepath.Join(home, ".rozsa")
	}
	if value, found, err := readAssignment(resolver.readFile(), filepath.Join(configDir, ".env"), name, ShellPOSIX, true); err != nil {
		return "", err
	} else if found && value != "" {
		return value, nil
	}
	for _, profile := range resolver.profilePaths() {
		syntax := ShellPOSIX
		if filepath.Base(profile) == "config.fish" {
			syntax = ShellFish
		}
		if value, found, err := readAssignment(resolver.readFile(), profile, name, syntax, false); err != nil {
			return "", err
		} else if found && value != "" {
			return value, nil
		}
	}
	return "", nil
}

func (resolver Resolver) profilePaths() []string {
	home, err := resolver.homeDir()
	if err != nil || home == "" || runtime.GOOS == "windows" {
		return nil
	}
	switch filepath.Base(resolver.Shell) {
	case "bash":
		return []string{filepath.Join(home, ".bash_profile"), filepath.Join(home, ".bash_login"), filepath.Join(home, ".profile"), filepath.Join(home, ".bashrc")}
	case "fish":
		return []string{filepath.Join(home, ".config", "fish", "config.fish")}
	default:
		return []string{filepath.Join(home, ".zshenv"), filepath.Join(home, ".zprofile"), filepath.Join(home, ".zshrc"), filepath.Join(home, ".profile")}
	}
}
func (resolver Resolver) lookupEnv() func(string) (string, bool) {
	if resolver.LookupEnv != nil {
		return resolver.LookupEnv
	}
	return os.LookupEnv
}
func (resolver Resolver) readFile() func(string) ([]byte, error) {
	if resolver.ReadFile != nil {
		return resolver.ReadFile
	}
	return os.ReadFile
}
func (resolver Resolver) homeDir() (string, error) {
	if resolver.HomeDir != nil {
		return resolver.HomeDir()
	}
	return os.UserHomeDir()
}
func (resolver Resolver) now() time.Time {
	if resolver.Now != nil {
		return resolver.Now()
	}
	return time.Now()
}

type ShellSyntax uint8

const (
	ShellPOSIX ShellSyntax = iota
	ShellFish
)

func readAssignment(readFile func(string) ([]byte, error), path, name string, syntax ShellSyntax, strict bool) (string, bool, error) {
	data, err := readFile(path)
	if errors.Is(err, fs.ErrNotExist) {
		return "", false, nil
	}
	if err != nil {
		return "", false, fmt.Errorf("read environment file %q: %w", path, err)
	}
	var value string
	found := false
	for index, line := range strings.Split(string(data), "\n") {
		parsedName, parsedValue, ok, parseErr := parseAssignment(strings.TrimSuffix(line, "\r"), syntax, strict)
		if parseErr != nil {
			return "", false, fmt.Errorf("invalid private environment entry at %s:%d: %w", path, index+1, parseErr)
		}
		if ok && parsedName == name {
			value, found = parsedValue, true
		}
	}
	return value, found, nil
}

func parseAssignment(line string, syntax ShellSyntax, strict bool) (string, string, bool, error) {
	line = strings.TrimSpace(line)
	if line == "" || strings.HasPrefix(line, "#") {
		return "", "", false, nil
	}
	if syntax == ShellFish {
		if !strings.HasPrefix(line, "set ") {
			return "", "", false, nil
		}
		fields := strings.Fields(strings.TrimSpace(strings.TrimPrefix(line, "set ")))
		index, exported := 0, false
		for index < len(fields) && strings.HasPrefix(fields[index], "-") {
			exported = exported || fields[index] == "--export" || strings.Contains(fields[index], "x")
			index++
		}
		if !exported || index >= len(fields) {
			return "", "", false, nil
		}
		name := fields[index]
		value := strings.TrimSpace(line[strings.Index(line, name)+len(name):])
		return parseLiteral(name, value, strict)
	}
	line = strings.TrimPrefix(line, "export ")
	name, rawValue, hasEquals := strings.Cut(line, "=")
	if !hasEquals {
		if strict {
			return "", "", false, errors.New("expected NAME=VALUE")
		}
		return "", "", false, nil
	}
	return parseLiteral(strings.TrimSpace(name), strings.TrimSpace(rawValue), strict)
}

func parseLiteral(name, rawValue string, strict bool) (string, string, bool, error) {
	if !environmentNamePattern.MatchString(name) {
		if strict {
			return "", "", false, fmt.Errorf("invalid variable name %q", name)
		}
		return "", "", false, nil
	}
	if !strict && strings.ContainsAny(rawValue, "$`()|&<>;") {
		if strict {
			return "", "", false, errors.New("expressions are not supported in private environment values")
		}
		return "", "", false, nil
	}
	value := rawValue
	if strings.HasPrefix(value, "\"") {
		var decoded string
		if err := json.Unmarshal([]byte(value), &decoded); err != nil {
			if strict {
				return "", "", false, fmt.Errorf("invalid quoted value: %w", err)
			}
			return "", "", false, nil
		}
		value = decoded
	} else if strings.HasPrefix(value, "'") {
		if len(value) < 2 || !strings.HasSuffix(value, "'") {
			if strict {
				return "", "", false, errors.New("invalid quoted value")
			}
			return "", "", false, nil
		}
		value = value[1 : len(value)-1]
	} else if !strict {
		value, _, _ = strings.Cut(value, " #")
	}
	return name, value, true, nil
}

type OAuthCredential struct {
	Type    string         `json:"type"`
	Access  string         `json:"access"`
	Refresh string         `json:"refresh"`
	Expires int64          `json:"expires"`
	Extra   map[string]any `json:"-"`
}

func (credential OAuthCredential) MarshalJSON() ([]byte, error) {
	object := make(map[string]any, len(credential.Extra)+4)
	for key, value := range credential.Extra {
		object[key] = value
	}
	object["type"], object["access"], object["refresh"], object["expires"] = "oauth", credential.Access, credential.Refresh, credential.Expires
	return json.Marshal(object)
}
func (credential *OAuthCredential) UnmarshalJSON(data []byte) error {
	var object map[string]any
	if err := json.Unmarshal(data, &object); err != nil {
		return err
	}
	if object["type"] != "oauth" {
		return errors.New("credential type is not oauth")
	}
	var required struct {
		Access  string `json:"access"`
		Refresh string `json:"refresh"`
		Expires int64  `json:"expires"`
	}
	if err := json.Unmarshal(data, &required); err != nil {
		return err
	}
	if required.Access == "" || required.Refresh == "" {
		return errors.New("OAuth credential requires access and refresh tokens")
	}
	delete(object, "type")
	delete(object, "access")
	delete(object, "refresh")
	delete(object, "expires")
	credential.Type, credential.Access, credential.Refresh, credential.Expires, credential.Extra = "oauth", required.Access, required.Refresh, required.Expires, object
	return nil
}

type RefreshFunc func(context.Context, string, OAuthCredential) (OAuthCredential, error)

var providerLocks sync.Map

type contextLock struct{ gate chan struct{} }

func lockFor(key string) *contextLock {
	value, _ := providerLocks.LoadOrStore(key, &contextLock{gate: make(chan struct{}, 1)})
	return value.(*contextLock)
}

func ResolveAuth(ctx context.Context, path, provider string, now time.Time, refresh RefreshFunc) (OAuthCredential, error) {
	unlock, err := acquireFileLock(ctx, path)
	if err != nil {
		return OAuthCredential{}, err
	}
	defer unlock()
	data, err := os.ReadFile(path)
	if err != nil {
		if errors.Is(err, fs.ErrNotExist) {
			return OAuthCredential{}, nil
		}
		return OAuthCredential{}, fmt.Errorf("read auth.json %q: %w", path, err)
	}
	var auth map[string]json.RawMessage
	if err := json.Unmarshal(data, &auth); err != nil {
		return OAuthCredential{}, fmt.Errorf("parse auth.json %q: %w", path, err)
	}
	if auth == nil {
		return OAuthCredential{}, fmt.Errorf("parse auth.json %q: root must be an object", path)
	}
	raw, exists := auth[provider]
	if !exists {
		return OAuthCredential{}, nil
	}
	var kind struct {
		Type string `json:"type"`
	}
	if err := json.Unmarshal(raw, &kind); err != nil {
		return OAuthCredential{}, fmt.Errorf("invalid auth.json credential for %q: %w", provider, err)
	}
	if kind.Type == "api_key" {
		var stored struct {
			Key string `json:"key"`
		}
		if err := json.Unmarshal(raw, &stored); err != nil || stored.Key == "" {
			return OAuthCredential{}, fmt.Errorf("invalid auth.json API key credential for %q", provider)
		}
		return OAuthCredential{Access: stored.Key}, nil
	}
	var credential OAuthCredential
	if err := json.Unmarshal(raw, &credential); err != nil {
		return OAuthCredential{}, fmt.Errorf("invalid auth.json OAuth credential for %q: %w", provider, err)
	}
	if credential.Expires > now.UnixMilli() {
		return credential, nil
	}
	if refresh == nil {
		return OAuthCredential{}, fmt.Errorf("OAuth credential for %q is expired and no refresh handler is configured", provider)
	}
	updated, err := refresh(ctx, provider, credential)
	if err != nil {
		return OAuthCredential{}, fmt.Errorf("refresh OAuth credential for %q: %w", provider, err)
	}
	if err := ctx.Err(); err != nil {
		return OAuthCredential{}, err
	}
	encoded, err := json.Marshal(updated)
	if err != nil {
		return OAuthCredential{}, fmt.Errorf("encode refreshed OAuth credential: %w", err)
	}
	auth[provider] = encoded
	updatedData, err := json.MarshalIndent(auth, "", "  ")
	if err != nil {
		return OAuthCredential{}, fmt.Errorf("encode auth.json: %w", err)
	}
	if err := writePrivateFile(path, updatedData); err != nil {
		return OAuthCredential{}, err
	}
	return updated, nil
}

func StoreOAuth(path, provider string, credential OAuthCredential) error {
	unlock, err := acquireFileLock(context.Background(), path)
	if err != nil {
		return err
	}
	defer unlock()
	auth, err := readAuthMap(path)
	if err != nil {
		return err
	}
	encoded, err := json.Marshal(credential)
	if err != nil {
		return fmt.Errorf("encode OAuth credential: %w", err)
	}
	auth[provider] = encoded
	data, err := json.MarshalIndent(auth, "", "  ")
	if err != nil {
		return fmt.Errorf("encode auth.json: %w", err)
	}
	return writePrivateFile(path, data)
}
func RemoveStored(path, provider string) (bool, error) {
	unlock, err := acquireFileLock(context.Background(), path)
	if err != nil {
		return false, err
	}
	defer unlock()
	auth, err := readAuthMap(path)
	if err != nil {
		return false, err
	}
	if _, exists := auth[provider]; !exists {
		return false, nil
	}
	delete(auth, provider)
	data, err := json.MarshalIndent(auth, "", "  ")
	if err != nil {
		return false, fmt.Errorf("encode auth.json: %w", err)
	}
	if err := writePrivateFile(path, data); err != nil {
		return false, err
	}
	return true, nil
}

func acquireFileLock(ctx context.Context, path string) (func(), error) {
	lock := lockFor(filepath.Clean(path))
	select {
	case lock.gate <- struct{}{}:
		return func() { <-lock.gate }, nil
	case <-ctx.Done():
		return nil, ctx.Err()
	}
}
func readAuthMap(path string) (map[string]json.RawMessage, error) {
	auth := map[string]json.RawMessage{}
	data, err := os.ReadFile(path)
	if errors.Is(err, fs.ErrNotExist) {
		return auth, nil
	}
	if err != nil {
		return nil, fmt.Errorf("read auth.json %q: %w", path, err)
	}
	if err := json.Unmarshal(data, &auth); err != nil {
		return nil, fmt.Errorf("parse auth.json %q: %w", path, err)
	}
	if auth == nil {
		return nil, fmt.Errorf("parse auth.json %q: root must be an object", path)
	}
	return auth, nil
}
func writePrivateFile(path string, data []byte) error {
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		return fmt.Errorf("create credential directory: %w", err)
	}
	temporary, err := os.CreateTemp(filepath.Dir(path), ".auth-*.tmp")
	if err != nil {
		return fmt.Errorf("create temporary auth.json: %w", err)
	}
	temporaryPath := temporary.Name()
	defer os.Remove(temporaryPath)
	if err := temporary.Chmod(0o600); err != nil {
		temporary.Close()
		return fmt.Errorf("secure temporary auth.json: %w", err)
	}
	if _, err := temporary.Write(data); err != nil {
		temporary.Close()
		return fmt.Errorf("write temporary auth.json: %w", err)
	}
	if err := temporary.Sync(); err != nil {
		temporary.Close()
		return fmt.Errorf("flush temporary auth.json: %w", err)
	}
	if err := temporary.Close(); err != nil {
		return fmt.Errorf("close temporary auth.json: %w", err)
	}
	if err := os.Rename(temporaryPath, path); err != nil {
		return fmt.Errorf("replace auth.json %q: %w", path, err)
	}
	return nil
}
