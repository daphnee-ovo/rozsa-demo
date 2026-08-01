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
// ├── with_child()
// ├── with_base_url()
// ├── id()
// ├── snapshot()
// ├── base_url()
// ├── enum ServiceShutdownOutcome
// ├── trait DashboardServiceControl
// ├── trait MemoryReader
// ├── child_rss_bytes_batch()
// ├── struct SystemMemoryReader
// ├── impl SystemMemoryReader
// ├── total_physical_memory_bytes()
// ├── child_rss_bytes()
// ├── child_rss_bytes_batch()
// ├── trait DashboardServiceFactory
// ├── struct SessionDevFlowState
// ├── struct SessionBinding
// ├── enum ServiceState
// ├── struct ServiceEntry
// ├── struct ServiceUsageInput
// ├── struct SweepReport
// ├── struct ShutdownAllReport
// ├── struct RegistryDiagnostics
// ├── struct RegistryState
// ├── struct DevFlowRegistry
// ├── impl DevFlowRegistry
// ├── new()
// ├── with_memory_reader()
// ├── probe_interval()
// ├── sweep_interval()
// ├── idle_reclamation_window()
// ├── no_client_shutdown_window()
// ├── memory_budget()
// ├── associate_session()
// ├── session_active()
// ├── session_finished()
// ├── session_closed()
// ├── set_current_project()
// ├── shutdown_all()
// ├── sweep()
// ├── diagnostics()
// ├── project_usage_bytes()
// ├── probe_selected()
// ├── rescan_after_successful_bash()
// ├── session_state()
// ├── last_stop_at()
// ├── service_count()
// ├── impl RegistryState
// ├── next_seq()
// ├── replace_session_binding()
// ├── record_stop()
// ├── refresh_stop_times()
// ├── is_current_or_active()
// ├── usage_inputs()
// ├── measure_usage()
// ├── estimate_snapshot_bytes()
// ├── estimate_task_bytes()
// ├── estimate_issue_bytes()
// ├── opt_text()
// ├── vec_text_bytes()
// ├── resolve_project_with()
// ├── probe_project()
// ├── validate_branch()
// ├── find_non_git_status()
// └── is_readable_file()

//! Project identity, initialization probing, and shared dev-flow services.

use std::cmp::max;
use std::collections::HashMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use serde_json::Value;
use thiserror::Error;
use tokio::process::Command;
use tokio::sync::{Mutex, RwLock};
use tokio::time::timeout;

use reqwest::Url;

use super::dashboard::{DevFlowIssue, DevFlowSnapshot, DevFlowTask};
use super::discovery::{CommandExecutionError, CommandOutput};

const PROJECT_COMMAND_TIMEOUT: Duration = Duration::from_secs(2);
const PROJECT_PROBE_INTERVAL: Duration = Duration::from_secs(2);

pub const SWEEP_INTERVAL: Duration = Duration::from_secs(60);
pub const IDLE_RECLAMATION_WINDOW: Duration = Duration::from_secs(15 * 60);
pub const NO_CLIENT_SHUTDOWN_WINDOW: Duration = Duration::from_secs(35);
pub const MIN_MEMORY_BUDGET_BYTES: u64 = 256 * 1024 * 1024;
/// Documented approximate fixed per-service registry overhead in bytes.
pub const REGISTRY_FIXED_OVERHEAD_BYTES_PER_SERVICE: u64 = 256 * 1024;
const SNAPSHOT_ITEM_OVERHEAD: u64 = 128;

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
    control: Option<Arc<dyn DashboardServiceControl>>,
    pid: Option<u32>,
    base_url: Option<Url>,
}

impl DevFlowServiceHandle {
    pub fn new(id: u64, snapshot: Arc<RwLock<Option<DevFlowSnapshot>>>) -> Self {
        Self {
            id,
            snapshot,
            control: None,
            pid: None,
            base_url: None,
        }
    }

    pub fn with_child(
        id: u64,
        snapshot: Arc<RwLock<Option<DevFlowSnapshot>>>,
        control: Arc<dyn DashboardServiceControl>,
        pid: Option<u32>,
    ) -> Self {
        Self::with_base_url(id, snapshot, control, pid, None)
    }

    /// Constructs a handle that records the loopback dashboard URL so the GUI
    /// can open it without re-deriving the child's port.
    pub fn with_base_url(
        id: u64,
        snapshot: Arc<RwLock<Option<DevFlowSnapshot>>>,
        control: Arc<dyn DashboardServiceControl>,
        pid: Option<u32>,
        base_url: Option<Url>,
    ) -> Self {
        Self {
            id,
            snapshot,
            control: Some(control),
            pid,
            base_url,
        }
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn snapshot(&self) -> Arc<RwLock<Option<DevFlowSnapshot>>> {
        self.snapshot.clone()
    }

    pub fn base_url(&self) -> Option<&Url> {
        self.base_url.as_ref()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceShutdownOutcome {
    Exited,
    StillRunning,
}

#[async_trait]
pub trait DashboardServiceControl: Send + Sync {
    /// Close owned connections and wait up to `grace` for the child to exit.
    async fn shutdown(&self, grace: Duration) -> ServiceShutdownOutcome;
    /// Probe whether the child (or its dashboard URL) is still alive.
    async fn is_alive(&self) -> bool;
}

pub trait MemoryReader: Send + Sync {
    fn total_physical_memory_bytes(&self) -> Option<u64>;
    fn child_rss_bytes(&self, pid: u32) -> Option<u64>;

    fn child_rss_bytes_batch(&self, pids: &[u32]) -> HashMap<u32, u64> {
        pids.iter()
            .filter_map(|pid| self.child_rss_bytes(*pid).map(|bytes| (*pid, bytes)))
            .collect()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemMemoryReader;

impl MemoryReader for SystemMemoryReader {
    fn total_physical_memory_bytes(&self) -> Option<u64> {
        let mut system = sysinfo::System::new_with_specifics(
            sysinfo::RefreshKind::nothing()
                .with_memory(sysinfo::MemoryRefreshKind::nothing().with_ram()),
        );
        system.refresh_memory();
        Some(system.total_memory())
    }

    fn child_rss_bytes(&self, pid: u32) -> Option<u64> {
        use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
        let mut system = System::new();
        let pid = Pid::from_u32(pid);
        system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[pid]),
            true,
            ProcessRefreshKind::everything(),
        );
        system.process(pid).map(|process| process.memory())
    }

    fn child_rss_bytes_batch(&self, pids: &[u32]) -> HashMap<u32, u64> {
        use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
        let pids = pids.iter().copied().map(Pid::from_u32).collect::<Vec<_>>();
        let mut system = System::new();
        system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&pids),
            true,
            ProcessRefreshKind::everything(),
        );
        pids.into_iter()
            .filter_map(|pid| {
                system
                    .process(pid)
                    .map(|process| (pid.as_u32(), process.memory()))
            })
            .collect()
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
    active: bool,
    last_stop_at: Option<SystemTime>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ServiceState {
    Live,
    Protected,
}

struct ServiceEntry {
    handle: DevFlowServiceHandle,
    last_used_seq: u64,
    last_stop_at: Option<SystemTime>,
    state: ServiceState,
}

struct ServiceUsageInput {
    state: ServiceState,
    pid: Option<u32>,
    snapshot: Arc<RwLock<Option<DevFlowSnapshot>>>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SweepReport {
    pub reclaimed: Vec<DevFlowProjectKey>,
    pub protected: Vec<DevFlowProjectKey>,
    pub usage_bytes: u64,
    pub budget_bytes: Option<u64>,
    pub over_budget: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ShutdownAllReport {
    pub terminated: Vec<DevFlowProjectKey>,
    pub still_running: Vec<DevFlowProjectKey>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RegistryDiagnostics {
    pub live_services: usize,
    pub protected_services: usize,
    pub active_sessions: usize,
    pub usage_bytes: u64,
    pub budget_bytes: Option<u64>,
    pub over_budget: bool,
    pub protected_usage_bytes: u64,
}

#[derive(Default)]
struct RegistryState {
    sessions: HashMap<String, SessionBinding>,
    services: HashMap<DevFlowProjectKey, ServiceEntry>,
    current_project: Option<DevFlowProjectKey>,
    last_used_seq: u64,
}

pub struct DevFlowRegistry {
    dow_executable: PathBuf,
    runner: Arc<dyn ProjectCommandRunner>,
    factory: Arc<dyn DashboardServiceFactory>,
    memory: Arc<dyn MemoryReader>,
    state: Mutex<RegistryState>,
    sweep_lock: Mutex<()>,
}

impl DevFlowRegistry {
    pub fn new(
        dow_executable: PathBuf,
        runner: Arc<dyn ProjectCommandRunner>,
        factory: Arc<dyn DashboardServiceFactory>,
    ) -> Self {
        Self::with_memory_reader(
            dow_executable,
            runner,
            factory,
            Arc::new(SystemMemoryReader),
        )
    }

    pub fn with_memory_reader(
        dow_executable: PathBuf,
        runner: Arc<dyn ProjectCommandRunner>,
        factory: Arc<dyn DashboardServiceFactory>,
        memory: Arc<dyn MemoryReader>,
    ) -> Self {
        Self {
            dow_executable,
            runner,
            factory,
            memory,
            state: Mutex::new(RegistryState::default()),
            sweep_lock: Mutex::new(()),
        }
    }

    pub fn probe_interval() -> Duration {
        PROJECT_PROBE_INTERVAL
    }

    pub fn sweep_interval() -> Duration {
        SWEEP_INTERVAL
    }

    pub fn idle_reclamation_window() -> Duration {
        IDLE_RECLAMATION_WINDOW
    }

    pub fn no_client_shutdown_window() -> Duration {
        NO_CLIENT_SHUTDOWN_WINDOW
    }

    pub fn memory_budget(total_physical_bytes: u64) -> u64 {
        max(total_physical_bytes * 5 / 100, MIN_MEMORY_BUDGET_BYTES)
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
        let _lifecycle_guard = self.sweep_lock.lock().await;
        let provisional = SessionDevFlowState {
            project: project.clone(),
            availability: availability.clone(),
            service: None,
        };
        {
            let mut registry = self.state.lock().await;
            registry.replace_session_binding(session_id.clone(), cwd.clone(), provisional.clone());
        }

        let mut insert_service = false;
        let mut start_error = None;
        let service = if availability == DevFlowAvailability::Ready {
            let existing = {
                let mut registry = self.state.lock().await;
                let seq = registry.next_seq();
                registry.services.get_mut(&project).map(|entry| {
                    entry.last_used_seq = seq;
                    (
                        entry.handle.clone(),
                        entry.state,
                        entry.handle.control.clone(),
                    )
                })
            };
            match existing {
                Some((handle, ServiceState::Live, _)) => Some(handle),
                Some((handle, ServiceState::Protected, control)) => {
                    let alive = match control {
                        Some(control) => control.is_alive().await,
                        None => false,
                    };
                    if alive {
                        if let Some(entry) = self.state.lock().await.services.get_mut(&project) {
                            entry.state = ServiceState::Live;
                        }
                        Some(handle)
                    } else {
                        self.state.lock().await.services.remove(&project);
                        insert_service = true;
                        match self.factory.start(&project).await {
                            Ok(service) => Some(service),
                            Err(error) => {
                                start_error = Some(error);
                                None
                            }
                        }
                    }
                }
                None => {
                    insert_service = true;
                    match self.factory.start(&project).await {
                        Ok(service) => Some(service),
                        Err(error) => {
                            start_error = Some(error);
                            None
                        }
                    }
                }
            }
        } else {
            None
        };

        let availability = if let Some(error) = start_error {
            DevFlowAvailability::DashboardStartFailed(error)
        } else {
            availability
        };
        let state = SessionDevFlowState {
            project: project.clone(),
            availability,
            service: service.clone(),
        };
        let mut registry = self.state.lock().await;
        let session_is_current = registry
            .sessions
            .get(&session_id)
            .is_some_and(|binding| binding.cwd == cwd && binding.state.project == project);
        if session_is_current {
            if insert_service && let Some(service) = service {
                let seq = registry.next_seq();
                registry.services.insert(
                    project,
                    ServiceEntry {
                        last_used_seq: seq,
                        last_stop_at: None,
                        state: ServiceState::Live,
                        handle: service,
                    },
                );
            }
            if let Some(binding) = registry.sessions.get_mut(&session_id) {
                binding.state = state.clone();
            }
        }
        Ok(state)
    }

    pub async fn session_active(&self, session_id: &str) {
        let mut registry = self.state.lock().await;
        if let Some(binding) = registry.sessions.get_mut(session_id) {
            binding.active = true;
            binding.last_stop_at = None;
        }
    }

    pub async fn session_finished(&self, session_id: &str, stopped_at: SystemTime) {
        let mut registry = self.state.lock().await;
        let project = {
            let Some(binding) = registry.sessions.get_mut(session_id) else {
                return;
            };
            binding.active = false;
            binding.last_stop_at = Some(stopped_at);
            binding.state.project.clone()
        };
        registry.record_stop(&project, stopped_at);
    }

    pub async fn session_closed(&self, session_id: &str, stopped_at: SystemTime) {
        let mut registry = self.state.lock().await;
        let project = {
            let Some(binding) = registry.sessions.remove(session_id) else {
                return;
            };
            binding.state.project.clone()
        };
        registry.record_stop(&project, stopped_at);
    }

    pub async fn set_current_project(&self, project: Option<DevFlowProjectKey>) {
        self.state.lock().await.current_project = project;
    }

    /// Terminate and reap every currently live Rózsa-owned child. Children
    /// already classified as `PossibleExternalClient` are never touched.
    pub async fn shutdown_all(&self) -> ShutdownAllReport {
        let _sweep_guard = self.sweep_lock.lock().await;
        let mut report = ShutdownAllReport::default();
        loop {
            let next = {
                let registry = self.state.lock().await;
                registry
                    .services
                    .iter()
                    .filter(|(_, entry)| entry.state != ServiceState::Protected)
                    .map(|(project, _)| project.clone())
                    .next()
            };
            let Some(project) = next else {
                break;
            };

            let outcome = {
                let registry = self.state.lock().await;
                let entry = registry
                    .services
                    .get(&project)
                    .expect("shutdown candidate exists");
                entry.handle.control.clone()
            };
            let outcome = match outcome {
                Some(control) => control.shutdown(NO_CLIENT_SHUTDOWN_WINDOW).await,
                None => ServiceShutdownOutcome::Exited,
            };

            let mut registry = self.state.lock().await;
            let Some(entry) = registry.services.get_mut(&project) else {
                continue;
            };
            match outcome {
                ServiceShutdownOutcome::Exited => {
                    registry.services.remove(&project);
                    report.terminated.push(project);
                }
                ServiceShutdownOutcome::StillRunning => {
                    entry.state = ServiceState::Protected;
                    report.still_running.push(project);
                }
            }
        }
        report
    }

    pub async fn sweep(&self, now: SystemTime) -> SweepReport {
        let _sweep_guard = self.sweep_lock.lock().await;
        let mut report = SweepReport::default();
        let idle_cutoff = now.checked_sub(IDLE_RECLAMATION_WINDOW);
        let previously_protected = {
            let registry = self.state.lock().await;
            registry
                .services
                .iter()
                .filter(|(_, entry)| entry.state == ServiceState::Protected)
                .map(|(project, _)| project.clone())
                .collect::<Vec<_>>()
        };

        loop {
            let usage_inputs = {
                let mut registry = self.state.lock().await;
                registry.refresh_stop_times();
                registry.usage_inputs()
            };
            let (usage, budget, _) = measure_usage(usage_inputs, self.memory.as_ref()).await;
            report.usage_bytes = usage;
            report.budget_bytes = budget;
            report.over_budget = budget.is_some_and(|budget| usage > budget);
            let next = {
                let registry = self.state.lock().await;
                let mut candidates = registry
                    .services
                    .iter()
                    .filter(|(key, entry)| {
                        entry.state != ServiceState::Protected
                            && !registry.is_current_or_active(key)
                    })
                    .map(|(key, entry)| {
                        let time_eligible = idle_cutoff.is_some_and(|cutoff| {
                            entry
                                .last_stop_at
                                .is_some_and(|stopped_at| stopped_at <= cutoff)
                        });
                        (key.clone(), time_eligible)
                    })
                    .collect::<Vec<_>>();
                candidates.sort_by(|left, right| {
                    let left_seq = registry.services[&left.0].last_used_seq;
                    let right_seq = registry.services[&right.0].last_used_seq;
                    right.1.cmp(&left.1).then(left_seq.cmp(&right_seq))
                });
                candidates
                    .into_iter()
                    .find(|(_, time_eligible)| *time_eligible || report.over_budget)
                    .map(|(key, _)| key)
            };
            let Some(project) = next else {
                break;
            };

            let outcome = {
                let registry = self.state.lock().await;
                let entry = registry
                    .services
                    .get(&project)
                    .expect("sweep candidate exists");
                entry.handle.control.clone()
            };
            let outcome = match outcome {
                Some(control) => control.shutdown(NO_CLIENT_SHUTDOWN_WINDOW).await,
                None => ServiceShutdownOutcome::Exited,
            };

            let mut registry = self.state.lock().await;
            if !registry.services.contains_key(&project) {
                continue;
            }
            match outcome {
                ServiceShutdownOutcome::Exited => {
                    let required_again = registry.is_current_or_active(&project);
                    registry.services.remove(&project);
                    drop(registry);
                    if required_again {
                        match self.factory.start(&project).await {
                            Ok(service) => {
                                let mut registry = self.state.lock().await;
                                let seq = registry.next_seq();
                                registry.services.insert(
                                    project.clone(),
                                    ServiceEntry {
                                        handle: service.clone(),
                                        last_used_seq: seq,
                                        last_stop_at: None,
                                        state: ServiceState::Live,
                                    },
                                );
                                for binding in registry
                                    .sessions
                                    .values_mut()
                                    .filter(|binding| binding.state.project == project)
                                {
                                    binding.state.availability = DevFlowAvailability::Ready;
                                    binding.state.service = Some(service.clone());
                                }
                            }
                            Err(error) => {
                                let mut registry = self.state.lock().await;
                                for binding in registry
                                    .sessions
                                    .values_mut()
                                    .filter(|binding| binding.state.project == project)
                                {
                                    binding.state.availability =
                                        DevFlowAvailability::DashboardStartFailed(error.clone());
                                    binding.state.service = None;
                                }
                            }
                        }
                    } else {
                        report.reclaimed.push(project);
                    }
                }
                ServiceShutdownOutcome::StillRunning => {
                    registry
                        .services
                        .get_mut(&project)
                        .expect("sweep service exists")
                        .state = ServiceState::Protected;
                    report.protected.push(project);
                }
            }
        }

        for project in previously_protected {
            let outcome = {
                let registry = self.state.lock().await;
                let entry = registry
                    .services
                    .get(&project)
                    .expect("protected service exists");
                entry.handle.control.clone()
            };
            let outcome = match outcome {
                Some(control) => control.shutdown(Duration::ZERO).await,
                None => ServiceShutdownOutcome::Exited,
            };
            let mut registry = self.state.lock().await;
            if registry
                .services
                .get(&project)
                .is_some_and(|entry| entry.state == ServiceState::Protected)
                && outcome == ServiceShutdownOutcome::Exited
            {
                registry.services.remove(&project);
                report.reclaimed.push(project);
            }
        }

        let usage_inputs = {
            let registry = self.state.lock().await;
            registry.usage_inputs()
        };
        let (usage, budget, _) = measure_usage(usage_inputs, self.memory.as_ref()).await;
        report.usage_bytes = usage;
        report.budget_bytes = budget;
        report.over_budget = budget.is_some_and(|budget| usage > budget);
        report
    }

    pub async fn diagnostics(&self) -> RegistryDiagnostics {
        let (live_services, protected_services, active_sessions, usage_inputs) = {
            let mut registry = self.state.lock().await;
            registry.refresh_stop_times();
            (
                registry
                    .services
                    .values()
                    .filter(|entry| entry.state == ServiceState::Live)
                    .count(),
                registry
                    .services
                    .values()
                    .filter(|entry| entry.state == ServiceState::Protected)
                    .count(),
                registry
                    .sessions
                    .values()
                    .filter(|binding| binding.active)
                    .count(),
                registry.usage_inputs(),
            )
        };
        let (usage, budget, protected_usage) =
            measure_usage(usage_inputs, self.memory.as_ref()).await;
        RegistryDiagnostics {
            live_services,
            protected_services,
            active_sessions,
            usage_bytes: usage,
            budget_bytes: budget,
            over_budget: budget.is_some_and(|budget| usage > budget),
            protected_usage_bytes: protected_usage,
        }
    }

    /// Memory currently owned by one project dashboard service, including
    /// child RSS and the registry's project-local cache estimate.
    pub async fn project_usage_bytes(&self, project: &DevFlowProjectKey) -> Option<u64> {
        let input = {
            let registry = self.state.lock().await;
            registry
                .services
                .get(project)
                .map(|entry| ServiceUsageInput {
                    state: entry.state,
                    pid: entry.handle.pid,
                    snapshot: entry.handle.snapshot.clone(),
                })
        }?;
        let (usage, _, _) = measure_usage(vec![input], self.memory.as_ref()).await;
        Some(usage)
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

    pub async fn last_stop_at(&self, session_id: &str) -> Option<SystemTime> {
        self.state
            .lock()
            .await
            .sessions
            .get(session_id)
            .and_then(|binding| binding.last_stop_at)
    }

    pub async fn service_count(&self) -> usize {
        self.state.lock().await.services.len()
    }
}

impl RegistryState {
    fn next_seq(&mut self) -> u64 {
        self.last_used_seq += 1;
        self.last_used_seq
    }

    fn replace_session_binding(
        &mut self,
        session_id: String,
        cwd: PathBuf,
        state: SessionDevFlowState,
    ) {
        let (active, last_stop_at) = self
            .sessions
            .get(&session_id)
            .map(|binding| (binding.active, binding.last_stop_at))
            .unwrap_or((false, None));
        self.sessions.insert(
            session_id,
            SessionBinding {
                cwd,
                state,
                active,
                last_stop_at,
            },
        );
    }

    fn record_stop(&mut self, project: &DevFlowProjectKey, stopped_at: SystemTime) {
        if let Some(entry) = self.services.get_mut(project) {
            entry.last_stop_at = Some(
                entry
                    .last_stop_at
                    .map_or(stopped_at, |current| current.max(stopped_at)),
            );
        }
    }

    fn refresh_stop_times(&mut self) {
        let stops = self
            .sessions
            .values()
            .filter_map(|binding| {
                binding
                    .last_stop_at
                    .map(|stopped_at| (binding.state.project.clone(), stopped_at))
            })
            .collect::<Vec<_>>();
        for (project, stopped_at) in stops {
            self.record_stop(&project, stopped_at);
        }
    }

    fn is_current_or_active(&self, project: &DevFlowProjectKey) -> bool {
        if self.current_project.as_ref() == Some(project) {
            return true;
        }
        self.sessions
            .values()
            .any(|binding| binding.active && binding.state.project == *project)
    }

    fn usage_inputs(&self) -> Vec<ServiceUsageInput> {
        self.services
            .values()
            .map(|entry| ServiceUsageInput {
                state: entry.state,
                pid: entry.handle.pid,
                snapshot: entry.handle.snapshot.clone(),
            })
            .collect()
    }
}

async fn measure_usage(
    entries: Vec<ServiceUsageInput>,
    memory: &dyn MemoryReader,
) -> (u64, Option<u64>, u64) {
    let pids = entries
        .iter()
        .filter_map(|entry| entry.pid)
        .collect::<Vec<_>>();
    let rss = memory.child_rss_bytes_batch(&pids);
    let mut usage = 0u64;
    let mut protected_usage = 0u64;
    for entry in entries {
        let mut bytes = REGISTRY_FIXED_OVERHEAD_BYTES_PER_SERVICE;
        if let Some(pid) = entry.pid {
            bytes += rss.get(&pid).copied().unwrap_or(0);
        }
        if let Some(snapshot) = entry.snapshot.read().await.as_ref() {
            bytes += estimate_snapshot_bytes(snapshot);
        }
        usage += bytes;
        if entry.state == ServiceState::Protected {
            protected_usage += bytes;
        }
    }
    let budget = memory
        .total_physical_memory_bytes()
        .map(DevFlowRegistry::memory_budget);
    (usage, budget, protected_usage)
}

fn estimate_snapshot_bytes(snapshot: &DevFlowSnapshot) -> u64 {
    let mut bytes = 512u64;
    for text in [
        snapshot.project.name.as_deref(),
        snapshot.project.phase.as_deref(),
        snapshot.project.mode.as_deref(),
        snapshot.project.version.as_deref(),
        snapshot.project.goals_minor.as_deref(),
        snapshot.project.updated.as_deref(),
    ] {
        bytes += text.map_or(0, |text| text.len() as u64);
    }
    bytes += snapshot.tasks.iter().map(estimate_task_bytes).sum::<u64>();
    bytes += snapshot
        .issues
        .iter()
        .map(estimate_issue_bytes)
        .sum::<u64>();
    bytes
}

fn estimate_task_bytes(task: &DevFlowTask) -> u64 {
    SNAPSHOT_ITEM_OVERHEAD
        + task.id.len() as u64
        + task.title.len() as u64
        + opt_text(task.priority.as_deref())
        + opt_text(task.complexity.as_deref())
        + opt_text(task.task_type.as_deref())
        + opt_text(task.refs.as_deref())
        + vec_text_bytes(&task.depends_on)
        + vec_text_bytes(&task.done_when)
        + vec_text_bytes(&task.files_create)
        + vec_text_bytes(&task.files_modify)
        + vec_text_bytes(&task.files_test)
}

fn estimate_issue_bytes(issue: &DevFlowIssue) -> u64 {
    SNAPSHOT_ITEM_OVERHEAD
        + issue.id.len() as u64
        + issue.title.len() as u64
        + opt_text(issue.severity.as_deref())
        + opt_text(issue.description.as_deref())
        + vec_text_bytes(&issue.files_create)
        + vec_text_bytes(&issue.files_modify)
}

fn opt_text(text: Option<&str>) -> u64 {
    text.map_or(0, |text| text.len() as u64)
}

fn vec_text_bytes(values: &[String]) -> u64 {
    values
        .iter()
        .map(|value| value.len() as u64 + 8)
        .sum::<u64>()
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
