package configpaths

import (
	"reflect"
	"testing"
)

func stringPointer(value string) *string { return &value }

func TestResolveUsesExplicitRootsAndHomeDefaults(testContext *testing.T) {
	roots, err := Resolve("/workspace", nil, nil, stringPointer("/home/user"), "darwin")
	if err != nil {
		testContext.Fatal(err)
	}
	if roots.Global() != "/home/user/.rozsa" || roots.Project() != "/workspace/.rozsa" {
		testContext.Fatalf("unexpected roots: %#v", roots)
	}
	if agentsPath, exists := roots.AgentsSkills(); !exists || agentsPath != "/home/user/.agents/skills" {
		testContext.Fatalf("agents skills path: %q %v", agentsPath, exists)
	}
	explicit, err := Resolve("/workspace", stringPointer("/private/config"), stringPointer("/private/project"), stringPointer("/ignored"), "linux")
	if err != nil {
		testContext.Fatal(err)
	}
	if explicit.Global() != "/private/config" || explicit.Project() != "/private/project" {
		testContext.Fatalf("overrides: %#v", explicit)
	}
}

func TestResolveRejectsEmptyOverridesAndMissingHome(testContext *testing.T) {
	if _, err := Resolve("/workspace", nil, nil, nil, "darwin"); err == nil {
		testContext.Fatal("expected missing-home error")
	}
	for _, testCase := range []struct {
		global, project *string
		expected        string
	}{
		{stringPointer(""), nil, GlobalConfigDirEnv}, {nil, stringPointer(""), ProjectConfigDirEnv},
	} {
		if _, err := Resolve("/workspace", testCase.global, testCase.project, stringPointer("/home/user"), "linux"); err == nil || err.Error() != testCase.expected+" must not be empty" {
			testContext.Fatalf("error = %v", err)
		}
	}
}

func TestPathLayoutIsSharedAcrossSupportedPathFamilies(testContext *testing.T) {
	cases := []struct{ operatingSystem, home, project, expectedGlobal, expectedProject, expectedKey string }{
		{"darwin", "/Users/test", "/workspace/project", "/Users/test/.rozsa", "/workspace/project/.rozsa", "-workspace-project-"},
		{"linux", "/home/test", "/srv/work/project", "/home/test/.rozsa", "/srv/work/project/.rozsa", "-srv-work-project-"},
		{"windows", `C:\Users\test`, `D:\work\project`, `C:\Users\test\.rozsa`, `D:\work\project\.rozsa`, `-D:-work-project-`},
	}
	for _, testCase := range cases {
		testContext.Run(testCase.operatingSystem, func(testContext *testing.T) {
			roots, err := Resolve(testCase.project, nil, nil, stringPointer(testCase.home), testCase.operatingSystem)
			if err != nil {
				testContext.Fatal(err)
			}
			if roots.Global() != testCase.expectedGlobal || roots.Project() != testCase.expectedProject {
				testContext.Fatalf("roots %q %q", roots.Global(), roots.Project())
			}
			settingsPaths := roots.SettingsPaths()
			pathSeparator := "/"
			if testCase.operatingSystem == "windows" {
				pathSeparator = `\`
			}
			expectedSettingsPaths := [2]string{testCase.expectedGlobal + pathSeparator + "settings.json", testCase.expectedProject + pathSeparator + "settings.json"}
			if !reflect.DeepEqual(settingsPaths, expectedSettingsPaths) {
				testContext.Fatalf("settings paths %q, want %q", settingsPaths, expectedSettingsPaths)
			}
			if actualKey := ProjectKey(testCase.project); actualKey != testCase.expectedKey {
				testContext.Fatalf("project key %q, want %q", actualKey, testCase.expectedKey)
			}
			if agentsPath, exists := roots.AgentsSkills(); !exists {
				testContext.Fatal("missing user-private agents skills directory")
			} else if testCase.operatingSystem == "windows" && agentsPath != `C:\Users\test\.agents\skills` {
				testContext.Fatalf("agents path %q", agentsPath)
			}
		})
	}
}

func TestEnvironmentOverridesPreservePrivatePathAndGlobalFirstOrder(testContext *testing.T) {
	roots, err := Resolve("/workspace", stringPointer("/private/user-config"), stringPointer("/private/project-config"), stringPointer("/home/user"), "darwin")
	if err != nil {
		testContext.Fatal(err)
	}
	if roots.Global() != "/private/user-config" || roots.Project() != "/private/project-config" {
		testContext.Fatalf("private overrides lost: %#v", roots)
	}
	settingsPaths := roots.SettingsPaths()
	if settingsPaths[0] != "/private/user-config/settings.json" || settingsPaths[1] != "/private/project-config/settings.json" {
		testContext.Fatalf("layer order: %#v", settingsPaths)
	}
}

func TestDiscoverReadsEnvironmentOverrides(testContext *testing.T) {
	testContext.Setenv(GlobalConfigDirEnv, "/private/global")
	testContext.Setenv(ProjectConfigDirEnv, "/private/project")
	roots, err := Discover("/workspace")
	if err != nil {
		testContext.Fatal(err)
	}
	if roots.Global() != "/private/global" || roots.Project() != "/private/project" {
		testContext.Fatalf("environment overrides: %#v", roots)
	}
}
