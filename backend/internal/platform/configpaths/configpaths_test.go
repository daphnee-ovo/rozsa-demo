package configpaths

import (
	"reflect"
	"testing"
)

func ptr(value string) *string { return &value }

func TestResolveUsesExplicitRootsAndHomeDefaults(t *testing.T) {
	roots, err := Resolve("/workspace", nil, nil, ptr("/home/user"), "darwin")
	if err != nil {
		t.Fatal(err)
	}
	if roots.Global() != "/home/user/.rozsa" || roots.Project() != "/workspace/.rozsa" {
		t.Fatalf("unexpected roots: %#v", roots)
	}
	if agents, ok := roots.AgentsSkills(); !ok || agents != "/home/user/.agents/skills" {
		t.Fatalf("agents skills path: %q %v", agents, ok)
	}
	explicit, err := Resolve("/workspace", ptr("/private/config"), ptr("/private/project"), ptr("/ignored"), "linux")
	if err != nil {
		t.Fatal(err)
	}
	if explicit.Global() != "/private/config" || explicit.Project() != "/private/project" {
		t.Fatalf("overrides: %#v", explicit)
	}
}

func TestResolveRejectsEmptyOverridesAndMissingHome(t *testing.T) {
	if _, err := Resolve("/workspace", nil, nil, nil, "darwin"); err == nil {
		t.Fatal("expected missing-home error")
	}
	for _, test := range []struct {
		global, project *string
		expected        string
	}{
		{ptr(""), nil, GlobalConfigDirEnv}, {nil, ptr(""), ProjectConfigDirEnv},
	} {
		if _, err := Resolve("/workspace", test.global, test.project, ptr("/home/user"), "linux"); err == nil || err.Error() != test.expected+" must not be empty" {
			t.Fatalf("error = %v", err)
		}
	}
}

func TestPathLayoutIsSharedAcrossSupportedPathFamilies(t *testing.T) {
	cases := []struct{ goos, home, project, wantGlobal, wantProject, wantKey string }{
		{"darwin", "/Users/test", "/workspace/project", "/Users/test/.rozsa", "/workspace/project/.rozsa", "-workspace-project-"},
		{"linux", "/home/test", "/srv/work/project", "/home/test/.rozsa", "/srv/work/project/.rozsa", "-srv-work-project-"},
		{"windows", `C:\Users\test`, `D:\work\project`, `C:\Users\test\.rozsa`, `D:\work\project\.rozsa`, `-D:-work-project-`},
	}
	for _, test := range cases {
		t.Run(test.goos, func(t *testing.T) {
			roots, err := Resolve(test.project, nil, nil, ptr(test.home), test.goos)
			if err != nil {
				t.Fatal(err)
			}
			if roots.Global() != test.wantGlobal || roots.Project() != test.wantProject {
				t.Fatalf("roots %q %q", roots.Global(), roots.Project())
			}
			paths := roots.SettingsPaths()
			sep := "/"
			if test.goos == "windows" {
				sep = `\`
			}
			want := [2]string{test.wantGlobal + sep + "settings.json", test.wantProject + sep + "settings.json"}
			if !reflect.DeepEqual(paths, want) {
				t.Fatalf("settings paths %q, want %q", paths, want)
			}
			if got := ProjectKey(test.project); got != test.wantKey {
				t.Fatalf("project key %q, want %q", got, test.wantKey)
			}
			if agents, ok := roots.AgentsSkills(); !ok {
				t.Fatal("missing user-private agents skills directory")
			} else if test.goos == "windows" && agents != `C:\Users\test\.agents\skills` {
				t.Fatalf("agents path %q", agents)
			}
		})
	}
}

func TestEnvironmentOverridesPreservePrivatePathAndGlobalFirstOrder(t *testing.T) {
	roots, err := Resolve("/workspace", ptr("/private/user-config"), ptr("/private/project-config"), ptr("/home/user"), "darwin")
	if err != nil {
		t.Fatal(err)
	}
	if roots.Global() != "/private/user-config" || roots.Project() != "/private/project-config" {
		t.Fatalf("private overrides lost: %#v", roots)
	}
	paths := roots.SettingsPaths()
	if paths[0] != "/private/user-config/settings.json" || paths[1] != "/private/project-config/settings.json" {
		t.Fatalf("layer order: %#v", paths)
	}
}

func TestDiscoverReadsEnvironmentOverrides(t *testing.T) {
	t.Setenv(GlobalConfigDirEnv, "/private/global")
	t.Setenv(ProjectConfigDirEnv, "/private/project")
	roots, err := Discover("/workspace")
	if err != nil {
		t.Fatal(err)
	}
	if roots.Global() != "/private/global" || roots.Project() != "/private/project" {
		t.Fatalf("environment overrides: %#v", roots)
	}
}
