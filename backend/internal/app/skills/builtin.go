package skills

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"io/fs"
	"net/http"
	"net/url"
	"os"
	"path"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"gopkg.in/yaml.v3"
	backendconfig "rozsa/backend"
)

const (
	builtinResponseLimit  = 2 << 20
	builtinRequestTimeout = 15 * time.Second
)

var builtinInstallMu sync.Mutex

type BuiltinConfig struct {
	BuiltinSkillsURL string `yaml:"BUILTIN_SKILLS_URL"`
}

type builtinSource struct {
	baseURL string
	client  *http.Client
}

type githubContent struct {
	Name        string `json:"name"`
	Type        string `json:"type"`
	DownloadURL string `json:"download_url"`
}

func LoadBuiltinConfig(configPath string) (BuiltinConfig, error) {
	contents, err := os.ReadFile(configPath)
	if err != nil {
		return BuiltinConfig{}, fmt.Errorf("read backend config %q: %w", configPath, err)
	}
	return parseBuiltinConfig(contents, configPath)
}

func parseBuiltinConfig(contents []byte, source string) (BuiltinConfig, error) {
	var config BuiltinConfig
	if err := yaml.Unmarshal(contents, &config); err != nil {
		return BuiltinConfig{}, fmt.Errorf("parse backend config %q: %w", source, err)
	}
	parsed, err := url.Parse(config.BuiltinSkillsURL)
	if err != nil || (parsed.Scheme != "https" && parsed.Scheme != "http") || parsed.Host == "" {
		return BuiltinConfig{}, fmt.Errorf("BUILTIN_SKILLS_URL in %q must be an absolute HTTP URL", source)
	}
	return config, nil
}

func installMissingBuiltinSkills(ctx context.Context, globalSkillsDirectory string, source builtinSource) error {
	builtinInstallMu.Lock()
	defer builtinInstallMu.Unlock()
	if source.client == nil {
		source.client = &http.Client{Timeout: builtinRequestTimeout}
	}
	entries, err := listContents(ctx, source, "")
	if err != nil {
		return fmt.Errorf("list remote built-in skills: %w", err)
	}
	for _, entry := range entries {
		if entry.Type != "dir" || !safeBuiltinName(entry.Name) {
			continue
		}
		destination := filepath.Join(globalSkillsDirectory, ".builtin", entry.Name)
		remoteFiles, err := collectRemoteSkillFiles(ctx, source, entry.Name, entry.Name)
		if err != nil {
			return fmt.Errorf("inspect remote built-in skill %q: %w", entry.Name, err)
		}
		if _, exists := remoteFiles["SKILL.md"]; !exists {
			return fmt.Errorf("remote built-in skill %q does not contain a root SKILL.md", entry.Name)
		}
		missing, err := missingBuiltinFiles(destination, remoteFiles)
		if err != nil {
			return err
		}
		if !missing {
			continue
		}
		if err := installRemoteSkill(ctx, globalSkillsDirectory, source, entry.Name, remoteFiles); err != nil {
			return fmt.Errorf("initialize built-in skill %q: %w", entry.Name, err)
		}
	}
	return nil
}

func listContents(ctx context.Context, source builtinSource, relativeDirectory string) ([]githubContent, error) {
	requestURL, err := contentsURL(source.baseURL, relativeDirectory)
	if err != nil {
		return nil, err
	}
	body, err := fetchURL(ctx, source.client, requestURL)
	if err != nil {
		return nil, err
	}
	var entries []githubContent
	if err := json.Unmarshal(body, &entries); err != nil {
		return nil, fmt.Errorf("decode GitHub Contents response: %w", err)
	}
	return entries, nil
}

func contentsURL(baseURL, relativePath string) (string, error) {
	parsed, err := url.Parse(baseURL)
	if err != nil || (parsed.Scheme != "https" && parsed.Scheme != "http") || parsed.Host == "" {
		return "", errors.New("built-in skills URL must be an absolute HTTP URL")
	}
	if relativePath != "" {
		parsed.Path = path.Join(parsed.Path, relativePath)
	}
	return parsed.String(), nil
}

func collectRemoteSkillFiles(ctx context.Context, source builtinSource, skillName, remoteDirectory string) (map[string]githubContent, error) {
	entries, err := listContents(ctx, source, remoteDirectory)
	if err != nil {
		return nil, err
	}
	files := make(map[string]githubContent)
	for _, entry := range entries {
		if !safeBuiltinName(entry.Name) {
			return nil, fmt.Errorf("GitHub Contents returned unsafe name %q", entry.Name)
		}
		relativeDirectory := strings.TrimPrefix(strings.TrimPrefix(remoteDirectory, skillName), "/")
		relativePath := path.Join(relativeDirectory, entry.Name)
		if entry.Type == "dir" {
			nested, err := collectRemoteSkillFiles(ctx, source, skillName, path.Join(remoteDirectory, entry.Name))
			if err != nil {
				return nil, err
			}
			for nestedPath, nestedEntry := range nested {
				files[nestedPath] = nestedEntry
			}
			continue
		}
		if entry.Type != "file" {
			return nil, fmt.Errorf("unsupported GitHub Contents entry type %q for %q", entry.Type, entry.Name)
		}
		if entry.DownloadURL == "" {
			return nil, fmt.Errorf("GitHub Contents file %q has no download URL", entry.Name)
		}
		files[relativePath] = entry
	}
	return files, nil
}

func missingBuiltinFiles(destination string, remoteFiles map[string]githubContent) (bool, error) {
	info, err := os.Lstat(destination)
	if errors.Is(err, os.ErrNotExist) {
		return true, nil
	}
	if err != nil {
		return false, fmt.Errorf("inspect built-in skill directory %q: %w", destination, err)
	}
	if !info.IsDir() {
		return false, fmt.Errorf("built-in skill destination is not a directory: %s", destination)
	}
	for relative := range remoteFiles {
		if !fs.ValidPath(relative) {
			return false, fmt.Errorf("remote built-in skill contains unsafe path %q", relative)
		}
		fileInfo, err := os.Lstat(filepath.Join(destination, filepath.FromSlash(relative)))
		if errors.Is(err, os.ErrNotExist) {
			return true, nil
		}
		if err != nil {
			return false, fmt.Errorf("inspect built-in skill file %q: %w", relative, err)
		}
		if !fileInfo.Mode().IsRegular() {
			return false, fmt.Errorf("built-in skill file is not regular: %s", relative)
		}
	}
	return false, nil
}

func installRemoteSkill(ctx context.Context, globalSkillsDirectory string, source builtinSource, skillName string, remoteFiles map[string]githubContent) error {
	destination := filepath.Join(globalSkillsDirectory, ".builtin", skillName)
	destinationExists := false
	if info, err := os.Lstat(destination); err == nil {
		if !info.IsDir() {
			return fmt.Errorf("destination is not a directory: %s", destination)
		}
		destinationExists = true
	} else if !errors.Is(err, os.ErrNotExist) {
		return fmt.Errorf("inspect destination: %w", err)
	}
	parent := filepath.Dir(destination)
	if err := os.MkdirAll(parent, 0o700); err != nil {
		return fmt.Errorf("create built-in skill directory: %w", err)
	}
	staging, err := os.MkdirTemp(parent, ".builtin-skill-install-*")
	if err != nil {
		return fmt.Errorf("create built-in skill staging directory: %w", err)
	}
	defer os.RemoveAll(staging)
	for relative, entry := range remoteFiles {
		if !fs.ValidPath(relative) {
			return fmt.Errorf("unsafe remote skill path %q", relative)
		}
		contents, err := fetchURL(ctx, source.client, entry.DownloadURL)
		if err != nil {
			return fmt.Errorf("download %q: %w", relative, err)
		}
		localPath := filepath.Join(staging, filepath.FromSlash(relative))
		if err := os.MkdirAll(filepath.Dir(localPath), 0o700); err != nil {
			return fmt.Errorf("create directory for %q: %w", relative, err)
		}
		if err := os.WriteFile(localPath, contents, 0o600); err != nil {
			return fmt.Errorf("write %q: %w", relative, err)
		}
	}
	if destinationExists {
		missing, checkErr := missingBuiltinFiles(destination, remoteFiles)
		if checkErr != nil {
			return checkErr
		}
		if !missing {
			return nil
		}
		return mergeMissingSkillFiles(staging, destination)
	}
	if err := os.Rename(staging, destination); err != nil {
		missing, checkErr := missingBuiltinFiles(destination, remoteFiles)
		if !missing && checkErr == nil {
			return nil
		}
		return fmt.Errorf("install into %s: %w", destination, err)
	}
	return nil
}

func mergeMissingSkillFiles(staging, destination string) error {
	return filepath.WalkDir(staging, func(sourcePath string, entry os.DirEntry, walkErr error) error {
		if walkErr != nil {
			return walkErr
		}
		if sourcePath == staging {
			return nil
		}
		relative, err := filepath.Rel(staging, sourcePath)
		if err != nil {
			return err
		}
		target := filepath.Join(destination, relative)
		if entry.IsDir() {
			if err := ensureSafeDirectories(destination, filepath.Dir(target)); err != nil {
				return fmt.Errorf("create missing skill directory %q: %w", relative, err)
			}
			return nil
		}
		if !entry.Type().IsRegular() {
			return fmt.Errorf("downloaded skill entry %q is not a regular file", relative)
		}
		if err := ensureSafeDirectories(destination, filepath.Dir(target)); err != nil {
			return fmt.Errorf("prepare skill file %q: %w", relative, err)
		}
		if info, err := os.Lstat(target); err == nil {
			if !info.Mode().IsRegular() {
				return fmt.Errorf("existing skill path %q is not a regular file", relative)
			}
			return nil
		} else if !errors.Is(err, os.ErrNotExist) {
			return fmt.Errorf("inspect skill file %q: %w", relative, err)
		}
		input, err := os.Open(sourcePath)
		if err != nil {
			return err
		}
		temporary, err := os.CreateTemp(filepath.Dir(target), ".builtin-skill-file-*")
		if err != nil {
			return errors.Join(err, input.Close())
		}
		_, copyErr := io.Copy(temporary, input)
		inputCloseErr := input.Close()
		closeErr := temporary.Close()
		if err := errors.Join(copyErr, inputCloseErr, closeErr); err != nil {
			_ = os.Remove(temporary.Name())
			return err
		}
		if err := os.Link(temporary.Name(), target); err != nil && !errors.Is(err, os.ErrExist) {
			_ = os.Remove(temporary.Name())
			return fmt.Errorf("install missing skill file %q: %w", relative, err)
		}
		return os.Remove(temporary.Name())
	})
}

func ensureSafeDirectories(root, target string) error {
	relative, err := filepath.Rel(root, target)
	if err != nil || relative == ".." || strings.HasPrefix(relative, ".."+string(filepath.Separator)) {
		return fmt.Errorf("directory escapes skill root")
	}
	current := root
	for _, segment := range strings.Split(relative, string(filepath.Separator)) {
		if segment == "." || segment == "" {
			continue
		}
		current = filepath.Join(current, segment)
		info, err := os.Lstat(current)
		if errors.Is(err, os.ErrNotExist) {
			if err := os.Mkdir(current, 0o700); err != nil && !errors.Is(err, os.ErrExist) {
				return err
			}
			info, err = os.Lstat(current)
		}
		if err != nil {
			return err
		}
		if !info.IsDir() {
			return fmt.Errorf("path component is not a directory: %s", current)
		}
	}
	return nil
}

func fetchURL(ctx context.Context, client *http.Client, target string) ([]byte, error) {
	request, err := http.NewRequestWithContext(ctx, http.MethodGet, target, nil)
	if err != nil {
		return nil, err
	}
	request.Header.Set("Accept", "application/vnd.github+json")
	request.Header.Set("User-Agent", "rozsa-builtin-skill-installer")
	response, err := client.Do(request)
	if err != nil {
		return nil, err
	}
	defer response.Body.Close()
	if response.StatusCode < 200 || response.StatusCode >= 300 {
		return nil, fmt.Errorf("GET %s returned HTTP %s", request.URL.Redacted(), response.Status)
	}
	body, err := io.ReadAll(io.LimitReader(response.Body, builtinResponseLimit+1))
	if err != nil {
		return nil, err
	}
	if len(body) > builtinResponseLimit {
		return nil, fmt.Errorf("response exceeds %d bytes", builtinResponseLimit)
	}
	return body, nil
}

func safeBuiltinName(name string) bool {
	return name != "" && name != "." && name != ".." && !strings.ContainsAny(name, `/\\:`) && path.Base(name) == name
}

func configuredBuiltinSkillsURL() (string, error) {
	configPath, exists, err := developmentConfigPath()
	if err != nil {
		return "", err
	}
	contents := backendconfig.BuiltinConfig()
	source := "compiled backend config"
	if exists {
		var readErr error
		contents, readErr = os.ReadFile(configPath)
		if readErr != nil {
			return "", fmt.Errorf("read development backend config %q: %w", configPath, readErr)
		}
		source = configPath
	}
	config, err := parseBuiltinConfig(contents, source)
	if err != nil {
		return "", err
	}
	return config.BuiltinSkillsURL, nil
}

func developmentConfigPath() (string, bool, error) {
	executable, err := os.Executable()
	if err != nil {
		return "", false, fmt.Errorf("locate backend executable: %w", err)
	}
	configPath := filepath.Clean(filepath.Join(filepath.Dir(executable), "..", "backend", "config.yaml"))
	if _, err := os.Stat(configPath); err == nil {
		return configPath, true, nil
	} else if errors.Is(err, os.ErrNotExist) {
		return "", false, nil
	} else {
		return "", false, fmt.Errorf("inspect development backend config %q: %w", configPath, err)
	}
}
