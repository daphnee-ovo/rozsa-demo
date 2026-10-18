// Package configpaths resolves the shared global and project configuration layout.
package configpaths

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"runtime"
	"strings"
)

const (
	GlobalConfigDirEnv  = "ROZSA_CONFIG_DIR"
	ProjectConfigDirEnv = "ROZSA_PROJECT_CONFIG_DIR"
)

var ErrHomeDirectoryUnavailable = errors.New("cannot determine the global Rózsa config directory: set ROZSA_CONFIG_DIR or make the home directory available")

type Roots struct {
	global          string
	project         string
	agentsSkills    string
	operatingSystem string
}

func Discover(projectDir string) (Roots, error) {
	home, err := os.UserHomeDir()
	if err != nil {
		home = ""
	}
	globalOverride, globalOverrideSet := os.LookupEnv(GlobalConfigDirEnv)
	projectOverride, projectOverrideSet := os.LookupEnv(ProjectConfigDirEnv)
	return Resolve(projectDir, optional(globalOverride, globalOverrideSet), optional(projectOverride, projectOverrideSet), optional(home, home != ""), runtime.GOOS)
}

func optional(providedValue string, isSet bool) *string {
	if !isSet {
		return nil
	}
	return &providedValue
}

// Resolve accepts the host family explicitly to make path environment behavior
// testable without reading the process environment or real home directory.
func Resolve(projectDir string, globalOverride, projectOverride, home *string, operatingSystem string) (Roots, error) {
	joinPath := pathJoin(operatingSystem)
	if globalOverride != nil && *globalOverride == "" {
		return Roots{}, fmt.Errorf("%s must not be empty", GlobalConfigDirEnv)
	}
	if projectOverride != nil && *projectOverride == "" {
		return Roots{}, fmt.Errorf("%s must not be empty", ProjectConfigDirEnv)
	}
	var global string
	if globalOverride != nil {
		global = *globalOverride
	} else {
		if home == nil || *home == "" {
			return Roots{}, ErrHomeDirectoryUnavailable
		}
		global = joinPath(*home, ".rozsa")
	}
	project := ""
	if projectOverride != nil {
		project = *projectOverride
	} else {
		project = joinPath(projectDir, ".rozsa")
	}
	agentsSkills := ""
	if home != nil && *home != "" {
		agentsSkills = joinPath(*home, ".agents", "skills")
	}
	return Roots{global: global, project: project, agentsSkills: agentsSkills, operatingSystem: operatingSystem}, nil
}

func (roots Roots) join(parts ...string) string { return pathJoin(roots.operatingSystem)(parts...) }

func pathJoin(operatingSystem string) func(...string) string {
	if operatingSystem != "windows" {
		return filepath.Join
	}
	return windowsJoin
}

func windowsJoin(parts ...string) string {
	if len(parts) == 0 {
		return ""
	}
	joined := strings.TrimRight(parts[0], `\/`)
	for _, part := range parts[1:] {
		if part == "" {
			continue
		}
		joined += `\` + strings.Trim(part, `\/`)
	}
	return joined
}

func (roots Roots) Global() string               { return roots.global }
func (roots Roots) Project() string              { return roots.project }
func (roots Roots) AgentsSkills() (string, bool) { return roots.agentsSkills, roots.agentsSkills != "" }

func (roots Roots) SettingsPaths() [2]string {
	return [2]string{roots.join(roots.global, "settings.json"), roots.join(roots.project, "settings.json")}
}
func (roots Roots) ModelDirs() [2]string {
	return [2]string{roots.join(roots.global, "models"), roots.join(roots.project, "models")}
}
func (roots Roots) AuthPath() string { return roots.join(roots.global, "models", "auth.json") }
func (roots Roots) ThemeDirs() [2]string {
	return [2]string{roots.join(roots.global, "themes"), roots.join(roots.project, "themes")}
}
func (roots Roots) SkillDirs() [2]string {
	return [2]string{roots.join(roots.global, "skills"), roots.join(roots.project, "skills")}
}
func (roots Roots) ResourceDirs() [2]string { return [2]string{roots.global, roots.project} }
func (roots Roots) SessionDirs(projectDir string) [2]string {
	key := ProjectKey(projectDir)
	return [2]string{roots.join(roots.global, "sessions", key), roots.join(roots.project, "sessions", key)}
}
func (roots Roots) WritableSessionDir(projectDir string) string {
	return roots.join(roots.global, "sessions", ProjectKey(projectDir))
}

func GlobalModelsDir() (string, error) {
	globalOverride, isSet := os.LookupEnv(GlobalConfigDirEnv)
	var override *string
	if isSet {
		override = &globalOverride
	}
	roots, err := Resolve("", override, nil, userHome(), runtime.GOOS)
	if err != nil {
		return "", err
	}
	return filepath.Join(roots.global, "models"), nil
}

func userHome() *string {
	home, err := os.UserHomeDir()
	if err != nil || home == "" {
		return nil
	}
	return &home
}

// ProjectKey matches the stable partition used by the Rust application on
// Unix and Windows paths, independent of the current host OS.
func ProjectKey(projectDir string) string {
	encoded := strings.NewReplacer("/", "-", `\`, "-").Replace(projectDir)
	encoded = strings.Trim(encoded, "-")
	return "-" + encoded + "-"
}
