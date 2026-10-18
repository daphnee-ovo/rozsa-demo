package skills

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"

	"rozsa/backend/internal/platform/configpaths"
)

func writeSkill(t *testing.T, root, directory, contents string) {
	t.Helper()
	path := filepath.Join(root, directory)
	if err := os.MkdirAll(path, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(path, "SKILL.md"), []byte(contents), 0o644); err != nil {
		t.Fatal(err)
	}
}

func TestLoadLayerPriorityDiagnosticsAndSlashNames(t *testing.T) {
	root := t.TempDir()
	user, agents, project := filepath.Join(root, "user"), filepath.Join(root, "agents"), filepath.Join(root, "project")
	writeSkill(t, user, "deploy", "---\nname: deploy\ndescription: user deploy\n---\nuser body")
	writeSkill(t, agents, "deploy", "---\nname: deploy\ndescription: agents deploy\n---\nagents body")
	writeSkill(t, project, "deploy", "---\nname: deploy\ndescription: project deploy\n---\nproject body")
	writeSkill(t, project, "lint", "---\ndescription: direct skill\n---\nbody")
	writeSkill(t, project, "nested/lint", "---\ndescription: nested skill\n---\nbody")
	writeSkill(t, project, "invalid", "---\nname: invalid\n---\nmissing description")
	failedSource := filepath.Join(root, "unreadable-source")
	if err := os.WriteFile(failedSource, []byte("not a directory"), 0o600); err != nil {
		t.Fatal(err)
	}

	result := Load([]Source{{user, User}, {failedSource, Agents}, {agents, Agents}, {project, Project}}, nil)
	if len(result.Skills) != 2 || len(result.Diagnostics) != 2 {
		t.Fatalf("got %d skills and %d diagnostics", len(result.Skills), len(result.Diagnostics))
	}
	var deploy Skill
	for _, skill := range result.Skills {
		if skill.Name == "deploy" {
			deploy = skill
		}
	}
	if deploy.Scope != Project || deploy.Description != "project deploy" {
		t.Fatalf("project scope did not win duplicate: %#v", deploy)
	}
	for _, skill := range result.Skills {
		if skill.Name == "lint" && skill.Description != "direct skill" {
			t.Fatalf("nested skill was loaded instead of the direct child: %#v", skill)
		}
	}
	if names := result.SlashCommandNames(); !reflect.DeepEqual(names, []string{"deploy", "lint"}) {
		t.Fatalf("slash names = %#v", names)
	}
	if got := result.PromptFragment(); got == "" || !contains(got, "$PROJECT_SKILLS/deploy/SKILL.md") {
		t.Fatalf("prompt fragment missing skill path: %q", got)
	}
}

func TestLoadDiscoversBuiltinDirectoryAndLetsRegularSkillsOverrideIt(t *testing.T) {
	global := t.TempDir()
	writeSkill(t, global, ".builtin/rozsa-docs", "---\nname: rozsa-docs\ndescription: built-in docs\n---\nbuiltin body")
	writeSkill(t, global, ".builtin/not-in-compiled-list", "---\nname: not-in-compiled-list\ndescription: unrelated\n---\nbody")
	loaded := loadWithBuiltins([]Source{{Directory: global, Scope: User}}, filepath.Join(global, ".builtin"), []string{"rozsa-docs"}, nil)
	if len(loaded.Skills) != 1 || loaded.Skills[0].Scope != Builtin || loaded.Skills[0].VariablePath() != "$USER_SKILLS/.builtin/rozsa-docs/SKILL.md" {
		t.Fatalf("built-in skill was not discovered with its virtual path: %#v", loaded.Skills)
	}

	writeSkill(t, global, "rozsa-docs", "---\nname: rozsa-docs\ndescription: user docs\n---\nuser body")
	loaded = loadWithBuiltins([]Source{{Directory: global, Scope: User}}, filepath.Join(global, ".builtin"), []string{"rozsa-docs"}, nil)
	if len(loaded.Skills) != 1 || loaded.Skills[0].Scope != User || loaded.Skills[0].Description != "user docs" {
		t.Fatalf("regular global skill did not override its built-in duplicate: %#v", loaded.Skills)
	}
}

func TestBuiltinSkillDownloadsEntireDirectoryAndFillsMissingFilesWithoutOverwrite(t *testing.T) {
	server := newBuiltinContentsServer(t, map[string]any{
		"rozsa-docs": []map[string]string{
			{"name": "SKILL.md", "type": "file", "download_url": "/raw/rozsa-docs/SKILL.md"},
			{"name": "references", "type": "dir"},
		},
		"rozsa-docs/references": []map[string]string{{"name": "guide.md", "type": "file", "download_url": "/raw/rozsa-docs/references/guide.md"}},
	})
	defer server.Close()
	globalSkillDirectory := filepath.Join(t.TempDir(), "global", "skills")
	existingDirectory := filepath.Join(globalSkillDirectory, ".builtin", "rozsa-docs")
	if err := os.MkdirAll(existingDirectory, 0o700); err != nil {
		t.Fatal(err)
	}
	localNote := filepath.Join(existingDirectory, "local-note.md")
	if err := os.WriteFile(localNote, []byte("keep local"), 0o600); err != nil {
		t.Fatal(err)
	}
	source := builtinSource{baseURL: server.URL + "/contents/builtin-skills", client: server.Client()}
	missing, err := checkMissingBuiltinSkills(globalSkillDirectory, []string{"rozsa-docs"})
	if err != nil || !reflect.DeepEqual(missing, []string{"rozsa-docs"}) {
		t.Fatalf("missing built-in skills = %#v, err=%v", missing, err)
	}
	if err := downloadBuiltinSkills(context.Background(), globalSkillDirectory, missing, source); err != nil {
		t.Fatal(err)
	}
	for file, expected := range map[string]string{
		"SKILL.md":            "name: rozsa-docs",
		"references/guide.md": "guide content",
		"local-note.md":       "keep local",
	} {
		contents, err := os.ReadFile(filepath.Join(existingDirectory, file))
		if err != nil {
			t.Fatalf("read %s: %v", file, err)
		}
		if !strings.Contains(string(contents), expected) {
			t.Errorf("%s = %q; want %q", file, contents, expected)
		}
	}
	missing, err = checkMissingBuiltinSkills(globalSkillDirectory, []string{"rozsa-docs"})
	if err != nil || len(missing) != 0 {
		t.Fatalf("installed skill still missing: %#v err=%v", missing, err)
	}
	if err := downloadBuiltinSkills(context.Background(), globalSkillDirectory, missing, source); err != nil {
		t.Fatal(err)
	}
	contents, err := os.ReadFile(localNote)
	if err != nil || string(contents) != "keep local" {
		t.Fatalf("existing local file changed: %q %v", contents, err)
	}
}

func TestBuiltinSkillMarkerIsPublishedOnlyAfterOtherFilesInstall(t *testing.T) {
	server := newBuiltinContentsServer(t, map[string]any{
		"rozsa-docs": []map[string]string{
			{"name": "SKILL.md", "type": "file", "download_url": "/raw/rozsa-docs/SKILL.md"},
			{"name": "references", "type": "dir"},
		},
		"rozsa-docs/references": []map[string]string{{"name": "guide.md", "type": "file", "download_url": "/raw/rozsa-docs/references/guide.md"}},
	})
	defer server.Close()
	globalSkillDirectory := filepath.Join(t.TempDir(), "global", "skills")
	destination := filepath.Join(globalSkillDirectory, ".builtin", "rozsa-docs")
	if err := os.MkdirAll(destination, 0o700); err != nil {
		t.Fatal(err)
	}
	blocker := filepath.Join(destination, "references")
	if err := os.WriteFile(blocker, []byte("keep this file"), 0o600); err != nil {
		t.Fatal(err)
	}
	source := builtinSource{baseURL: server.URL + "/contents/builtin-skills", client: server.Client()}
	missing := []string{"rozsa-docs"}
	if err := downloadBuiltinSkills(context.Background(), globalSkillDirectory, missing, source); err == nil {
		t.Fatal("installation succeeded despite a conflicting auxiliary path")
	}
	marker := filepath.Join(destination, "SKILL.md")
	if _, err := os.Lstat(marker); !errors.Is(err, os.ErrNotExist) {
		t.Fatalf("SKILL.md was published before auxiliary installation completed: %v", err)
	}
	missing, err := checkMissingBuiltinSkills(globalSkillDirectory, []string{"rozsa-docs"})
	if err != nil || !reflect.DeepEqual(missing, []string{"rozsa-docs"}) {
		t.Fatalf("incomplete install was not reported as missing: %#v, err=%v", missing, err)
	}
	if err := os.Remove(blocker); err != nil {
		t.Fatal(err)
	}
	if err := downloadBuiltinSkills(context.Background(), globalSkillDirectory, missing, source); err != nil {
		t.Fatalf("retry installation: %v", err)
	}
	for _, file := range []string{"SKILL.md", "references/guide.md"} {
		if _, err := os.Stat(filepath.Join(destination, file)); err != nil {
			t.Errorf("installed file %s: %v", file, err)
		}
	}
}

func TestBuiltinSkillRejectsUnsafeRemotePath(t *testing.T) {
	server := newBuiltinContentsServer(t, map[string]any{
		"":                      []map[string]string{{"name": "rozsa-docs", "type": "dir"}},
		"rozsa-docs":            []map[string]string{{"name": "SKILL.md", "type": "file", "download_url": "/raw/skill.md"}, {"name": "references", "type": "dir"}},
		"rozsa-docs/references": []map[string]string{{"name": "../escape", "type": "file", "download_url": "/raw/escape"}},
	})
	defer server.Close()
	globalSkillDirectory := filepath.Join(t.TempDir(), "global", "skills")
	source := builtinSource{baseURL: server.URL + "/contents/builtin-skills", client: server.Client()}
	missing, err := checkMissingBuiltinSkills(globalSkillDirectory, []string{"rozsa-docs"})
	if err != nil {
		t.Fatal(err)
	}
	if err := downloadBuiltinSkills(context.Background(), globalSkillDirectory, missing, source); err == nil {
		t.Fatal("unsafe remote entry was accepted")
	}
	if _, err := os.Stat(filepath.Join(filepath.Dir(globalSkillDirectory), "escape")); !errors.Is(err, os.ErrNotExist) {
		t.Fatalf("unsafe remote entry wrote outside destination: %v", err)
	}
}

func TestSkillDiscoveryDownloadsMissingBuiltins(t *testing.T) {
	root := t.TempDir()
	global := filepath.Join(root, "config")
	home := filepath.Join(root, "home")
	cwd := filepath.Join(root, "workspace")
	resolver := func(projectDir string) (configpaths.Roots, error) {
		return configpaths.Resolve(projectDir, &global, nil, &home, "darwin")
	}
	server := newBuiltinContentsServer(t, map[string]any{
		"rozsa-docs": []map[string]string{{"name": "SKILL.md", "type": "file", "download_url": "/raw/rozsa-docs/SKILL.md"}},
	})
	defer server.Close()
	application := NewApplicationWithBuiltinURL(nil, resolver, server.URL+"/contents/builtin-skills", server.Client(), nil)
	loaded, err := application.Enabled(cwd, nil)
	if err != nil {
		t.Fatal(err)
	}
	if len(loaded.Skills) != 1 || loaded.Skills[0].Name != "rozsa-docs" {
		t.Fatalf("installed built-in skill not found through global scan: %#v", loaded.Skills)
	}
}

func TestBuiltinConfigAndInitializationErrorNotification(t *testing.T) {
	config, err := readBuiltinConfig()
	if err != nil {
		t.Fatal(err)
	}
	url, err := builtinSkillsURL(config.BuiltinSkillsURL)
	if err != nil || url != "https://api.github.com/repos/daphnee-ovo/rozsa-demo/contents/backend/resources/builtin-skills?ref=main" {
		t.Fatalf("embedded config URL = %q err=%v", url, err)
	}
	if !reflect.DeepEqual(config.BuiltinSkills, []string{"rozsa-docs"}) {
		t.Fatalf("embedded built-in skill names = %#v", config.BuiltinSkills)
	}

	server := httptest.NewServer(http.HandlerFunc(func(response http.ResponseWriter, _ *http.Request) {
		http.Error(response, "upstream unavailable", http.StatusServiceUnavailable)
	}))
	defer server.Close()
	global := t.TempDir()
	resolver := func(projectDir string) (configpaths.Roots, error) {
		return configpaths.Resolve(projectDir, &global, nil, nil, "darwin")
	}
	writeSkill(t, filepath.Join(global, "skills"), "available", "---\nname: available\ndescription: local skill\n---\nbody")
	var notification error
	application := NewApplicationWithBuiltinURL(nil, resolver, server.URL, server.Client(), func(err error) { notification = err })
	loaded, err := application.Enabled(t.TempDir(), nil)
	if err != nil || notification == nil || len(loaded.Skills) != 1 || loaded.Skills[0].Name != "available" || len(loaded.Diagnostics) == 0 {
		t.Fatalf("initialization error=%v notification=%v", err, notification)
	}
}

func newBuiltinContentsServer(t *testing.T, listings map[string]any) *httptest.Server {
	t.Helper()
	var server *httptest.Server
	server = httptest.NewServer(http.HandlerFunc(func(response http.ResponseWriter, request *http.Request) {
		if strings.HasPrefix(request.URL.Path, "/raw/") {
			if request.URL.Path == "/raw/rozsa-docs/SKILL.md" || request.URL.Path == "/raw/skill.md" {
				_, _ = response.Write([]byte("---\nname: rozsa-docs\ndescription: docs\n---\n"))
				return
			}
			_, _ = response.Write([]byte("guide content"))
			return
		}
		if !strings.HasPrefix(request.URL.Path, "/contents/builtin-skills") {
			http.NotFound(response, request)
			return
		}
		key := strings.TrimPrefix(strings.TrimPrefix(request.URL.Path, "/contents/builtin-skills"), "/")
		listing, exists := listings[key]
		if !exists {
			http.NotFound(response, request)
			return
		}
		if entries, ok := listing.([]map[string]string); ok {
			for _, entry := range entries {
				if downloadURL := entry["download_url"]; strings.HasPrefix(downloadURL, "/") {
					entry["download_url"] = server.URL + downloadURL
				}
			}
		}
		encoded, err := json.Marshal(listing)
		if err != nil {
			t.Fatalf("encode listing: %v", err)
		}
		_, _ = response.Write(encoded)
	}))
	return server
}

func TestDisabledSkillIsRemovedAndMissingSettingMeansEnabled(t *testing.T) {
	root := t.TempDir()
	writeSkill(t, root, "keep", "---\ndescription: keep\n---\nbody")
	writeSkill(t, root, "disable", "---\ndescription: disable\n---\nbody")
	result := Load([]Source{{root, Project}}, map[string]bool{"disable": false})
	if len(result.Skills) != 1 || result.Skills[0].Name != "keep" {
		t.Fatalf("unexpected enabled skills: %#v", result.Skills)
	}
}

func TestStripFrontmatter(t *testing.T) {
	if got := StripFrontmatter("---\nname: x\n---\n\nbody"); got != "\nbody" {
		t.Fatalf("body = %q", got)
	}
	input := "plain markdown"
	if got := StripFrontmatter(input); got != input {
		t.Fatalf("plain content changed: %q", got)
	}
}

func TestExpandCommandsInjectsKnownSkillsAndKeepsUnknownTokens(t *testing.T) {
	root := t.TempDir()
	writeSkill(t, root, "deploy", "---\ndescription: deploy\n---\n\nDo the deploy.")
	result := Load([]Source{{root, Project}}, nil)
	expanded, display, changed := ExpandCommands("prefix /skill:unknown then /skill:deploy suffix", result.Skills)
	if !changed || display != "prefix /skill:unknown then /skill:deploy suffix" {
		t.Fatalf("display=%q changed=%v", display, changed)
	}
	for _, expected := range []string{"/skill:unknown", "<name>deploy</name>", "Do the deploy.", result.Skills[0].BaseDir} {
		if !contains(expanded, expected) {
			t.Fatalf("expanded text missing %q: %s", expected, expanded)
		}
	}
	unchanged, noDisplay, changed := ExpandCommands("inline/skill:deploy", result.Skills)
	if changed || noDisplay != "" || unchanged != "inline/skill:deploy" {
		t.Fatalf("inline token unexpectedly expanded: %q %q %v", unchanged, noDisplay, changed)
	}
}

func contains(value, part string) bool {
	for index := 0; index+len(part) <= len(value); index++ {
		if value[index:index+len(part)] == part {
			return true
		}
	}
	return false
}
