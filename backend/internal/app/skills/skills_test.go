package skills

import (
	"os"
	"path/filepath"
	"reflect"
	"testing"
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
	writeSkill(t, project, "nested/lint", "---\ndescription: |\n  First line\n  second line\n---\nbody")
	writeSkill(t, project, "invalid", "---\nname: invalid\n---\nmissing description")

	result := Load([]Source{{user, User}, {agents, Agents}, {project, Project}}, nil)
	if len(result.Skills) != 2 || len(result.Diagnostics) != 1 {
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
	if names := result.SlashCommandNames(); !reflect.DeepEqual(names, []string{"deploy", "lint"}) {
		t.Fatalf("slash names = %#v", names)
	}
	if got := result.PromptFragment(); got == "" || !contains(got, "$PROJECT_SKILLS/deploy/SKILL.md") {
		t.Fatalf("prompt fragment missing skill path: %q", got)
	}
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
