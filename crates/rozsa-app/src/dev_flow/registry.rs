// FrameworkTree
// registry.rs
// ├── struct DevFlowProjectKey
// ├── enum DevFlowRevisionKey
// ├── enum ProjectResolutionError
// ├── enum DevFlowAvailability
// ├── trait ProjectCommandRunner
// ├── struct SystemProjectCommandRunner
// ├── impl SystemProjectCommandRunner
// ├── run()
// ├── struct DevFlowServiceHandle
// ├── impl DevFlowServiceHandle
// ├── new()
// ├── id()
// ├── snapshot()
// ├── trait DashboardServiceFactory
// ├── struct SessionDevFlowState
// ├── struct SessionBinding
// ├── struct RegistryState
// ├── struct DevFlowRegistry
// ├── impl DevFlowRegistry
// ├── new()
// ├── probe_interval()
// ├── associate_session()
// ├── probe_selected()
// ├── rescan_after_successful_bash()
// ├── session_state()
// ├── service_count()
// ├── resolve_project_with()
// ├── probe_project()
// ├── validate_branch()
// ├── find_non_git_status()
// └── is_readable_file()

//! Project identity, initialization probing, and shared dev-flow services.

use std::collections::HashMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;
use thiserror::Error;
use tokio::process::Command;
use tokio::sync::{Mutex, RwLock};
use tokio::time::timeout;

use super::dashboard::DevFlowSnapshot;
use super::discovery::{CommandExecutionError, CommandOutput};

const PROJECT_COMMAND_TIMEOUT: Duration = Duration::from_secs(2);
const PROJECT_PROBE_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct DevFlowProjectKey {
    pub root: PathBuf,
    pub revision: DevFlowRevisionKey,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum DevFlowRevisionKey {
    NamedBranch(String),
    UnbornBranch(String),
    DetachedCommit(String),
    NonGit,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ProjectResolutionError {
    #[error("project path is unavailable: {path}: {reason}")]
    UnavailablePath { path: PathBuf, reason: String },
    #[error("git returned an invalid project root: {0}")]
    InvalidGitRoot(String),
    #[error("git returned an invalid branch name: {0}")]
    InvalidBranch(String),
    #[error("git repository has neither a symbolic branch nor a commit")]
    UnknownGitRevision,
    #[error("project identity command failed: {0}")]
    Command(String),
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum DevFlowAvailability {
    #[error("project has not been initialized for dev-flow")]
    ProjectNotInitialized,
    #[error("non-Git project has multiple dev-flow STATUS.yaml files")]
    AmbiguousNonGitProject,
    #[error("detached Git revisions are not supported by the current dow dashboard")]
    UnsupportedRevision,
    #[error("dev-flow STATUS.yaml is not readable: {0}")]
    StatusUnreadable(PathBuf),
    #[error("dow status validation failed: {0}")]
    StatusProbeFailed(String),
    #[error("dashboard startup failed: {0}")]
    DashboardStartFailed(String),
    #[error("dev-flow is ready")]
    Ready,
}

#[async_trait]
pub trait ProjectCommandRunner: Send + Sync {
    async fn run(
        &self,
        cwd: &Path,
        executable: &Path,
        args: &[&str],
        deadline: Duration,
    ) -> Result<CommandOutput, CommandExecutionError>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemProjectCommandRunner;

#[async_trait]
impl ProjectCommandRunner for SystemProjectCommandRunner {
    async fn run(
        &self,
        cwd: &Path,
        executable: &Path,
        args: &[&str],
        deadline: Duration,
    ) -> Result<CommandOutput, CommandExecutionError> {
        let mut command = Command::new(executable);
        command.current_dir(cwd).args(args).kill_on_drop(true);
        let output = timeout(deadline, command.output())
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

#[derive(Clone)]
pub struct DevFlowServiceHandle {
    id: u64,
    snapshot: Arc<RwLock<Option<DevFlowSnapshot>>>,
}

impl DevFlowServiceHandle {
    pub fn new(id: u64, snapshot: Arc<RwLock<Option<DevFlowSnapshot>>>) -> Self {
        Self { id, snapshot }
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn snapshot(&self) -> Arc<RwLock<Option<DevFlowSnapshot>>> {
        self.snapshot.clone()
    }
}

#[async_trait]
pub trait DashboardServiceFactory: Send + Sync {
    /// Start and validate a service, returning only after its first valid snapshot.
    async fn start(&self, project: &DevFlowProjectKey) -> Result<DevFlowServiceHandle, String>;
}

#[derive(Clone)]
pub struct SessionDevFlowState {
    pub project: DevFlowProjectKey,
    pub availability: DevFlowAvailability,
    pub service: Option<DevFlowServiceHandle>,
}

struct SessionBinding {
    cwd: PathBuf,
    state: SessionDevFlowState,
}

#[derive(Default)]
struct RegistryState {
    sessions: HashMap<String, SessionBinding>,
    services: HashMap<DevFlowProjectKey, DevFlowServiceHandle>,
}

pub struct DevFlowRegistry {
    dow_executable: PathBuf,
    runner: Arc<dyn ProjectCommandRunner>,
    factory: Arc<dyn DashboardServiceFactory>,
    state: Mutex<RegistryState>,
}

impl DevFlowRegistry {
    pub fn new(
        dow_executable: PathBuf,
        runner: Arc<dyn ProjectCommandRunner>,
        factory: Arc<dyn DashboardServiceFactory>,
    ) -> Self {
        Self {
            dow_executable,
            runner,
            factory,
            state: Mutex::new(RegistryState::default()),
        }
    }

    pub fn probe_interval() -> Duration {
        PROJECT_PROBE_INTERVAL
    }

    pub async fn associate_session(
        &self,
        session_id: impl Into<String>,
        cwd: PathBuf,
    ) -> Result<SessionDevFlowState, ProjectResolutionError> {
        let session_id = session_id.into();
        let project = resolve_project_with(&cwd, self.runner.as_ref()).await?;
        let availability =
            probe_project(&project, &self.dow_executable, self.runner.as_ref()).await;

        let mut registry = self.state.lock().await;
        let service = if availability == DevFlowAvailability::Ready {
            if let Some(service) = registry.services.get(&project) {
                Some(service.clone())
            } else {
                match self.factory.start(&project).await {
                    Ok(service) => {
                        registry.services.insert(project.clone(), service.clone());
                        Some(service)
                    }
                    Err(error) => {
                        let state = SessionDevFlowState {
                            project: project.clone(),
                            availability: DevFlowAvailability::DashboardStartFailed(error),
                            service: None,
                        };
                        registry.sessions.insert(
                            session_id,
                            SessionBinding {
                                cwd,
                                state: state.clone(),
                            },
                        );
                        return Ok(state);
                    }
                }
            }
        } else {
            None
        };
        let state = SessionDevFlowState {
            project,
            availability,
            service,
        };
        registry.sessions.insert(
            session_id,
            SessionBinding {
                cwd,
                state: state.clone(),
            },
        );
        Ok(state)
    }

    pub async fn probe_selected(
        &self,
        session_id: &str,
    ) -> Result<Option<SessionDevFlowState>, ProjectResolutionError> {
        let cwd = {
            let registry = self.state.lock().await;
            registry
                .sessions
                .get(session_id)
                .map(|binding| binding.cwd.clone())
        };
        match cwd {
            Some(cwd) => self
                .associate_session(session_id.to_owned(), cwd)
                .await
                .map(Some),
            None => Ok(None),
        }
    }

    pub async fn rescan_after_successful_bash(
        &self,
        cwd: &Path,
    ) -> Result<Vec<(String, SessionDevFlowState)>, ProjectResolutionError> {
        let changed_project = resolve_project_with(cwd, self.runner.as_ref()).await?;
        let sessions = {
            let registry = self.state.lock().await;
            registry
                .sessions
                .iter()
                .filter(|(_, binding)| binding.state.project.root == changed_project.root)
                .map(|(id, binding)| (id.clone(), binding.cwd.clone()))
                .collect::<Vec<_>>()
        };

        let mut rescanned = Vec::with_capacity(sessions.len());
        for (id, session_cwd) in sessions {
            let state = self.associate_session(id.clone(), session_cwd).await?;
            rescanned.push((id, state));
        }
        Ok(rescanned)
    }

    pub async fn session_state(&self, session_id: &str) -> Option<SessionDevFlowState> {
        self.state
            .lock()
            .await
            .sessions
            .get(session_id)
            .map(|binding| binding.state.clone())
    }

    pub async fn service_count(&self) -> usize {
        self.state.lock().await.services.len()
    }
}

pub async fn resolve_project_with(
    cwd: &Path,
    runner: &dyn ProjectCommandRunner,
) -> Result<DevFlowProjectKey, ProjectResolutionError> {
    let canonical_cwd =
        fs::canonicalize(cwd).map_err(|error| ProjectResolutionError::UnavailablePath {
            path: cwd.to_path_buf(),
            reason: error.to_string(),
        })?;
    let git = Path::new("git");
    let root_output = runner
        .run(
            &canonical_cwd,
            git,
            &["rev-parse", "--show-toplevel"],
            PROJECT_COMMAND_TIMEOUT,
        )
        .await
        .map_err(|error| ProjectResolutionError::Command(error.to_string()))?;
    if !root_output.success {
        return Ok(DevFlowProjectKey {
            root: canonical_cwd,
            revision: DevFlowRevisionKey::NonGit,
        });
    }

    let root_text = root_output.stdout.trim();
    if root_text.is_empty() {
        return Err(ProjectResolutionError::InvalidGitRoot(root_output.stdout));
    }
    let root = fs::canonicalize(root_text)
        .map_err(|_| ProjectResolutionError::InvalidGitRoot(root_text.to_owned()))?;
    let symbolic = runner
        .run(
            &root,
            git,
            &["symbolic-ref", "--quiet", "--short", "HEAD"],
            PROJECT_COMMAND_TIMEOUT,
        )
        .await
        .map_err(|error| ProjectResolutionError::Command(error.to_string()))?;
    let head = runner
        .run(
            &root,
            git,
            &["rev-parse", "--verify", "HEAD"],
            PROJECT_COMMAND_TIMEOUT,
        )
        .await
        .map_err(|error| ProjectResolutionError::Command(error.to_string()))?;

    if symbolic.success {
        let branch = symbolic.stdout.trim().to_owned();
        validate_branch(&branch)?;
        let revision = if head.success && !head.stdout.trim().is_empty() {
            DevFlowRevisionKey::NamedBranch(branch)
        } else {
            DevFlowRevisionKey::UnbornBranch(branch)
        };
        return Ok(DevFlowProjectKey { root, revision });
    }

    if head.success {
        let oid = head.stdout.trim();
        if !oid.is_empty() {
            return Ok(DevFlowProjectKey {
                root,
                revision: DevFlowRevisionKey::DetachedCommit(oid.to_owned()),
            });
        }
    }

    Err(ProjectResolutionError::UnknownGitRevision)
}

pub async fn probe_project(
    project: &DevFlowProjectKey,
    dow_executable: &Path,
    runner: &dyn ProjectCommandRunner,
) -> DevFlowAvailability {
    let status = match &project.revision {
        DevFlowRevisionKey::NamedBranch(branch) | DevFlowRevisionKey::UnbornBranch(branch) => {
            project
                .root
                .join(".dev-doc")
                .join(branch)
                .join("STATUS.yaml")
        }
        DevFlowRevisionKey::DetachedCommit(_) => {
            return DevFlowAvailability::UnsupportedRevision;
        }
        DevFlowRevisionKey::NonGit => match find_non_git_status(&project.root) {
            Ok(status) => status,
            Err(availability) => return availability,
        },
    };

    if !is_readable_file(&status) {
        return if status.exists() {
            DevFlowAvailability::StatusUnreadable(status)
        } else {
            DevFlowAvailability::ProjectNotInitialized
        };
    }

    let output = runner
        .run(
            &project.root,
            dow_executable,
            &["status"],
            PROJECT_COMMAND_TIMEOUT,
        )
        .await;
    let output = match output {
        Ok(output) if output.success => output,
        Ok(output) => {
            return DevFlowAvailability::StatusProbeFailed(format!(
                "exit {:?}: {}",
                output.code,
                output.stderr.trim()
            ));
        }
        Err(error) => return DevFlowAvailability::StatusProbeFailed(error.to_string()),
    };
    match serde_json::from_str::<Value>(&output.stdout) {
        Ok(Value::Object(_)) => DevFlowAvailability::Ready,
        Ok(_) => {
            DevFlowAvailability::StatusProbeFailed("dow status returned non-object JSON".to_owned())
        }
        Err(error) => DevFlowAvailability::StatusProbeFailed(format!(
            "dow status returned invalid JSON: {error}"
        )),
    }
}

fn validate_branch(branch: &str) -> Result<(), ProjectResolutionError> {
    let path = Path::new(branch);
    if branch.is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(ProjectResolutionError::InvalidBranch(branch.to_owned()));
    }
    Ok(())
}

fn find_non_git_status(root: &Path) -> Result<PathBuf, DevFlowAvailability> {
    let dev_doc = root.join(".dev-doc");
    if !dev_doc.is_dir() {
        return Err(DevFlowAvailability::ProjectNotInitialized);
    }
    let mut pending = vec![dev_doc];
    let mut readable = Vec::new();
    while let Some(directory) = pending.pop() {
        let Ok(entries) = fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.file_name().is_some_and(|name| name == "STATUS.yaml")
                && is_readable_file(&path)
            {
                readable.push(path);
                if readable.len() > 1 {
                    return Err(DevFlowAvailability::AmbiguousNonGitProject);
                }
            }
        }
    }
    readable
        .pop()
        .ok_or(DevFlowAvailability::ProjectNotInitialized)
}

fn is_readable_file(path: &Path) -> bool {
    fs::File::open(path).is_ok_and(|file| file.metadata().is_ok_and(|metadata| metadata.is_file()))
}
