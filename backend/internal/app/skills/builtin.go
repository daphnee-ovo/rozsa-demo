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

type builtinConfig struct {
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

func builtinSkillsURL() (string, error) {
	var config builtinConfig
	if err := yaml.Unmarshal(backendconfig.BuiltinConfig, &config); err != nil {
		return "", fmt.Errorf("parse embedded backend config: %w", err)
	}
	parsed, err := url.Parse(config.BuiltinSkillsURL)
	if err != nil || (parsed.Scheme != "https" && parsed.Scheme != "http") || parsed.Host == "" {
		return "", errors.New("BUILTIN_SKILLS_URL in embedded backend config must be an absolute HTTP URL")
	}
	return config.BuiltinSkillsURL, nil
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
	requestURL := source.baseURL
	if relativeDirectory != "" {
		parsed, err := url.Parse(source.baseURL)
		if err != nil {
			return nil, err
		}
		parsed.Path = path.Join(parsed.Path, relativeDirectory)
		requestURL = parsed.String()
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

func collectRemoteSkillFiles(ctx context.Context, source builtinSource, skillName, remoteDirectory string) (map[string]githubContent, error) {
	entries, err := listContents(ctx, source, remoteDirectory)
	if err != nil {
		return nil, err
	}
	files := make(map[string]githubContent)
	relativeDirectory := strings.TrimPrefix(strings.TrimPrefix(remoteDirectory, skillName), "/")
	for _, entry := range entries {
		if !safeBuiltinName(entry.Name) {
			return nil, fmt.Errorf("GitHub Contents returned unsafe name %q", entry.Name)
		}
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
	if err := os.Rename(staging, destination); err != nil {
		if _, statErr := os.Lstat(destination); statErr == nil {
			missing, checkErr := missingBuiltinFiles(destination, remoteFiles)
			if checkErr != nil {
				return checkErr
			}
			if !missing {
				return nil
			}
			return mergeMissingSkillFiles(staging, destination)
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
			return ensureSafeDirectories(destination, target)
		}
		if !entry.Type().IsRegular() {
			return fmt.Errorf("downloaded skill entry %q is not a regular file", relative)
		}
		if err := ensureSafeDirectories(destination, filepath.Dir(target)); err != nil {
			return fmt.Errorf("prepare skill file %q: %w", relative, err)
		}
		input, err := os.Open(sourcePath)
		if err != nil {
			return err
		}
		output, err := os.OpenFile(target, os.O_CREATE|os.O_EXCL|os.O_WRONLY, 0o600)
		if err != nil {
			_ = input.Close()
			if errors.Is(err, os.ErrExist) {
				info, statErr := os.Lstat(target)
				if statErr == nil && info.Mode().IsRegular() {
					return nil
				}
				return fmt.Errorf("existing skill path %q is not a regular file", relative)
			}
			return err
		}
		_, copyErr := io.Copy(output, input)
		inputCloseErr := input.Close()
		outputCloseErr := output.Close()
		if err := errors.Join(copyErr, inputCloseErr, outputCloseErr); err != nil {
			_ = os.Remove(target)
			return fmt.Errorf("install skill file %q: %w", relative, err)
		}
		return nil
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
			err = os.Mkdir(current, 0o700)
			if errors.Is(err, os.ErrExist) {
				info, err = os.Lstat(current)
			} else if err == nil {
				info, err = os.Lstat(current)
			}
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
