// FrameworkTree
// discovery.rs
// ├── enum DowInstallSource
// ├── struct DiscoveredDow
// ├── struct CommandOutput
// ├── enum CommandExecutionError
// ├── trait DiscoveryCommandRunner
// ├── struct SystemCommandRunner
// ├── impl SystemCommandRunner
// ├── run()
// ├── struct DiscoveryEnvironment
// ├── impl DiscoveryEnvironment
// ├── from_process()
// ├── enum DowDiscoveryError
// ├── discover_dow()
// ├── discover_dow_with()
// ├── validate_custom()
// ├── try_candidates()
// ├── validate_candidate()
// ├── helper_executables()
// ├── path_executables()
// ├── executable_names()
// ├── executable_name()
// ├── canonical_executable()
// ├── single_path_line()
// ├── npm_prefix_candidates()
// └── standard_npm_candidates()

//! Discovery and validation of the optional `dow` executable.

use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;
use semver::Version;
use thiserror::Error;
use tokio::process::Command;
use tokio::time::timeout;

use crate::settings::DevFlowSettings;

const COMMAND_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DowInstallSource {
    Custom,
    Path,
    Homebrew,
    Cargo,
    Npm,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoveredDow {
    pub executable: PathBuf,
    pub version: Version,
    pub source: DowInstallSource,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandOutput {
    pub success: bool,
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum CommandExecutionError {
    #[error("command timed out after {0:?}")]
    Timeout(Duration),
    #[error("failed to launch command: {0}")]
    Launch(String),
}

#[async_trait]
pub trait DiscoveryCommandRunner: Send + Sync {
    async fn run(
        &self,
        executable: &Path,
        args: &[&str],
        deadline: Duration,
    ) -> Result<CommandOutput, CommandExecutionError>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemCommandRunner;

#[async_trait]
impl DiscoveryCommandRunner for SystemCommandRunner {
    async fn run(
        &self,
        executable: &Path,
        args: &[&str],
        deadline: Duration,
    ) -> Result<CommandOutput, CommandExecutionError> {
        let output = timeout(deadline, Command::new(executable).args(args).output())
            .await
            .map_err(|_| CommandExecutionError::Timeout(deadline))?
            .map_err(|error| CommandExecutionError::Launch(error.to_string()))?;
        Ok(CommandOutput {
            success: output.status.success(),
            code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}

#[derive(Clone, Debug)]
pub struct DiscoveryEnvironment {
    pub path: OsString,
    pub home_dir: Option<PathBuf>,
    pub cargo_home: Option<PathBuf>,
    pub npm_config_prefix: Option<PathBuf>,
    pub app_data: Option<PathBuf>,
    pub homebrew_bin_dirs: Vec<PathBuf>,
}

impl DiscoveryEnvironment {
    pub fn from_process() -> Self {
        Self {
            path: std::env::var_os("PATH").unwrap_or_default(),
            home_dir: dirs_next::home_dir(),
            cargo_home: std::env::var_os("CARGO_HOME").map(PathBuf::from),
            npm_config_prefix: std::env::var_os("NPM_CONFIG_PREFIX").map(PathBuf::from),
            app_data: std::env::var_os("APPDATA").map(PathBuf::from),
            homebrew_bin_dirs: vec![
                PathBuf::from("/opt/homebrew/bin"),
                PathBuf::from("/usr/local/bin"),
            ],
        }
    }
}

#[derive(Debug, Error)]
pub enum DowDiscoveryError {
    #[error("configured dow path must be absolute: {0}")]
    CustomPathNotAbsolute(PathBuf),
    #[error("configured dow path is unavailable: {0}")]
    CustomPathUnavailable(PathBuf),
    #[error("configured dow executable is invalid: {path}: {reason}")]
    InvalidCustomExecutable { path: PathBuf, reason: String },
    #[error("dow was not found in PATH, Homebrew, Cargo, or npm locations")]
    NotFound,
}

pub async fn discover_dow(settings: &DevFlowSettings) -> Result<DiscoveredDow, DowDiscoveryError> {
    discover_dow_with(
        settings,
        &DiscoveryEnvironment::from_process(),
        &SystemCommandRunner,
    )
    .await
}

pub async fn discover_dow_with<R: DiscoveryCommandRunner>(
    settings: &DevFlowSettings,
    environment: &DiscoveryEnvironment,
    runner: &R,
) -> Result<DiscoveredDow, DowDiscoveryError> {
    if let Some(custom) = settings.executable_path.as_deref() {
        return validate_custom(custom, runner).await;
    }

    let mut visited = HashSet::new();
    if let Some(found) = try_candidates(
        path_executables(&environment.path, executable_names("dow")),
        DowInstallSource::Path,
        &mut visited,
        runner,
    )
    .await
    {
        return Ok(found);
    }

    let homebrew_dow = environment
        .homebrew_bin_dirs
        .iter()
        .map(|directory| directory.join(executable_name("dow")))
        .collect::<Vec<_>>();
    if let Some(found) = try_candidates(
        homebrew_dow,
        DowInstallSource::Homebrew,
        &mut visited,
        runner,
    )
    .await
    {
        return Ok(found);
    }

    for brew in helper_executables(environment, "brew") {
        let Ok(output) = runner.run(&brew, &["--prefix"], COMMAND_TIMEOUT).await else {
            continue;
        };
        if !output.success {
            continue;
        }
        let Some(prefix) = single_path_line(&output.stdout) else {
            continue;
        };
        if let Some(found) = try_candidates(
            [prefix.join("bin").join(executable_name("dow"))],
            DowInstallSource::Homebrew,
            &mut visited,
            runner,
        )
        .await
        {
            return Ok(found);
        }
    }

    let cargo_home = environment.cargo_home.clone().or_else(|| {
        environment
            .home_dir
            .as_ref()
            .map(|home| home.join(".cargo"))
    });
    if let Some(cargo_home) = cargo_home
        && let Some(found) = try_candidates(
            [cargo_home.join("bin").join(executable_name("dow"))],
            DowInstallSource::Cargo,
            &mut visited,
            runner,
        )
        .await
    {
        return Ok(found);
    }

    for npm in helper_executables(environment, "npm") {
        let Ok(output) = runner.run(&npm, &["prefix", "-g"], COMMAND_TIMEOUT).await else {
            continue;
        };
        if !output.success {
            continue;
        }
        let Some(prefix) = single_path_line(&output.stdout) else {
            continue;
        };
        if let Some(found) = try_candidates(
            npm_prefix_candidates(&prefix),
            DowInstallSource::Npm,
            &mut visited,
            runner,
        )
        .await
        {
            return Ok(found);
        }
    }

    if let Some(found) = try_candidates(
        standard_npm_candidates(environment),
        DowInstallSource::Npm,
        &mut visited,
        runner,
    )
    .await
    {
        return Ok(found);
    }

    Err(DowDiscoveryError::NotFound)
}

async fn validate_custom<R: DiscoveryCommandRunner>(
    custom: &Path,
    runner: &R,
) -> Result<DiscoveredDow, DowDiscoveryError> {
    if !custom.is_absolute() {
        return Err(DowDiscoveryError::CustomPathNotAbsolute(
            custom.to_path_buf(),
        ));
    }
    let executable = canonical_executable(custom)
        .ok_or_else(|| DowDiscoveryError::CustomPathUnavailable(custom.to_path_buf()))?;
    validate_candidate(&executable, DowInstallSource::Custom, runner)
        .await
        .map_err(|reason| DowDiscoveryError::InvalidCustomExecutable {
            path: executable,
            reason,
        })
}

async fn try_candidates<R, I>(
    candidates: I,
    source: DowInstallSource,
    visited: &mut HashSet<PathBuf>,
    runner: &R,
) -> Option<DiscoveredDow>
where
    R: DiscoveryCommandRunner,
    I: IntoIterator<Item = PathBuf>,
{
    for candidate in candidates {
        let Some(executable) = canonical_executable(&candidate) else {
            continue;
        };
        if !visited.insert(executable.clone()) {
            continue;
        }
        if let Ok(found) = validate_candidate(&executable, source, runner).await {
            return Some(found);
        }
    }
    None
}

async fn validate_candidate<R: DiscoveryCommandRunner>(
    executable: &Path,
    source: DowInstallSource,
    runner: &R,
) -> Result<DiscoveredDow, String> {
    let output = runner
        .run(executable, &["--version"], COMMAND_TIMEOUT)
        .await
        .map_err(|error| error.to_string())?;
    if !output.success {
        return Err(format!(
            "dow --version exited with {:?}: {}",
            output.code,
            output.stderr.trim()
        ));
    }
    let mut words = output.stdout.split_whitespace();
    if words.next() != Some("dow") {
        return Err("version output must start with `dow`".to_owned());
    }
    let version = words
        .next()
        .ok_or_else(|| "version output is missing semver".to_owned())?
        .parse::<Version>()
        .map_err(|error| format!("invalid semantic version: {error}"))?;
    if words.next().is_some() {
        return Err("version output contains unexpected fields".to_owned());
    }
    Ok(DiscoveredDow {
        executable: executable.to_path_buf(),
        version,
        source,
    })
}

fn helper_executables(environment: &DiscoveryEnvironment, name: &str) -> Vec<PathBuf> {
    let mut helpers = path_executables(&environment.path, executable_names(name));
    helpers.extend(
        environment
            .homebrew_bin_dirs
            .iter()
            .map(|directory| directory.join(executable_name(name))),
    );
    helpers
        .into_iter()
        .filter_map(|path| canonical_executable(&path))
        .collect()
}

fn path_executables(path: &OsStr, names: Vec<OsString>) -> Vec<PathBuf> {
    std::env::split_paths(path)
        .flat_map(|directory| names.iter().map(move |name| directory.join(name)))
        .collect()
}

fn executable_names(name: &str) -> Vec<OsString> {
    #[cfg(windows)]
    {
        vec![
            OsString::from(format!("{name}.exe")),
            OsString::from(format!("{name}.cmd")),
            OsString::from(name),
        ]
    }
    #[cfg(not(windows))]
    {
        vec![OsString::from(name)]
    }
}

fn executable_name(name: &str) -> OsString {
    executable_names(name)
        .into_iter()
        .next()
        .expect("executable names are never empty")
}

fn canonical_executable(path: &Path) -> Option<PathBuf> {
    if !path.is_file() {
        return None;
    }
    std::fs::canonicalize(path).ok()
}

fn single_path_line(stdout: &str) -> Option<PathBuf> {
    let mut lines = stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty());
    let path = PathBuf::from(lines.next()?);
    if lines.next().is_some() || !path.is_absolute() {
        return None;
    }
    Some(path)
}

fn npm_prefix_candidates(prefix: &Path) -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        vec![
            prefix.join("dow.cmd"),
            prefix.join("dow.exe"),
            prefix.join("bin").join("dow.cmd"),
        ]
    }
    #[cfg(not(windows))]
    {
        vec![prefix.join("bin").join("dow")]
    }
}

fn standard_npm_candidates(environment: &DiscoveryEnvironment) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(prefix) = environment.npm_config_prefix.as_deref() {
        candidates.extend(npm_prefix_candidates(prefix));
    }
    #[cfg(windows)]
    if let Some(app_data) = environment.app_data.as_deref() {
        candidates.push(app_data.join("npm").join("dow.cmd"));
        candidates.push(app_data.join("npm").join("dow.exe"));
    }
    #[cfg(not(windows))]
    if let Some(home) = environment.home_dir.as_deref() {
        candidates.push(home.join(".npm-global/bin/dow"));
        candidates.push(home.join(".local/bin/dow"));
    }
    candidates
}
