package skills_test

import (
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"reflect"
	. "rozsa/backend/internal/app/skills"
	"runtime"
	"strings"
	"testing"

	"rozsa/backend/internal/platform/configpaths"
)

func copySkillFixture(t *testing.T, root, directory, fixtureName string) {
	t.Helper()
	fixtureContents, err := os.ReadFile("../../../../testdata/fixtures/backend/skills/skills.json")
	if err != nil {
		t.Fatal(err)
	}
	var fixtures map[string]string
	if err := json.Unmarshal(fixtureContents, &fixtures); err != nil {
		t.Fatal(err)
	}
	contents, exists := fixtures[fixtureName]
	if !exists {
		t.Fatalf("missing skill fixture %q", fixtureName)
	}
	path := filepath.Join(root, directory)
	if err := os.MkdirAll(path, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(path, "SKILL.md"), []byte(contents), 0o644); err != nil {
		t.Fatal(err)
	}
}

func testProjectCWD(t *testing.T) string {
	t.Helper()
	_, sourceFile, _, ok := runtime.Caller(0)
	if !ok {
		t.Fatal("could not locate skills test source")
	}
	repositoryRoot := filepath.Clean(filepath.Join(filepath.Dir(sourceFile), "../../../.."))
	targetDirectory := filepath.Join(repositoryRoot, "testdata", "project-test-dirs", "skills")
	if err := os.MkdirAll(targetDirectory, 0o700); err != nil {
		t.Fatal(err)
	}
	cwd, err := os.MkdirTemp(targetDirectory, "skills-test-")
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = os.RemoveAll(cwd) })
	return cwd
}

func TestLoadLayerPriorityDiagnosticsAndSlashNames(t *testing.T) {
	root := t.TempDir()
	user, agents, project := filepath.Join(root, "user"), filepath.Join(root, "agents"), filepath.Join(root, "project")
	copySkillFixture(t, user, "deploy", "userDeploy")
	copySkillFixture(t, agents, "deploy", "agentsDeploy")
	copySkillFixture(t, project, "deploy", "projectDeploy")
	copySkillFixture(t, project, "lint", "directLint")
	copySkillFixture(t, project, "nested/lint", "nestedLint")
	copySkillFixture(t, project, "invalid", "invalid")
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
	project := t.TempDir()
	cwd := testProjectCWD(t)
	resolver := func(projectDir string) (configpaths.Roots, error) {
		return configpaths.Resolve(projectDir, &global, &project, nil, "darwin")
	}
	roots, err := resolver(cwd)
	if err != nil {
		t.Fatal(err)
	}
	copySkillFixture(t, roots.SkillDirs()[0], ".builtin/rozsa-docs", "builtinDocs")
	copySkillFixture(t, roots.SkillDirs()[0], ".builtin/not-in-compiled-list", "unlistedBuiltin")
	application := NewApplication(nil, resolver, nil)
	loaded, err := application.Enabled(cwd, nil)
	if err != nil {
		t.Fatal(err)
	}
	if len(loaded.Skills) != 1 || loaded.Skills[0].Scope != Builtin || loaded.Skills[0].VariablePath() != "$USER_SKILLS/.builtin/rozsa-docs/SKILL.md" {
		t.Fatalf("built-in skill was not discovered with its virtual path: %#v", loaded.Skills)
	}

	copySkillFixture(t, roots.SkillDirs()[0], "rozsa-docs", "userDocs")
	loaded, err = application.Enabled(cwd, nil)
	if err != nil {
		t.Fatal(err)
	}
	if len(loaded.Skills) != 1 || loaded.Skills[0].Scope != User || loaded.Skills[0].Description != "user docs" {
		t.Fatalf("regular global skill did not override its built-in duplicate: %#v", loaded.Skills)
	}
}

func TestBuiltinSkillDownloadsEntireDirectoryAndFillsMissingFilesWithoutOverwrite(t *testing.T) {
	server := newBuiltinContentsServer(t, "nested")
	defer server.Close()
	root := t.TempDir()
	global, project, cwd := filepath.Join(root, "global"), filepath.Join(root, "project"), testProjectCWD(t)
	resolver := func(projectDir string) (configpaths.Roots, error) {
		return configpaths.Resolve(projectDir, &global, &project, nil, "darwin")
	}
	roots, err := resolver(cwd)
	if err != nil {
		t.Fatal(err)
	}
	globalSkillDirectory := roots.SkillDirs()[0]
	existingDirectory := filepath.Join(globalSkillDirectory, ".builtin", "rozsa-docs")
	if err := os.MkdirAll(existingDirectory, 0o700); err != nil {
		t.Fatal(err)
	}
	localNote := filepath.Join(existingDirectory, "local-note.md")
	if err := os.WriteFile(localNote, []byte("keep local"), 0o600); err != nil {
		t.Fatal(err)
	}
	application := NewApplicationWithBuiltinURL(nil, resolver, server.URL+"/contents/builtin-skills", server.Client(), nil)
	loaded, err := application.Enabled(cwd, nil)
	if err != nil || len(loaded.Skills) != 1 || loaded.Skills[0].Name != "rozsa-docs" {
		t.Fatalf("downloaded builtin result=%#v err=%v", loaded, err)
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
	contents, err := os.ReadFile(localNote)
	if err != nil || string(contents) != "keep local" {
		t.Fatalf("existing local file changed: %q %v", contents, err)
	}
}

func TestBuiltinSkillMarkerIsPublishedOnlyAfterOtherFilesInstall(t *testing.T) {
	server := newBuiltinContentsServer(t, "nested")
	defer server.Close()
	root := t.TempDir()
	global, project, cwd := filepath.Join(root, "global"), filepath.Join(root, "project"), testProjectCWD(t)
	resolver := func(projectDir string) (configpaths.Roots, error) {
		return configpaths.Resolve(projectDir, &global, &project, nil, "darwin")
	}
	roots, err := resolver(cwd)
	if err != nil {
		t.Fatal(err)
	}
	globalSkillDirectory := roots.SkillDirs()[0]
	destination := filepath.Join(globalSkillDirectory, ".builtin", "rozsa-docs")
	if err := os.MkdirAll(destination, 0o700); err != nil {
		t.Fatal(err)
	}
	blocker := filepath.Join(destination, "references")
	if err := os.WriteFile(blocker, []byte("keep this file"), 0o600); err != nil {
		t.Fatal(err)
	}
	application := NewApplicationWithBuiltinURL(nil, resolver, server.URL+"/contents/builtin-skills", server.Client(), nil)
	failed, err := application.Enabled(cwd, nil)
	builtinPath := filepath.Join(globalSkillDirectory, ".builtin")
	if err != nil || len(failed.Diagnostics) != 1 || failed.Diagnostics[0].Path != builtinPath || !strings.Contains(failed.Diagnostics[0].Message, "references") {
		t.Fatalf("conflicting installation result=%#v err=%v", failed, err)
	}
	marker := filepath.Join(destination, "SKILL.md")
	if _, err := os.Lstat(marker); !errors.Is(err, os.ErrNotExist) {
		t.Fatalf("SKILL.md was published before auxiliary installation completed: %v", err)
	}
	if err := os.Remove(blocker); err != nil {
		t.Fatal(err)
	}
	recovered, err := application.Enabled(cwd, nil)
	if err != nil || len(recovered.Skills) != 1 {
		t.Fatalf("retry installation result=%#v err=%v", recovered, err)
	}
	for _, file := range []string{"SKILL.md", "references/guide.md"} {
		if _, err := os.Stat(filepath.Join(destination, file)); err != nil {
			t.Errorf("installed file %s: %v", file, err)
		}
	}
}

func TestBuiltinSkillRejectsUnsafeRemotePath(t *testing.T) {
	server := newBuiltinContentsServer(t, "unsafe")
	defer server.Close()
	root := t.TempDir()
	global, project, cwd := filepath.Join(root, "global"), filepath.Join(root, "project"), testProjectCWD(t)
	resolver := func(projectDir string) (configpaths.Roots, error) {
		return configpaths.Resolve(projectDir, &global, &project, nil, "darwin")
	}
	roots, err := resolver(cwd)
	if err != nil {
		t.Fatal(err)
	}
	application := NewApplicationWithBuiltinURL(nil, resolver, server.URL+"/contents/builtin-skills", server.Client(), nil)
	loaded, err := application.Enabled(cwd, nil)
	if err != nil || len(loaded.Diagnostics) != 1 || loaded.Diagnostics[0].Path != filepath.Join(roots.SkillDirs()[0], ".builtin") || loaded.Diagnostics[0].Message != `inspect remote built-in skill "rozsa-docs": GitHub Contents returned unsafe name "../escape"` {
		t.Fatalf("unsafe remote entry was not reported: %#v err=%v", loaded, err)
	}
	if _, err := os.Stat(filepath.Join(roots.SkillDirs()[0], ".builtin", "escape")); !errors.Is(err, os.ErrNotExist) {
		t.Fatalf("unsafe remote entry wrote outside destination: %v", err)
	}
}

func TestSkillDiscoveryDownloadsMissingBuiltins(t *testing.T) {
	root := t.TempDir()
	global := filepath.Join(root, "config")
	home := filepath.Join(root, "home")
	cwd := testProjectCWD(t)
	resolver := func(projectDir string) (configpaths.Roots, error) {
		return configpaths.Resolve(projectDir, &global, nil, &home, "darwin")
	}
	server := newBuiltinContentsServer(t, "simple")
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
	server := httptest.NewServer(http.HandlerFunc(func(response http.ResponseWriter, _ *http.Request) {
		http.Error(response, "upstream unavailable", http.StatusServiceUnavailable)
	}))
	defer server.Close()
	global := t.TempDir()
	resolver := func(projectDir string) (configpaths.Roots, error) {
		return configpaths.Resolve(projectDir, &global, nil, nil, "darwin")
	}
	copySkillFixture(t, filepath.Join(global, "skills"), "available", "available")
	var notification error
	application := NewApplicationWithBuiltinURL(nil, resolver, server.URL, server.Client(), func(err error) { notification = err })
	loaded, err := application.Enabled(testProjectCWD(t), nil)
	if err != nil || notification == nil || len(loaded.Skills) != 1 || loaded.Skills[0].Name != "available" || len(loaded.Diagnostics) == 0 {
		t.Fatalf("initialization error=%v notification=%v", err, notification)
	}
}

func newBuiltinContentsServer(t *testing.T, listingFixture string) *httptest.Server {
	t.Helper()
	fixtureData, err := os.ReadFile("../../../../testdata/fixtures/backend/skills/builtin-http.json")
	if err != nil {
		t.Fatal(err)
	}
	var fixtures map[string]map[string]json.RawMessage
	if err := json.Unmarshal(fixtureData, &fixtures); err != nil {
		t.Fatal(err)
	}
	listings, exists := fixtures[listingFixture]
	if !exists {
		t.Fatalf("missing builtin HTTP fixture %q", listingFixture)
	}
	var server *httptest.Server
	server = httptest.NewServer(http.HandlerFunc(func(response http.ResponseWriter, request *http.Request) {
		if strings.HasPrefix(request.URL.Path, "/raw/") {
			fixturePath := "../../../../testdata/fixtures/backend/skills/guide.md"
			if strings.HasSuffix(request.URL.Path, "/SKILL.md") {
				fixturePath = "../../../../testdata/fixtures/backend/skills/skills.json"
				contents, err := os.ReadFile(fixturePath)
				if err != nil {
					t.Errorf("read skill fixtures: %v", err)
					response.WriteHeader(http.StatusInternalServerError)
					return
				}
				var skillFixtures map[string]string
				if err := json.Unmarshal(contents, &skillFixtures); err != nil {
					t.Errorf("decode skill fixtures: %v", err)
					response.WriteHeader(http.StatusInternalServerError)
					return
				}
				_, _ = response.Write([]byte(skillFixtures["builtinDocs"]))
				return
			}
			contents, err := os.ReadFile(fixturePath)
			if err != nil {
				t.Errorf("read guide fixture: %v", err)
				response.WriteHeader(http.StatusInternalServerError)
				return
			}
			_, _ = response.Write(contents)
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
		var entries []map[string]string
		if err := json.Unmarshal(listing, &entries); err != nil {
			t.Errorf("decode listing fixture: %v", err)
			response.WriteHeader(http.StatusInternalServerError)
			return
		}
		for _, entry := range entries {
			if downloadURL := entry["download_url"]; strings.HasPrefix(downloadURL, "/") {
				entry["download_url"] = server.URL + downloadURL
			}
		}
		encoded, err := json.Marshal(entries)
		if err != nil {
			t.Fatalf("encode listing: %v", err)
		}
		_, _ = response.Write(encoded)
	}))
	return server
}

func TestDisabledSkillIsRemovedAndMissingSettingMeansEnabled(t *testing.T) {
	root := t.TempDir()
	copySkillFixture(t, root, "keep", "keep")
	copySkillFixture(t, root, "disable", "disable")
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
	copySkillFixture(t, root, "deploy", "deployCommand")
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
