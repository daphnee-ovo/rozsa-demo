package resources

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func writeContext(t *testing.T, directory, filename, content string) {
	t.Helper()
	if err := os.MkdirAll(directory, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(directory, filename), []byte(content), 0o644); err != nil {
		t.Fatal(err)
	}
}

func TestLoadUsesConfiguredOverrideAndRootToCWDOrder(t *testing.T) {
	root := t.TempDir()
	global, project, cwd := filepath.Join(root, "global"), filepath.Join(root, "project"), filepath.Join(root, "workspace", "nested")
	writeContext(t, global, "AGENTS.md", "global config")
	writeContext(t, project, "AGENTS.md", "project config")
	writeContext(t, root, "CLAUDE.md", "root context")
	writeContext(t, filepath.Join(root, "workspace"), "AGENTS.md", "workspace context")
	writeContext(t, cwd, "CLAUDE.md", "cwd context")

	result := Load(cwd, []string{global, project})
	if len(result.Resources) != 4 {
		t.Fatalf("resources = %#v", result.Resources)
	}
	got := []string{result.Resources[0].Content, result.Resources[1].Content, result.Resources[2].Content, result.Resources[3].Content}
	want := []string{"project config", "root context", "workspace context", "cwd context"}
	for index := range want {
		if got[index] != want[index] {
			t.Fatalf("resource order/content = %#v", got)
		}
	}
}

func TestLoadUsesGlobalWhenProjectMissingAndFilenamePriority(t *testing.T) {
	root := t.TempDir()
	global, cwd := filepath.Join(root, "global"), filepath.Join(root, "workspace")
	writeContext(t, global, "AGENTS.md", "global")
	writeContext(t, cwd, "AGENTS.md", "agents")
	writeContext(t, cwd, "CLAUDE.md", "claude")
	result := Load(cwd, []string{global, filepath.Join(root, "missing")})
	if len(result.Resources) != 2 || result.Resources[0].Content != "global" || result.Resources[1].Content != "agents" {
		t.Fatalf("unexpected resolution: %#v", result.Resources)
	}
}

func TestLoadSkipsUnreadablePreferredCandidateAndRecordsDiagnostic(t *testing.T) {
	root := t.TempDir()
	if err := os.MkdirAll(filepath.Join(root, "AGENTS.md"), 0o755); err != nil {
		t.Fatal(err)
	}
	writeContext(t, root, "CLAUDE.md", "fallback")
	result := Load(root, nil)
	if len(result.Resources) != 1 || result.Resources[0].Content != "fallback" || len(result.Diagnostics) == 0 {
		t.Fatalf("resources=%#v diagnostics=%#v", result.Resources, result.Diagnostics)
	}
}

func TestSystemPromptFormatsSourceAndContent(t *testing.T) {
	formatted := NewSystemPrompt("base prompt").Format([]Resource{{Path: "/work/AGENTS.md", Content: "rules", Source: AgentsMD}})
	for _, expected := range []string{"base prompt", "# AGENTS.md", "From: /work/AGENTS.md", "rules"} {
		if !strings.Contains(formatted, expected) {
			t.Fatalf("prompt missing %q: %s", expected, formatted)
		}
	}
}
