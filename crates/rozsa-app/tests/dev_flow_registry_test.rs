// FrameworkTree
// dev_flow_registry_test.rs
// ├── enum FakeRevision
// ├── struct FakeStatus
// ├── impl FakeStatus
// ├── default()
// ├── struct FakeRunnerState
// ├── struct FakeRunner
// ├── impl FakeRunner
// ├── add_git_project()
// ├── set_revision()
// ├── add_non_git_project()
// ├── set_status()
// ├── deadlines()
// ├── impl FakeRunner
// ├── run()
// ├── success()
// ├── failure()
// ├── struct FakeFactory
// ├── impl FakeFactory
// ├── starts()
// ├── set_control()
// ├── set_pid()
// ├── set_snapshot()
// ├── impl FakeFactory
// ├── start()
// ├── struct BlockingFactory
// ├── struct BlockingControl
// ├── impl BlockingControl
// ├── shutdown()
// ├── is_alive()
// ├── struct SingleControlFactory
// ├── struct HealthBlockingControl
// ├── impl HealthBlockingControl
// ├── shutdown()
// ├── is_alive()
// ├── struct HealthControlFactory
// ├── impl HealthControlFactory
// ├── start()
// ├── impl SingleControlFactory
// ├── start()
// ├── impl BlockingFactory
// ├── start()
// ├── enum FakeShutdownMode
// ├── struct FakeControlState
// ├── struct FakeControl
// ├── impl FakeControl
// ├── new()
// ├── set_mode()
// ├── set_alive()
// ├── shutdown_calls()
// ├── impl FakeControl
// ├── shutdown()
// ├── is_alive()
// ├── struct FakeMemory
// ├── impl FakeMemory
// ├── with_total()
// ├── set_rss()
// ├── batch_calls()
// ├── impl FakeMemory
// ├── total_physical_memory_bytes()
// ├── child_rss_bytes()
// ├── child_rss_bytes_batch()
// ├── temp_project()
// ├── write_status()
// ├── registry()
// ├── registry_with_memory()
// ├── add_ready_git_project()
// ├── snapshot_with_task()
// ├── struct TimeoutRunner
// ├── impl TimeoutRunner
// ├── run()
// ├── revision_resolution_distinguishes_named_unborn_detached_and_non_git()
// ├── identity_command_failure_is_not_misreported_as_non_git()
// ├── detached_and_ambiguous_non_git_projects_never_start_dashboard()
// ├── readable_marker_and_valid_two_second_status_probe_gate_startup()
// ├── unreadable_status_marker_does_not_start_dashboard()
// ├── selected_probe_detects_dow_init_completed_during_runtime()
// ├── same_project_revision_shares_service_and_snapshot()
// ├── different_projects_do_not_share_services()
// ├── successful_bash_reassociates_worktree_sessions_and_keeps_old_service()
// ├── activity_signals_track_exact_stop_times_and_active_state()
// ├── reassociation_preserves_runtime_activity_and_exact_stop_time()
// ├── slow_service_start_does_not_block_registry_state_updates()
// ├── activity_change_during_slow_sweep_keeps_a_live_service()
// ├── slow_protected_health_check_does_not_block_activity_updates()
// ├── closed_session_records_stop_time_and_enables_reclamation()
// ├── sweep_protects_current_and_active_services_and_reclaims_idle_ones()
// ├── memory_budget_is_max_of_five_percent_and_256_mib()
// ├── usage_accounts_for_child_rss_snapshots_and_fixed_overhead()
// ├── budget_pressure_reclaims_lru_order_and_stops_under_budget()
// ├── no_client_child_exits_within_window_and_cleanup_is_idempotent()
// ├── surviving_child_becomes_protected_and_is_never_force_killed()
// └── protected_revisit_reuses_alive_service_and_replaces_dead_one()

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, UNIX_EPOCH};

use async_trait::async_trait;
use rozsa_app::dev_flow::registry::{
    DashboardServiceControl, MemoryReader, NO_CLIENT_SHUTDOWN_WINDOW,
    REGISTRY_FIXED_OVERHEAD_BYTES_PER_SERVICE, ServiceShutdownOutcome,
};
use rozsa_app::dev_flow::{
    CommandExecutionError, CommandOutput, DashboardServiceFactory, DevFlowAvailability,
    DevFlowProjectKey, DevFlowProjectStatus, DevFlowRegistry, DevFlowRevisionKey,
    DevFlowServiceHandle, DevFlowSnapshot, DevFlowTask, DevFlowTaskStatus, ProjectCommandRunner,
    ProjectResolutionError, resolve_project_with,
};
use tempfile::TempDir;
use tokio::sync::{Notify, RwLock};

#[derive(Clone)]
enum FakeRevision {
    Named { branch: String, oid: String },
    Unborn { branch: String },
    Detached { oid: String },
}

#[derive(Clone)]
struct FakeStatus {
    success: bool,
    stdout: String,
    stderr: String,
}

impl Default for FakeStatus {
    fn default() -> Self {
        Self {
            success: true,
            stdout: r#"{"name":"test","phase":"DEV"}"#.to_owned(),
            stderr: String::new(),
        }
    }
}

#[derive(Default)]
struct FakeRunnerState {
    git_projects: HashMap<PathBuf, FakeRevision>,
    statuses: HashMap<PathBuf, FakeStatus>,
    deadlines: Vec<Duration>,
}

#[derive(Clone, Default)]
struct FakeRunner {
    state: Arc<Mutex<FakeRunnerState>>,
}

impl FakeRunner {
    fn add_git_project(&self, root: &Path, revision: FakeRevision) {
        let root = std::fs::canonicalize(root).unwrap();
        let mut state = self.state.lock().unwrap();
        state.git_projects.insert(root.clone(), revision);
        state.statuses.entry(root).or_default();
    }

    fn set_revision(&self, root: &Path, revision: FakeRevision) {
        self.state
            .lock()
            .unwrap()
            .git_projects
            .insert(std::fs::canonicalize(root).unwrap(), revision);
    }

    fn add_non_git_project(&self, root: &Path) {
        self.state
            .lock()
            .unwrap()
            .statuses
            .entry(std::fs::canonicalize(root).unwrap())
            .or_default();
    }

    fn set_status(&self, root: &Path, status: FakeStatus) {
        self.state
            .lock()
            .unwrap()
            .statuses
            .insert(std::fs::canonicalize(root).unwrap(), status);
    }

    fn deadlines(&self) -> Vec<Duration> {
        self.state.lock().unwrap().deadlines.clone()
    }
}

#[async_trait]
impl ProjectCommandRunner for FakeRunner {
    async fn run(
        &self,
        cwd: &Path,
        executable: &Path,
        args: &[&str],
        deadline: Duration,
    ) -> Result<CommandOutput, CommandExecutionError> {
        let cwd = std::fs::canonicalize(cwd)
            .map_err(|error| CommandExecutionError::Launch(error.to_string()))?;
        let mut state = self.state.lock().unwrap();
        state.deadlines.push(deadline);
        if executable == Path::new("git") {
            let project = state
                .git_projects
                .iter()
                .filter(|(root, _)| cwd.starts_with(root))
                .max_by_key(|(root, _)| root.components().count())
                .map(|(root, revision)| (root.clone(), revision.clone()));
            let Some((root, revision)) = project else {
                return Ok(failure("not a git repository"));
            };
            return Ok(match args {
                ["rev-parse", "--show-toplevel"] => success(&root.to_string_lossy()),
                ["symbolic-ref", "--quiet", "--short", "HEAD"] => match revision {
                    FakeRevision::Named { branch, .. } | FakeRevision::Unborn { branch } => {
                        success(&branch)
                    }
                    FakeRevision::Detached { .. } => failure("detached"),
                },
                ["rev-parse", "--verify", "HEAD"] => match revision {
                    FakeRevision::Named { oid, .. } | FakeRevision::Detached { oid } => {
                        success(&oid)
                    }
                    FakeRevision::Unborn { .. } => failure("unborn"),
                },
                _ => failure("unexpected git command"),
            });
        }

        let status = state
            .statuses
            .get(&cwd)
            .cloned()
            .unwrap_or_else(|| FakeStatus {
                success: false,
                stdout: String::new(),
                stderr: "unknown project".to_owned(),
            });
        Ok(CommandOutput {
            success: status.success,
            code: Some(if status.success { 0 } else { 1 }),
            stdout: status.stdout,
            stderr: status.stderr,
        })
    }
}

fn success(stdout: &str) -> CommandOutput {
    CommandOutput {
        success: true,
        code: Some(0),
        stdout: format!("{stdout}\n"),
        stderr: String::new(),
    }
}

fn failure(stderr: &str) -> CommandOutput {
    CommandOutput {
        success: false,
        code: Some(1),
        stdout: String::new(),
        stderr: stderr.to_owned(),
    }
}

#[derive(Default)]
struct FakeFactory {
    next_id: AtomicU64,
    starts: Mutex<Vec<DevFlowProjectKey>>,
    controls: Mutex<HashMap<DevFlowProjectKey, FakeControl>>,
    pids: Mutex<HashMap<DevFlowProjectKey, u32>>,
    snapshots: Mutex<HashMap<DevFlowProjectKey, DevFlowSnapshot>>,
}

impl FakeFactory {
    fn starts(&self) -> Vec<DevFlowProjectKey> {
        self.starts.lock().unwrap().clone()
    }

    fn set_control(&self, project: &DevFlowProjectKey, control: FakeControl) {
        self.controls
            .lock()
            .unwrap()
            .insert(project.clone(), control);
    }

    fn set_pid(&self, project: &DevFlowProjectKey, pid: u32) {
        self.pids.lock().unwrap().insert(project.clone(), pid);
    }

    fn set_snapshot(&self, project: &DevFlowProjectKey, snapshot: DevFlowSnapshot) {
        self.snapshots
            .lock()
            .unwrap()
            .insert(project.clone(), snapshot);
    }
}

#[async_trait]
impl DashboardServiceFactory for FakeFactory {
    async fn start(&self, project: &DevFlowProjectKey) -> Result<DevFlowServiceHandle, String> {
        self.starts.lock().unwrap().push(project.clone());
        let id = self.next_id.fetch_add(1, Ordering::SeqCst) + 1;
        let snapshot = self.snapshots.lock().unwrap().get(project).cloned();
        let snapshot = Arc::new(RwLock::new(snapshot));
        let control = self.controls.lock().unwrap().get(project).cloned();
        let pid = self.pids.lock().unwrap().get(project).copied();
        Ok(match control {
            Some(control) => DevFlowServiceHandle::with_child(id, snapshot, Arc::new(control), pid),
            None if pid.is_some() => DevFlowServiceHandle::with_child(
                id,
                snapshot,
                Arc::new(FakeControl::default()),
                pid,
            ),
            None => DevFlowServiceHandle::new(id, snapshot),
        })
    }
}

#[derive(Default)]
struct BlockingFactory {
    next_id: AtomicU64,
    block: AtomicBool,
    started: Notify,
    release: Notify,
}

#[derive(Default)]
struct BlockingControl {
    started: Notify,
    release: Notify,
}

#[async_trait]
impl DashboardServiceControl for BlockingControl {
    async fn shutdown(&self, _grace: Duration) -> ServiceShutdownOutcome {
        self.started.notify_one();
        self.release.notified().await;
        ServiceShutdownOutcome::Exited
    }

    async fn is_alive(&self) -> bool {
        true
    }
}

struct SingleControlFactory {
    next_id: AtomicU64,
    control: Arc<BlockingControl>,
}

#[derive(Default)]
struct HealthBlockingControl {
    started: Notify,
    release: Notify,
}

#[async_trait]
impl DashboardServiceControl for HealthBlockingControl {
    async fn shutdown(&self, _grace: Duration) -> ServiceShutdownOutcome {
        ServiceShutdownOutcome::StillRunning
    }

    async fn is_alive(&self) -> bool {
        self.started.notify_one();
        self.release.notified().await;
        true
    }
}

struct HealthControlFactory {
    next_id: AtomicU64,
    control: Arc<HealthBlockingControl>,
}

#[async_trait]
impl DashboardServiceFactory for HealthControlFactory {
    async fn start(&self, _project: &DevFlowProjectKey) -> Result<DevFlowServiceHandle, String> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst) + 1;
        Ok(DevFlowServiceHandle::with_child(
            id,
            Arc::new(RwLock::new(None)),
            self.control.clone(),
            None,
        ))
    }
}

#[async_trait]
impl DashboardServiceFactory for SingleControlFactory {
    async fn start(&self, _project: &DevFlowProjectKey) -> Result<DevFlowServiceHandle, String> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst) + 1;
        Ok(DevFlowServiceHandle::with_child(
            id,
            Arc::new(RwLock::new(None)),
            self.control.clone(),
            None,
        ))
    }
}

#[async_trait]
impl DashboardServiceFactory for BlockingFactory {
    async fn start(&self, _project: &DevFlowProjectKey) -> Result<DevFlowServiceHandle, String> {
        if self.block.load(Ordering::SeqCst) {
            self.started.notify_one();
            self.release.notified().await;
        }
        let id = self.next_id.fetch_add(1, Ordering::SeqCst) + 1;
        Ok(DevFlowServiceHandle::new(id, Arc::new(RwLock::new(None))))
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum FakeShutdownMode {
    #[default]
    Exited,
    StillRunning,
}

#[derive(Default)]
struct FakeControlState {
    shutdown_calls: Vec<Duration>,
    mode: FakeShutdownMode,
    alive: bool,
}

#[derive(Clone, Default)]
struct FakeControl {
    state: Arc<Mutex<FakeControlState>>,
}

impl FakeControl {
    fn new(mode: FakeShutdownMode, alive: bool) -> Self {
        let control = Self::default();
        control.set_mode(mode);
        control.set_alive(alive);
        control
    }

    fn set_mode(&self, mode: FakeShutdownMode) {
        self.state.lock().unwrap().mode = mode;
    }

    fn set_alive(&self, alive: bool) {
        self.state.lock().unwrap().alive = alive;
    }

    fn shutdown_calls(&self) -> Vec<Duration> {
        self.state.lock().unwrap().shutdown_calls.clone()
    }
}

#[async_trait]
impl DashboardServiceControl for FakeControl {
    async fn shutdown(&self, grace: Duration) -> ServiceShutdownOutcome {
        let mut state = self.state.lock().unwrap();
        state.shutdown_calls.push(grace);
        match state.mode {
            FakeShutdownMode::Exited => ServiceShutdownOutcome::Exited,
            FakeShutdownMode::StillRunning => ServiceShutdownOutcome::StillRunning,
        }
    }

    async fn is_alive(&self) -> bool {
        self.state.lock().unwrap().alive
    }
}

#[derive(Clone, Default)]
struct FakeMemory {
    total: Option<u64>,
    rss: Arc<Mutex<HashMap<u32, u64>>>,
    batch_calls: Arc<AtomicU64>,
}

impl FakeMemory {
    fn with_total(total: u64) -> Self {
        Self {
            total: Some(total),
            ..Default::default()
        }
    }

    fn set_rss(&self, pid: u32, bytes: u64) {
        self.rss.lock().unwrap().insert(pid, bytes);
    }

    fn batch_calls(&self) -> u64 {
        self.batch_calls.load(Ordering::SeqCst)
    }
}

impl MemoryReader for FakeMemory {
    fn total_physical_memory_bytes(&self) -> Option<u64> {
        self.total
    }

    fn child_rss_bytes(&self, pid: u32) -> Option<u64> {
        self.rss.lock().unwrap().get(&pid).copied()
    }

    fn child_rss_bytes_batch(&self, _pids: &[u32]) -> HashMap<u32, u64> {
        self.batch_calls.fetch_add(1, Ordering::SeqCst);
        self.rss.lock().unwrap().clone()
    }
}

fn temp_project() -> TempDir {
    tempfile::tempdir_in("tmp")
        .or_else(|_| tempfile::tempdir())
        .unwrap()
}

fn write_status(root: &Path, branch: &str) {
    let directory = root.join(".dev-doc").join(branch);
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("STATUS.yaml"), "phase: DEV\n").unwrap();
}

fn registry(runner: Arc<FakeRunner>, factory: Arc<FakeFactory>) -> DevFlowRegistry {
    DevFlowRegistry::new(PathBuf::from("/fake/dow"), runner, factory)
}

fn registry_with_memory(
    runner: Arc<FakeRunner>,
    factory: Arc<FakeFactory>,
    memory: Arc<FakeMemory>,
) -> DevFlowRegistry {
    DevFlowRegistry::with_memory_reader(PathBuf::from("/fake/dow"), runner, factory, memory)
}

fn add_ready_git_project(runner: &Arc<FakeRunner>, project: &TempDir, oid: &str, branch: &str) {
    runner.add_git_project(
        project.path(),
        FakeRevision::Named {
            branch: branch.to_owned(),
            oid: oid.to_owned(),
        },
    );
    write_status(project.path(), branch);
}

fn snapshot_with_task(title: &str) -> DevFlowSnapshot {
    DevFlowSnapshot {
        revision: 1,
        project: DevFlowProjectStatus {
            name: None,
            phase: None,
            mode: None,
            version: None,
            goals_minor: None,
            updated: None,
        },
        tasks: vec![DevFlowTask {
            id: "TASK-T001".to_owned(),
            title: title.to_owned(),
            status: DevFlowTaskStatus::Pending,
            priority: None,
            complexity: None,
            task_type: None,
            refs: None,
            depends_on: Vec::new(),
            done_when: Vec::new(),
            files_create: Vec::new(),
            files_modify: Vec::new(),
            files_test: Vec::new(),
        }],
        issues: Vec::new(),
        received_at: UNIX_EPOCH,
        stale: false,
    }
}

struct TimeoutRunner;

#[async_trait]
impl ProjectCommandRunner for TimeoutRunner {
    async fn run(
        &self,
        _cwd: &Path,
        _executable: &Path,
        _args: &[&str],
        deadline: Duration,
    ) -> Result<CommandOutput, CommandExecutionError> {
        Err(CommandExecutionError::Timeout(deadline))
    }
}

#[tokio::test]
async fn revision_resolution_distinguishes_named_unborn_detached_and_non_git() {
    let git_project = temp_project();
    let non_git = temp_project();
    let runner = FakeRunner::default();
    runner.add_git_project(
        git_project.path(),
        FakeRevision::Named {
            branch: "main".to_owned(),
            oid: "a".repeat(40),
        },
    );

    let named = resolve_project_with(git_project.path(), &runner)
        .await
        .unwrap();
    assert_eq!(
        named.revision,
        DevFlowRevisionKey::NamedBranch("main".to_owned())
    );

    runner.set_revision(
        git_project.path(),
        FakeRevision::Unborn {
            branch: "feature/new".to_owned(),
        },
    );
    let unborn = resolve_project_with(git_project.path(), &runner)
        .await
        .unwrap();
    assert_eq!(
        unborn.revision,
        DevFlowRevisionKey::UnbornBranch("feature/new".to_owned())
    );

    runner.set_revision(
        git_project.path(),
        FakeRevision::Detached {
            oid: "b".repeat(40),
        },
    );
    let detached = resolve_project_with(git_project.path(), &runner)
        .await
        .unwrap();
    assert_eq!(
        detached.revision,
        DevFlowRevisionKey::DetachedCommit("b".repeat(40))
    );

    let non_git = resolve_project_with(non_git.path(), &runner).await.unwrap();
    assert_eq!(non_git.revision, DevFlowRevisionKey::NonGit);
}

#[tokio::test]
async fn identity_command_failure_is_not_misreported_as_non_git() {
    let project = temp_project();
    let error = resolve_project_with(project.path(), &TimeoutRunner)
        .await
        .unwrap_err();
    assert!(matches!(error, ProjectResolutionError::Command(_)));
}

#[tokio::test]
async fn detached_and_ambiguous_non_git_projects_never_start_dashboard() {
    let detached = temp_project();
    let non_git = temp_project();
    let runner = Arc::new(FakeRunner::default());
    runner.add_git_project(
        detached.path(),
        FakeRevision::Detached {
            oid: "c".repeat(40),
        },
    );
    runner.add_non_git_project(non_git.path());
    write_status(non_git.path(), "main");
    write_status(non_git.path(), "other");
    let factory = Arc::new(FakeFactory::default());
    let registry = registry(runner, factory.clone());

    let detached_state = registry
        .associate_session("detached", detached.path().to_path_buf())
        .await
        .unwrap();
    assert_eq!(
        detached_state.availability,
        DevFlowAvailability::UnsupportedRevision
    );
    let ambiguous_state = registry
        .associate_session("non-git", non_git.path().to_path_buf())
        .await
        .unwrap();
    assert_eq!(
        ambiguous_state.availability,
        DevFlowAvailability::AmbiguousNonGitProject
    );
    assert!(factory.starts().is_empty());
}

#[tokio::test]
async fn readable_marker_and_valid_two_second_status_probe_gate_startup() {
    let project = temp_project();
    let runner = Arc::new(FakeRunner::default());
    runner.add_git_project(
        project.path(),
        FakeRevision::Named {
            branch: "main".to_owned(),
            oid: "d".repeat(40),
        },
    );
    let factory = Arc::new(FakeFactory::default());
    let registry = registry(runner.clone(), factory.clone());

    let missing = registry
        .associate_session("session", project.path().to_path_buf())
        .await
        .unwrap();
    assert_eq!(
        missing.availability,
        DevFlowAvailability::ProjectNotInitialized
    );

    write_status(project.path(), "main");
    runner.set_status(
        project.path(),
        FakeStatus {
            stdout: "not-json".to_owned(),
            ..FakeStatus::default()
        },
    );
    let invalid = registry.probe_selected("session").await.unwrap().unwrap();
    assert!(matches!(
        invalid.availability,
        DevFlowAvailability::StatusProbeFailed(_)
    ));
    assert!(factory.starts().is_empty());

    runner.set_status(project.path(), FakeStatus::default());
    let ready = registry.probe_selected("session").await.unwrap().unwrap();
    assert_eq!(ready.availability, DevFlowAvailability::Ready);
    assert!(ready.service.is_some());
    assert_eq!(factory.starts().len(), 1);
    assert_eq!(DevFlowRegistry::probe_interval(), Duration::from_secs(2));
    assert!(
        runner
            .deadlines()
            .into_iter()
            .all(|deadline| deadline == Duration::from_secs(2))
    );
}

#[tokio::test]
async fn unreadable_status_marker_does_not_start_dashboard() {
    let project = temp_project();
    let runner = Arc::new(FakeRunner::default());
    runner.add_git_project(
        project.path(),
        FakeRevision::Unborn {
            branch: "new".to_owned(),
        },
    );
    let marker = project.path().join(".dev-doc/new/STATUS.yaml");
    std::fs::create_dir_all(&marker).unwrap();
    let factory = Arc::new(FakeFactory::default());
    let registry = registry(runner, factory.clone());

    let state = registry
        .associate_session("session", project.path().to_path_buf())
        .await
        .unwrap();
    let canonical_marker = std::fs::canonicalize(project.path())
        .unwrap()
        .join(".dev-doc/new/STATUS.yaml");
    assert_eq!(
        state.availability,
        DevFlowAvailability::StatusUnreadable(canonical_marker)
    );
    assert!(factory.starts().is_empty());
}

#[tokio::test]
async fn selected_probe_detects_dow_init_completed_during_runtime() {
    let project = temp_project();
    let runner = Arc::new(FakeRunner::default());
    runner.add_git_project(
        project.path(),
        FakeRevision::Named {
            branch: "main".to_owned(),
            oid: "e".repeat(40),
        },
    );
    let factory = Arc::new(FakeFactory::default());
    let registry = registry(runner, factory.clone());
    let first = registry
        .associate_session("session", project.path().to_path_buf())
        .await
        .unwrap();
    assert_eq!(
        first.availability,
        DevFlowAvailability::ProjectNotInitialized
    );

    write_status(project.path(), "main");
    let rescanned = registry.probe_selected("session").await.unwrap().unwrap();
    assert_eq!(rescanned.availability, DevFlowAvailability::Ready);
    assert_eq!(factory.starts().len(), 1);
}

#[tokio::test]
async fn same_project_revision_shares_service_and_snapshot() {
    let project = temp_project();
    let runner = Arc::new(FakeRunner::default());
    runner.add_git_project(
        project.path(),
        FakeRevision::Named {
            branch: "main".to_owned(),
            oid: "f".repeat(40),
        },
    );
    write_status(project.path(), "main");
    let factory = Arc::new(FakeFactory::default());
    let registry = registry(runner, factory.clone());

    let first = registry
        .associate_session("one", project.path().to_path_buf())
        .await
        .unwrap();
    let second = registry
        .associate_session("two", project.path().to_path_buf())
        .await
        .unwrap();
    let first_service = first.service.unwrap();
    let second_service = second.service.unwrap();
    assert_eq!(first_service.id(), second_service.id());
    assert!(Arc::ptr_eq(
        &first_service.snapshot(),
        &second_service.snapshot()
    ));
    assert_eq!(factory.starts().len(), 1);
}

#[tokio::test]
async fn different_projects_do_not_share_services() {
    let first_project = temp_project();
    let second_project = temp_project();
    let runner = Arc::new(FakeRunner::default());
    for (project, oid) in [
        (&first_project, "1".repeat(40)),
        (&second_project, "2".repeat(40)),
    ] {
        runner.add_git_project(
            project.path(),
            FakeRevision::Named {
                branch: "main".to_owned(),
                oid,
            },
        );
        write_status(project.path(), "main");
    }
    let factory = Arc::new(FakeFactory::default());
    let registry = registry(runner, factory);

    let first = registry
        .associate_session("one", first_project.path().to_path_buf())
        .await
        .unwrap();
    let second = registry
        .associate_session("two", second_project.path().to_path_buf())
        .await
        .unwrap();
    assert_ne!(first.service.unwrap().id(), second.service.unwrap().id());
    assert_eq!(registry.service_count().await, 2);
}

#[tokio::test]
async fn successful_bash_reassociates_worktree_sessions_and_keeps_old_service() {
    let project = temp_project();
    let runner = Arc::new(FakeRunner::default());
    runner.add_git_project(
        project.path(),
        FakeRevision::Named {
            branch: "main".to_owned(),
            oid: "3".repeat(40),
        },
    );
    write_status(project.path(), "main");
    let factory = Arc::new(FakeFactory::default());
    let registry = registry(runner.clone(), factory);
    let one = registry
        .associate_session("one", project.path().to_path_buf())
        .await
        .unwrap();
    registry
        .associate_session("two", project.path().to_path_buf())
        .await
        .unwrap();
    let old_id = one.service.unwrap().id();

    runner.set_revision(
        project.path(),
        FakeRevision::Named {
            branch: "feature".to_owned(),
            oid: "4".repeat(40),
        },
    );
    write_status(project.path(), "feature");
    let states = registry
        .rescan_after_successful_bash(project.path())
        .await
        .unwrap();
    assert_eq!(states.len(), 2);
    let new_ids = states
        .iter()
        .map(|(_, state)| state.service.as_ref().unwrap().id())
        .collect::<Vec<_>>();
    assert!(new_ids.iter().all(|id| *id == new_ids[0]));
    assert_ne!(old_id, new_ids[0]);
    assert_eq!(registry.service_count().await, 2);
    for session in ["one", "two"] {
        assert_eq!(
            registry
                .session_state(session)
                .await
                .unwrap()
                .project
                .revision,
            DevFlowRevisionKey::NamedBranch("feature".to_owned())
        );
    }
}

#[tokio::test]
async fn activity_signals_track_exact_stop_times_and_active_state() {
    let project = temp_project();
    let runner = Arc::new(FakeRunner::default());
    add_ready_git_project(&runner, &project, &"a".repeat(40), "main");
    let factory = Arc::new(FakeFactory::default());
    let registry = registry(runner, factory);
    let associated = registry
        .associate_session("session", project.path().to_path_buf())
        .await
        .unwrap();
    let key = associated.project;

    registry.session_active("session").await;
    assert_eq!(registry.diagnostics().await.active_sessions, 1);

    let stopped_at = UNIX_EPOCH + Duration::from_secs(2_000_000);
    let active_sweep = registry
        .sweep(stopped_at + Duration::from_secs(15 * 60 + 60))
        .await;
    assert!(
        active_sweep.reclaimed.is_empty(),
        "active service is protected"
    );

    registry.session_finished("session", stopped_at).await;
    assert_eq!(registry.diagnostics().await.active_sessions, 0);

    let before = registry
        .sweep(stopped_at + Duration::from_secs(15 * 60 - 1))
        .await;
    assert!(before.reclaimed.is_empty(), "not idle before 15 minutes");

    let at = registry
        .sweep(stopped_at + Duration::from_secs(15 * 60))
        .await;
    assert_eq!(at.reclaimed, vec![key]);

    // A newer finish extends the idle horizon to the exact newest stop time.
    registry
        .associate_session("session", project.path().to_path_buf())
        .await
        .unwrap();
    registry
        .session_finished("session", stopped_at + Duration::from_secs(60))
        .await;
    let still_recent = registry
        .sweep(stopped_at + Duration::from_secs(15 * 60 + 59))
        .await;
    assert!(still_recent.reclaimed.is_empty());
    let now_idle = registry
        .sweep(stopped_at + Duration::from_secs(15 * 60 + 60))
        .await;
    assert_eq!(now_idle.reclaimed.len(), 1);
}

#[tokio::test]
async fn reassociation_preserves_runtime_activity_and_exact_stop_time() {
    let project = temp_project();
    let runner = Arc::new(FakeRunner::default());
    add_ready_git_project(&runner, &project, &"a".repeat(40), "main");
    let factory = Arc::new(FakeFactory::default());
    let registry = registry(runner.clone(), factory);

    registry
        .associate_session("session", project.path().to_path_buf())
        .await
        .unwrap();
    registry.session_active("session").await;
    registry
        .rescan_after_successful_bash(project.path())
        .await
        .unwrap();
    assert_eq!(registry.diagnostics().await.active_sessions, 1);
    assert_eq!(registry.last_stop_at("session").await, None);

    runner.set_revision(
        project.path(),
        FakeRevision::Named {
            branch: "feature".to_owned(),
            oid: "b".repeat(40),
        },
    );
    write_status(project.path(), "feature");
    registry
        .rescan_after_successful_bash(project.path())
        .await
        .unwrap();
    assert_eq!(registry.diagnostics().await.active_sessions, 1);

    let stopped_at = UNIX_EPOCH + Duration::from_secs(2_050_000);
    registry.session_finished("session", stopped_at).await;
    registry
        .rescan_after_successful_bash(project.path())
        .await
        .unwrap();
    assert_eq!(registry.diagnostics().await.active_sessions, 0);
    assert_eq!(registry.last_stop_at("session").await, Some(stopped_at));
}

#[tokio::test]
async fn slow_service_start_does_not_block_registry_state_updates() {
    let first = temp_project();
    let second = temp_project();
    let runner = Arc::new(FakeRunner::default());
    add_ready_git_project(&runner, &first, &"1".repeat(40), "main");
    add_ready_git_project(&runner, &second, &"2".repeat(40), "main");
    let factory = Arc::new(BlockingFactory::default());
    let registry = Arc::new(DevFlowRegistry::new(
        PathBuf::from("/fake/dow"),
        runner,
        factory.clone(),
    ));
    let first_state = registry
        .associate_session("first", first.path().to_path_buf())
        .await
        .unwrap();
    registry.session_active("first").await;

    factory.block.store(true, Ordering::SeqCst);
    let starting_registry = registry.clone();
    let second_cwd = second.path().to_path_buf();
    let start = tokio::spawn(async move {
        starting_registry
            .associate_session("second", second_cwd)
            .await
            .unwrap()
    });
    factory.started.notified().await;

    tokio::time::timeout(Duration::from_millis(100), async {
        registry.session_active("first").await;
        registry
            .set_current_project(Some(first_state.project.clone()))
            .await;
        assert_eq!(registry.diagnostics().await.active_sessions, 1);
    })
    .await
    .expect("registry state remains available while service start is pending");

    factory.release.notify_one();
    start.await.unwrap();
}

#[tokio::test]
async fn activity_change_during_slow_sweep_keeps_a_live_service() {
    let project = temp_project();
    let runner = Arc::new(FakeRunner::default());
    add_ready_git_project(&runner, &project, &"3".repeat(40), "main");
    let control = Arc::new(BlockingControl::default());
    let blocking_factory = Arc::new(SingleControlFactory {
        next_id: AtomicU64::new(0),
        control: control.clone(),
    });
    let registry = Arc::new(DevFlowRegistry::new(
        PathBuf::from("/fake/dow"),
        runner,
        blocking_factory,
    ));
    registry
        .associate_session("session", project.path().to_path_buf())
        .await
        .unwrap();
    let stopped_at = UNIX_EPOCH + Duration::from_secs(2_060_000);
    registry.session_finished("session", stopped_at).await;

    let sweeping_registry = registry.clone();
    let sweep = tokio::spawn(async move {
        sweeping_registry
            .sweep(stopped_at + Duration::from_secs(15 * 60))
            .await
    });
    control.started.notified().await;
    tokio::time::timeout(
        Duration::from_millis(100),
        registry.session_active("session"),
    )
    .await
    .expect("activity update is not blocked by child shutdown");
    control.release.notify_one();
    let report = sweep.await.unwrap();

    assert!(report.reclaimed.is_empty());
    assert_eq!(registry.diagnostics().await.active_sessions, 1);
    assert_eq!(registry.service_count().await, 1);
}

#[tokio::test]
async fn slow_protected_health_check_does_not_block_activity_updates() {
    let project = temp_project();
    let runner = Arc::new(FakeRunner::default());
    add_ready_git_project(&runner, &project, &"4".repeat(40), "main");
    let control = Arc::new(HealthBlockingControl::default());
    let registry = Arc::new(DevFlowRegistry::new(
        PathBuf::from("/fake/dow"),
        runner,
        Arc::new(HealthControlFactory {
            next_id: AtomicU64::new(0),
            control: control.clone(),
        }),
    ));
    registry
        .associate_session("session", project.path().to_path_buf())
        .await
        .unwrap();
    let stopped_at = UNIX_EPOCH + Duration::from_secs(2_070_000);
    registry.session_finished("session", stopped_at).await;
    let protected = registry
        .sweep(stopped_at + Duration::from_secs(15 * 60))
        .await;
    assert_eq!(protected.protected.len(), 1);

    let revisiting_registry = registry.clone();
    let cwd = project.path().to_path_buf();
    let revisit = tokio::spawn(async move {
        revisiting_registry
            .associate_session("session", cwd)
            .await
            .unwrap()
    });
    control.started.notified().await;
    tokio::time::timeout(
        Duration::from_millis(100),
        registry.session_active("session"),
    )
    .await
    .expect("activity update is not blocked by protected-service health check");
    control.release.notify_one();
    revisit.await.unwrap();
    assert_eq!(registry.diagnostics().await.active_sessions, 1);
}

#[tokio::test]
async fn closed_session_records_stop_time_and_enables_reclamation() {
    let project = temp_project();
    let runner = Arc::new(FakeRunner::default());
    add_ready_git_project(&runner, &project, &"c".repeat(40), "main");
    let factory = Arc::new(FakeFactory::default());
    let registry = registry(runner, factory);
    let associated = registry
        .associate_session("session", project.path().to_path_buf())
        .await
        .unwrap();
    let key = associated.project;

    let stopped_at = UNIX_EPOCH + Duration::from_secs(2_100_000);
    registry.session_closed("session", stopped_at).await;
    assert!(registry.session_state("session").await.is_none());

    let report = registry
        .sweep(stopped_at + Duration::from_secs(15 * 60))
        .await;
    assert_eq!(report.reclaimed, vec![key]);
}

#[tokio::test]
async fn sweep_protects_current_and_active_services_and_reclaims_idle_ones() {
    let first = temp_project();
    let second = temp_project();
    let third = temp_project();
    let runner = Arc::new(FakeRunner::default());
    let projects = [&first, &second, &third];
    for (index, project) in projects.iter().enumerate() {
        add_ready_git_project(&runner, project, &format!("{index}").repeat(40), "main");
    }
    let factory = Arc::new(FakeFactory::default());
    let registry = registry(runner, factory);
    let mut keys = Vec::new();
    for (index, project) in projects.iter().enumerate() {
        let state = registry
            .associate_session(format!("session-{index}"), project.path().to_path_buf())
            .await
            .unwrap();
        keys.push(state.project);
    }

    let stopped_at = UNIX_EPOCH + Duration::from_secs(2_200_000);
    for index in 0..3 {
        registry
            .session_finished(&format!("session-{index}"), stopped_at)
            .await;
    }
    registry.set_current_project(Some(keys[0].clone())).await;
    registry.session_active("session-1").await;

    let report = registry
        .sweep(stopped_at + Duration::from_secs(15 * 60 + 5))
        .await;
    assert_eq!(report.reclaimed, vec![keys[2].clone()]);
    assert_eq!(registry.service_count().await, 2);
    let diagnostics = registry.diagnostics().await;
    assert_eq!(diagnostics.active_sessions, 1);
    assert_eq!(diagnostics.live_services, 2);
}

#[test]
fn memory_budget_is_max_of_five_percent_and_256_mib() {
    let gib = 1024 * 1024 * 1024;
    assert_eq!(DevFlowRegistry::memory_budget(gib), 256 * 1024 * 1024);
    assert_eq!(DevFlowRegistry::memory_budget(16 * gib), 16 * gib / 20);
    assert_eq!(DevFlowRegistry::memory_budget(0), 256 * 1024 * 1024);
}

#[tokio::test]
async fn usage_accounts_for_child_rss_snapshots_and_fixed_overhead() {
    let first = temp_project();
    let second = temp_project();
    let runner = Arc::new(FakeRunner::default());
    add_ready_git_project(&runner, &first, &"d".repeat(40), "main");
    add_ready_git_project(&runner, &second, &"e".repeat(40), "main");
    let first_key = resolve_project_with(first.path(), runner.as_ref())
        .await
        .unwrap();
    let second_key = resolve_project_with(second.path(), runner.as_ref())
        .await
        .unwrap();
    let factory = Arc::new(FakeFactory::default());
    factory.set_pid(&first_key, 4242);
    factory.set_snapshot(&first_key, snapshot_with_task("short"));
    let memory = Arc::new(FakeMemory::with_total(1 << 30));
    let registry = registry_with_memory(runner, factory.clone(), memory.clone());

    let associated = registry
        .associate_session("first", first.path().to_path_buf())
        .await
        .unwrap();
    let handle = associated.service.unwrap();
    assert_eq!(handle.id(), 1);

    memory.set_rss(4242, 300 * 1024 * 1024);
    let project_usage = registry.project_usage_bytes(&first_key).await.unwrap();
    assert!(
        project_usage >= 300 * 1024 * 1024 + REGISTRY_FIXED_OVERHEAD_BYTES_PER_SERVICE,
        "project usage includes child RSS and registry overhead"
    );
    assert_eq!(registry.project_usage_bytes(&second_key).await, None);
    let diagnostics = registry.diagnostics().await;
    assert!(
        diagnostics.over_budget,
        "300 MiB RSS exceeds the 256 MiB budget"
    );
    assert!(
        diagnostics.usage_bytes >= 300 * 1024 * 1024 + REGISTRY_FIXED_OVERHEAD_BYTES_PER_SERVICE
    );

    memory.set_rss(4242, 0);
    let usage_short = registry.diagnostics().await.usage_bytes;
    assert!(
        (REGISTRY_FIXED_OVERHEAD_BYTES_PER_SERVICE
            ..REGISTRY_FIXED_OVERHEAD_BYTES_PER_SERVICE + 2048)
            .contains(&usage_short),
        "usage includes the documented fixed per-service overhead"
    );
    factory.set_snapshot(&second_key, snapshot_with_task(&"x".repeat(4096)));
    registry
        .associate_session("second", second.path().to_path_buf())
        .await
        .unwrap();
    let batch_calls = memory.batch_calls();
    let usage_long = registry.diagnostics().await.usage_bytes;
    assert_eq!(memory.batch_calls(), batch_calls + 1);
    let snapshot_delta = usage_long - usage_short - REGISTRY_FIXED_OVERHEAD_BYTES_PER_SERVICE;
    assert!(
        (4096..4096 + 2048).contains(&snapshot_delta),
        "snapshot bytes are counted in usage: {snapshot_delta}"
    );
}

#[tokio::test]
async fn budget_pressure_reclaims_lru_order_and_stops_under_budget() {
    let projects = [temp_project(), temp_project(), temp_project()];
    let runner = Arc::new(FakeRunner::default());
    for (index, project) in projects.iter().enumerate() {
        add_ready_git_project(&runner, project, &format!("{index}").repeat(40), "main");
    }
    let factory = Arc::new(FakeFactory::default());
    let memory = Arc::new(FakeMemory::with_total(1 << 30));
    let registry = registry_with_memory(runner.clone(), factory.clone(), memory.clone());

    let mut keys = Vec::new();
    for (index, project) in projects.iter().enumerate() {
        let key = resolve_project_with(project.path(), runner.as_ref())
            .await
            .unwrap();
        keys.push(key.clone());
        let pid = 5000 + index as u32;
        factory.set_pid(&key, pid);
        memory.set_rss(pid, 200 * 1024 * 1024);
        registry
            .associate_session(format!("session-{index}"), project.path().to_path_buf())
            .await
            .unwrap();
    }
    let stopped_at = UNIX_EPOCH + Duration::from_secs(2_300_000);
    for index in 0..3 {
        registry
            .session_finished(&format!("session-{index}"), stopped_at)
            .await;
    }
    assert!(registry.diagnostics().await.over_budget);

    let report = registry.sweep(stopped_at + Duration::from_secs(60)).await;
    assert_eq!(report.reclaimed, vec![keys[0].clone(), keys[1].clone()]);
    assert_eq!(registry.service_count().await, 1);
    let diagnostics = registry.diagnostics().await;
    assert_eq!(diagnostics.live_services, 1);
    assert!(!diagnostics.over_budget);
}

#[tokio::test]
async fn no_client_child_exits_within_window_and_cleanup_is_idempotent() {
    let project = temp_project();
    let runner = Arc::new(FakeRunner::default());
    add_ready_git_project(&runner, &project, &"f".repeat(40), "main");
    let key = resolve_project_with(project.path(), runner.as_ref())
        .await
        .unwrap();
    let control = FakeControl::new(FakeShutdownMode::Exited, true);
    let factory = Arc::new(FakeFactory::default());
    factory.set_control(&key, control.clone());
    let registry = registry(runner, factory);
    let associated = registry
        .associate_session("session", project.path().to_path_buf())
        .await
        .unwrap();
    let first_id = associated.service.unwrap().id();

    let stopped_at = UNIX_EPOCH + Duration::from_secs(2_400_000);
    registry.session_finished("session", stopped_at).await;
    let report = registry
        .sweep(stopped_at + Duration::from_secs(15 * 60))
        .await;
    assert_eq!(report.reclaimed, vec![key.clone()]);
    assert_eq!(control.shutdown_calls(), vec![NO_CLIENT_SHUTDOWN_WINDOW]);

    let again = registry
        .sweep(stopped_at + Duration::from_secs(15 * 60 + 60))
        .await;
    assert!(again.reclaimed.is_empty());
    assert_eq!(
        control.shutdown_calls().len(),
        1,
        "cleanup must be idempotent"
    );

    let stale = registry.session_state("session").await.unwrap();
    assert_eq!(
        stale.service.unwrap().id(),
        first_id,
        "stale cache is retained"
    );
    let state = registry
        .associate_session("session", project.path().to_path_buf())
        .await
        .unwrap();
    assert_eq!(state.service.unwrap().id(), first_id + 1);
}

#[tokio::test]
async fn surviving_child_becomes_protected_and_is_never_force_killed() {
    let project = temp_project();
    let runner = Arc::new(FakeRunner::default());
    add_ready_git_project(&runner, &project, &"9".repeat(40), "main");
    let key = resolve_project_with(project.path(), runner.as_ref())
        .await
        .unwrap();
    let control = FakeControl::new(FakeShutdownMode::StillRunning, true);
    let factory = Arc::new(FakeFactory::default());
    factory.set_control(&key, control.clone());
    factory.set_pid(&key, 7777);
    let memory = Arc::new(FakeMemory::with_total(1 << 30));
    let registry = registry_with_memory(runner, factory, memory.clone());
    registry
        .associate_session("session", project.path().to_path_buf())
        .await
        .unwrap();

    let stopped_at = UNIX_EPOCH + Duration::from_secs(2_500_000);
    registry.session_finished("session", stopped_at).await;
    memory.set_rss(7777, 300 * 1024 * 1024);
    assert!(registry.diagnostics().await.over_budget);
    let report = registry
        .sweep(stopped_at + Duration::from_secs(15 * 60))
        .await;
    assert!(report.reclaimed.is_empty());
    assert_eq!(report.protected, vec![key.clone()]);
    assert_eq!(
        control.shutdown_calls(),
        vec![NO_CLIENT_SHUTDOWN_WINDOW],
        "protected child must never be force-killed"
    );
    let diagnostics = registry.diagnostics().await;
    assert_eq!(diagnostics.protected_services, 1);
    assert!(
        diagnostics.over_budget,
        "temporary budget excess is visible"
    );
    assert!(diagnostics.protected_usage_bytes >= 300 * 1024 * 1024);

    let second = registry
        .sweep(stopped_at + Duration::from_secs(15 * 60 + 60))
        .await;
    assert!(second.protected.is_empty());
    assert_eq!(
        control.shutdown_calls(),
        vec![NO_CLIENT_SHUTDOWN_WINDOW, Duration::ZERO],
        "protected children are rechecked with zero grace"
    );
    assert_eq!(registry.diagnostics().await.protected_services, 1);

    control.set_mode(FakeShutdownMode::Exited);
    let third = registry
        .sweep(stopped_at + Duration::from_secs(15 * 60 + 120))
        .await;
    assert_eq!(third.reclaimed, vec![key]);
    assert_eq!(registry.service_count().await, 0);
}

#[tokio::test]
async fn protected_revisit_reuses_alive_service_and_replaces_dead_one() {
    let project = temp_project();
    let runner = Arc::new(FakeRunner::default());
    add_ready_git_project(&runner, &project, &"1".repeat(40), "main");
    let key = resolve_project_with(project.path(), runner.as_ref())
        .await
        .unwrap();
    let control = FakeControl::new(FakeShutdownMode::StillRunning, true);
    let factory = Arc::new(FakeFactory::default());
    factory.set_control(&key, control.clone());
    let registry = registry(runner, factory.clone());
    let associated = registry
        .associate_session("session", project.path().to_path_buf())
        .await
        .unwrap();
    let first_id = associated.service.unwrap().id();

    let stopped_at = UNIX_EPOCH + Duration::from_secs(2_600_000);
    registry.session_finished("session", stopped_at).await;
    registry
        .sweep(stopped_at + Duration::from_secs(15 * 60))
        .await;
    assert_eq!(registry.diagnostics().await.protected_services, 1);

    let state = registry
        .associate_session("session", project.path().to_path_buf())
        .await
        .unwrap();
    assert_eq!(
        state.service.unwrap().id(),
        first_id,
        "alive service is reused"
    );
    assert_eq!(factory.starts().len(), 1, "no replacement is started");
    assert_eq!(registry.diagnostics().await.protected_services, 0);

    registry.session_finished("session", stopped_at).await;
    registry
        .sweep(stopped_at + Duration::from_secs(15 * 60 + 60))
        .await;
    control.set_alive(false);
    let state = registry
        .associate_session("session", project.path().to_path_buf())
        .await
        .unwrap();
    assert_eq!(
        state.service.unwrap().id(),
        first_id + 1,
        "dead service is replaced"
    );
    assert_eq!(factory.starts().len(), 2);
}
