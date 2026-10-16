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
	global       string
	project      string
	agentsSkills string
	goos         string
}

func Discover(projectDir string) (Roots, error) {
	home, err := os.UserHomeDir()
	if err != nil {
		home = ""
	}
	global, globalSet := os.LookupEnv(GlobalConfigDirEnv)
	project, projectSet := os.LookupEnv(ProjectConfigDirEnv)
	return Resolve(projectDir, optional(global, globalSet), optional(project, projectSet), optional(home, home != ""), runtime.GOOS)
}

func optional(value string, set bool) *string {
	if !set {
		return nil
	}
	return &value
}

// Resolve accepts the host family explicitly to make path environment behavior
// testable without reading the process environment or real home directory.
func Resolve(projectDir string, globalOverride, projectOverride, home *string, goos string) (Roots, error) {
	join := pathJoin(goos)
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
		global = join(*home, ".rozsa")
	}
	project := ""
	if projectOverride != nil {
		project = *projectOverride
	} else {
		project = join(projectDir, ".rozsa")
	}
	agentsSkills := ""
	if home != nil && *home != "" {
		agentsSkills = join(*home, ".agents", "skills")
	}
	return Roots{global: global, project: project, agentsSkills: agentsSkills, goos: goos}, nil
}

func (r Roots) join(parts ...string) string { return pathJoin(r.goos)(parts...) }

func pathJoin(goos string) func(...string) string {
	if goos != "windows" {
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

func (r Roots) Global() string               { return r.global }
func (r Roots) Project() string              { return r.project }
func (r Roots) AgentsSkills() (string, bool) { return r.agentsSkills, r.agentsSkills != "" }

func (r Roots) SettingsPaths() [2]string {
	return [2]string{r.join(r.global, "settings.json"), r.join(r.project, "settings.json")}
}
func (r Roots) ModelDirs() [2]string {
	return [2]string{r.join(r.global, "models"), r.join(r.project, "models")}
}
func (r Roots) ThemeDirs() [2]string {
	return [2]string{r.join(r.global, "themes"), r.join(r.project, "themes")}
}
func (r Roots) SkillDirs() [2]string {
	return [2]string{r.join(r.global, "skills"), r.join(r.project, "skills")}
}
func (r Roots) ResourceDirs() [2]string { return [2]string{r.global, r.project} }
func (r Roots) SessionDirs(projectDir string) [2]string {
	key := ProjectKey(projectDir)
	return [2]string{r.join(r.global, "sessions", key), r.join(r.project, "sessions", key)}
}
func (r Roots) WritableSessionDir(projectDir string) string {
	return r.join(r.global, "sessions", ProjectKey(projectDir))
}

func GlobalModelsDir() (string, error) {
	global, set := os.LookupEnv(GlobalConfigDirEnv)
	var override *string
	if set {
		override = &global
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
