// FrameworkTree
// config_paths.rs
// ├── enum ConfigPathError
// ├── struct ConfigRoots
// ├── impl ConfigRoots
// ├── discover()
// ├── from_roots()
// ├── global()
// ├── project()
// ├── settings_paths()
// ├── model_dirs()
// ├── theme_dirs()
// ├── skill_dirs()
// ├── agents_skills_dir()
// ├── resource_dirs()
// ├── session_dirs()
// ├── writable_session_dir()
// ├── global_models_dir()
// ├── from_overrides()
// ├── resolve_global_root()
// └── encode_project_path()

use std::path::{Path, PathBuf};

use thiserror::Error;

pub const GLOBAL_CONFIG_DIR_ENV: &str = "ROZSA_CONFIG_DIR";
pub const PROJECT_CONFIG_DIR_ENV: &str = "ROZSA_PROJECT_CONFIG_DIR";

#[derive(Debug, Error)]
pub enum ConfigPathError {
    #[error(
        "Cannot determine the global Rózsa config directory: set {GLOBAL_CONFIG_DIR_ENV} or make the home directory available"
    )]
    HomeDirectoryUnavailable,
    #[error("{variable} must not be empty")]
    EmptyDirectory { variable: &'static str },
}

/// Resolved global and project configuration roots.
///
/// Every configuration category uses the same relative path under both roots.
/// Readers must load the global layer first and the project layer second.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigRoots {
    global: PathBuf,
    project: PathBuf,
    agents_skills: Option<PathBuf>,
}

impl ConfigRoots {
    pub fn discover(project_dir: &Path) -> Result<Self, ConfigPathError> {
        Self::from_overrides(
            project_dir,
            std::env::var_os(GLOBAL_CONFIG_DIR_ENV).map(PathBuf::from),
            std::env::var_os(PROJECT_CONFIG_DIR_ENV).map(PathBuf::from),
            dirs_next::home_dir(),
        )
    }

    pub fn from_roots(global: PathBuf, project: PathBuf) -> Self {
        Self {
            global,
            project,
            agents_skills: None,
        }
    }

    pub fn global(&self) -> &Path {
        &self.global
    }

    pub fn project(&self) -> &Path {
        &self.project
    }

    pub fn settings_paths(&self) -> [PathBuf; 2] {
        [
            self.global.join("settings.json"),
            self.project.join("settings.json"),
        ]
    }

    pub fn model_dirs(&self) -> [PathBuf; 2] {
        [self.global.join("models"), self.project.join("models")]
    }

    pub fn theme_dirs(&self) -> [PathBuf; 2] {
        [self.global.join("themes"), self.project.join("themes")]
    }

    pub fn skill_dirs(&self) -> [PathBuf; 2] {
        [self.global.join("skills"), self.project.join("skills")]
    }

    /// Legacy user-wide skill directory kept for compatibility with existing skills.
    pub fn agents_skills_dir(&self) -> Option<&Path> {
        self.agents_skills.as_deref()
    }

    pub fn resource_dirs(&self) -> [PathBuf; 2] {
        [self.global.clone(), self.project.clone()]
    }

    pub fn session_dirs(&self, project_dir: &Path) -> [PathBuf; 2] {
        let project_key = encode_project_path(project_dir);
        [
            self.global.join("sessions").join(&project_key),
            self.project.join("sessions").join(project_key),
        ]
    }

    pub fn writable_session_dir(&self, project_dir: &Path) -> PathBuf {
        self.global
            .join("sessions")
            .join(encode_project_path(project_dir))
    }

    pub fn global_models_dir() -> Result<PathBuf, ConfigPathError> {
        let global_override = std::env::var_os(GLOBAL_CONFIG_DIR_ENV).map(PathBuf::from);
        let global = resolve_global_root(global_override, dirs_next::home_dir())?;
        Ok(global.join("models"))
    }

    pub fn from_overrides(
        project_dir: &Path,
        global_override: Option<PathBuf>,
        project_override: Option<PathBuf>,
        home: Option<PathBuf>,
    ) -> Result<Self, ConfigPathError> {
        let agents_skills = home
            .as_ref()
            .map(|path| path.join(".agents").join("skills"));
        let global = resolve_global_root(global_override, home)?;
        if project_override
            .as_ref()
            .is_some_and(|path| path.as_os_str().is_empty())
        {
            return Err(ConfigPathError::EmptyDirectory {
                variable: PROJECT_CONFIG_DIR_ENV,
            });
        }
        let project = project_override.unwrap_or_else(|| project_dir.join(".rozsa"));
        Ok(Self {
            global,
            project,
            agents_skills,
        })
    }
}

fn resolve_global_root(
    global_override: Option<PathBuf>,
    home: Option<PathBuf>,
) -> Result<PathBuf, ConfigPathError> {
    match global_override {
        Some(path) if path.as_os_str().is_empty() => Err(ConfigPathError::EmptyDirectory {
            variable: GLOBAL_CONFIG_DIR_ENV,
        }),
        Some(path) => Ok(path),
        None => Ok(home
            .ok_or(ConfigPathError::HomeDirectoryUnavailable)?
            .join(".rozsa")),
    }
}

pub fn encode_project_path(project_dir: &Path) -> String {
    let encoded = project_dir
        .to_string_lossy()
        .replace(['/', '\\'], "-")
        .trim_matches('-')
        .to_string();
    format!("-{encoded}-")
}
