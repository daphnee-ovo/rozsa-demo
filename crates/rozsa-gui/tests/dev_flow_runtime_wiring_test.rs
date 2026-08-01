// FrameworkTree
// dev_flow_runtime_wiring_test.rs
// ├── enum FakeRevision
// ├── struct FakeStatus
// ├── impl FakeStatus
// ├── default()
// ├── success()
// ├── failure()
// ├── struct FakeRunnerState
// ├── struct FakeRunner
// ├── impl FakeRunner
// ├── add_git_project()
// ├── set_branch()
// ├── set_status()
// ├── impl FakeRunner
// ├── run()
// ├── struct FakeDiscoveryRunner
// ├── impl FakeDiscoveryRunner
// ├── set_found()
// ├── impl FakeDiscoveryRunner
// ├── run()
// ├── struct FakeControlState
// ├── struct FakeControl
// ├── impl FakeControl
// ├── shutdown()
// ├── is_alive()
// ├── struct FakeFactoryState
// ├── struct FakeFactory
// ├── impl FakeFactory
// ├── starts()
// ├── shutdown_calls()
// ├── snapshot()
// ├── impl FakeFactory
// ├── start()
// ├── environment()
// ├── notifier()
// ├── no_notifier()
// ├── write_status()
// ├── system_runtime_and_late_attachment_share_one_sidebar_refresh_slot()
// ├── session_start_and_finish_track_active_state_and_exact_stop_time()
// ├── disable_stops_owned_children_and_reenable_restarts_services()
// ├── invalid_custom_path_fails_without_falling_back()
// ├── branch_change_rescan_and_switch_reassociate_sessions()
// ├── late_init_becomes_ready_via_probe()
// ├── cli_error_notification_resolves_on_recovery()
// ├── routine_ready_emits_no_extra_notification()
// ├── dashboard_start_failure_uses_per_project_error_id()
// ├── repeated_project_failure_updates_the_same_notification_id()
// ├── struct FailingFactory
// ├── impl FailingFactory
// ├── start()
// ├── struct UpdatingFailingFactory
// ├── impl UpdatingFailingFactory
// └── start()

//! Runtime wiring tests: settings adoption, session activity signals, rescan
//! triggers, and stable notification IDs for the GUI dev-flow runtime.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use rozsa_app::dev_flow::registry::{DashboardServiceControl, ServiceShutdownOutcome};
use rozsa_app::dev_flow::{
    CommandExecutionError, CommandOutput, DashboardServiceFactory, DevFlowAvailability,
    DevFlowProjectKey, DevFlowServiceHandle, DiscoveryCommandRunner, DiscoveryEnvironment,
    ProjectCommandRunner,
};
use rozsa_app::settings::DevFlowSettings;
use rozsa_gui::dev_flow::{
    CLI_ERROR_ID, DASHBOARD_START_PREFIX, DashboardFactoryProvider, DevFlowRuntime,
    NotificationSink, SharedNotificationSink, SharedSidebarRefreshSink, SidebarRefreshSink,
    system_runtime,
};
use rozsa_gui::notifications::AppNotificationEvent;

#[derive(Clone, Debug)]
enum FakeRevision {
    Named { branch: String, oid: String },
    Unborn { branch: String },
    Detached { oid: String },
}

#[derive(Clone, Debug)]
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
struct FakeRunnerState {
    git_projects: HashMap<PathBuf, FakeRevision>,
    statuses: HashMap<PathBuf, FakeStatus>,
}

#[derive(Clone, Default)]
struct FakeRunner {
    state: Arc<StdMutex<FakeRunnerState>>,
}

impl FakeRunner {
    fn add_git_project(&self, root: &Path, branch: &str, oid: &str) {
        let root = std::fs::canonicalize(root).unwrap();
        let mut state = self.state.lock().unwrap();
        state.git_projects.insert(
            root.clone(),
            FakeRevision::Named {
                branch: branch.to_owned(),
                oid: oid.to_owned(),
            },
        );
        state.statuses.entry(root).or_default();
    }

    fn set_branch(&self, root: &Path, branch: &str, oid: &str) {
        let root = std::fs::canonicalize(root).unwrap();
        self.state.lock().unwrap().git_projects.insert(
            root,
            FakeRevision::Named {
                branch: branch.to_owned(),
                oid: oid.to_owned(),
            },
        );
    }

    fn set_status(&self, root: &Path, status: FakeStatus) {
        let root = std::fs::canonicalize(root).unwrap();
        self.state.lock().unwrap().statuses.insert(root, status);
    }
}

#[async_trait]
impl ProjectCommandRunner for FakeRunner {
    async fn run(
        &self,
        cwd: &Path,
        executable: &Path,
        args: &[&str],
        _deadline: Duration,
    ) -> Result<CommandOutput, CommandExecutionError> {
        let cwd = std::fs::canonicalize(cwd)
            .map_err(|error| CommandExecutionError::Launch(error.to_string()))?;
        let state = self.state.lock().unwrap();
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

#[derive(Clone, Default)]
struct FakeDiscoveryRunner {
    found: Arc<StdMutex<Option<String>>>,
}

impl FakeDiscoveryRunner {
    fn set_found(&self, stdout: &str) {
        *self.found.lock().unwrap() = Some(stdout.to_owned());
    }
}

#[async_trait]
impl DiscoveryCommandRunner for FakeDiscoveryRunner {
    async fn run(
        &self,
        _executable: &Path,
        args: &[&str],
        _deadline: Duration,
    ) -> Result<CommandOutput, CommandExecutionError> {
        if args == ["--version"] {
            let found = self.found.lock().unwrap().clone();
            return match found {
                Some(stdout) => Ok(success(&stdout)),
                None => Ok(failure("dow: command not found")),
            };
        }
        Ok(failure("unexpected discovery command"))
    }
}

#[derive(Default)]
struct FakeControlState {
    shutdown_calls: Vec<Duration>,
    alive: bool,
}

#[derive(Clone, Default)]
struct FakeControl {
    state: Arc<StdMutex<FakeControlState>>,
}

#[async_trait]
impl DashboardServiceControl for FakeControl {
    async fn shutdown(&self, grace: Duration) -> ServiceShutdownOutcome {
        self.state.lock().unwrap().shutdown_calls.push(grace);
        ServiceShutdownOutcome::Exited
    }

    async fn is_alive(&self) -> bool {
        self.state.lock().unwrap().alive
    }
}

#[derive(Default)]
struct FakeFactoryState {
    starts: Vec<DevFlowProjectKey>,
    controls: Vec<FakeControl>,
    snapshots: Vec<DevFlowSnapshot>,
}

#[derive(Clone, Default)]
struct FakeFactory {
    state: Arc<StdMutex<FakeFactoryState>>,
}

impl FakeFactory {
    fn starts(&self) -> Vec<DevFlowProjectKey> {
        self.state.lock().unwrap().starts.clone()
    }

    fn shutdown_calls(&self) -> Vec<Duration> {
        self.state
            .lock()
            .unwrap()
            .controls
            .iter()
            .flat_map(|control| control.state.lock().unwrap().shutdown_calls.clone())
            .collect()
    }
}

fn snapshot() -> DevFlowSnapshot {
    DevFlowSnapshot {
        revision: 1,
        project: rozsa_app::dev_flow::DevFlowProjectStatus {
            name: None,
            phase: None,
            mode: None,
            version: None,
            goals_minor: None,
            updated: None,
        },
        tasks: Vec::new(),
        issues: Vec::new(),
        received_at: UNIX_EPOCH,
        stale: false,
    }
}

#[async_trait]
impl DashboardServiceFactory for FakeFactory {
    async fn start(&self, project: &DevFlowProjectKey) -> Result<DevFlowServiceHandle, String> {
        let control = FakeControl::default();
        let mut state = self.state.lock().unwrap();
        state.starts.push(project.clone());
        state.controls.push(control.clone());
        let snapshot = Arc::new(tokio::sync::RwLock::new(Some(snapshot())));
        Ok(DevFlowServiceHandle::with_child(
            state.starts.len() as u64,
            snapshot,
            Arc::new(control),
            None,
        ))
    }
}

fn environment() -> (DiscoveryEnvironment, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("dow"), "fake dow").unwrap();
    let mut path =
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).collect::<Vec<_>>();
    path.insert(0, dir.path().to_path_buf());
    (
        DiscoveryEnvironment {
            path: std::env::join_paths(path).unwrap(),
            home_dir: None,
            cargo_home: None,
            npm_config_prefix: None,
            app_data: None,
            homebrew_bin_dirs: vec![],
        },
        dir,
    )
}

fn notifier() -> (
    SharedNotificationSink,
    tokio::sync::mpsc::UnboundedReceiver<AppNotificationEvent>,
) {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let sink: NotificationSink = Arc::new(move |event| {
        let _ = tx.send(event);
    });
    (Arc::new(StdMutex::new(Some(sink))), rx)
}

fn no_notifier() -> SharedNotificationSink {
    Arc::new(StdMutex::new(None))
}

fn write_status(root: &Path, branch: &str) {
    let directory = root.join(".dev-doc").join(branch);
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("STATUS.yaml"), "phase: DEV\n").unwrap();
}

#[tokio::test]
async fn system_runtime_and_late_attachment_share_one_sidebar_refresh_slot() {
    let refreshes = Arc::new(AtomicUsize::new(0));
    let shared: SharedSidebarRefreshSink = Arc::new(StdMutex::new(None));
    let runtime = system_runtime(
        no_notifier(),
        Arc::new(FakeRunner::default()),
        shared.clone(),
    );
    let refreshes_for_sink = refreshes.clone();
    let sink: SidebarRefreshSink = Arc::new(move || {
        refreshes_for_sink.fetch_add(1, Ordering::SeqCst);
    });

    runtime.attach_sidebar_refresh(sink);
    shared.lock().unwrap().clone().expect("attached refresh")();

    assert_eq!(refreshes.load(Ordering::SeqCst), 1);
    runtime.shutdown();
}

#[tokio::test]
async fn session_start_and_finish_track_active_state_and_exact_stop_time() {
    let project = tempfile::tempdir().unwrap();
    write_status(project.path(), "main");
    let runner = Arc::new(FakeRunner::default());
    runner.add_git_project(project.path(), "main", &"a".repeat(40));
    let discovery = FakeDiscoveryRunner::default();
    discovery.set_found("dow 1.2.3");
    let (environment, _dow_dir) = environment();
    let factory = FakeFactory::default();
    let runtime = DevFlowRuntime::new(no_notifier(), runner, Arc::new(discovery), environment, {
        let factory = factory.clone();
        Arc::new(move |_executable| Arc::new(factory.clone()))
    });
    let settings = DevFlowSettings::default();
    runtime.reconfigure(&settings).await.unwrap();

    runtime
        .session_started("s1", project.path().to_path_buf())
        .await;
    assert_eq!(runtime.active_sessions().await, 1);
    let state = runtime.session_state("s1").await.unwrap();
    assert_eq!(state.availability, DevFlowAvailability::Ready);
    assert_eq!(factory.starts().len(), 1);

    let stopped_at = UNIX_EPOCH + Duration::from_secs(5_000_000);
    runtime.session_finished("s1", stopped_at).await;
    assert_eq!(runtime.active_sessions().await, 0);
    assert_eq!(runtime.last_stop_at("s1").await, Some(stopped_at));

    // Resolution of a waiting permission keeps the session active.
    runtime
        .session_started("s1", project.path().to_path_buf())
        .await;
    runtime.session_resumed("s1").await;
    assert_eq!(runtime.active_sessions().await, 1);
    runtime.session_finished("s1", SystemTime::now()).await;
    runtime.session_closed("s1").await;
    assert!(runtime.session_state("s1").await.is_none());
    assert!(runtime.last_stop_at("s1").await.is_none());
}

#[tokio::test]
async fn disable_stops_owned_children_and_reenable_restarts_services() {
    let project = tempfile::tempdir().unwrap();
    write_status(project.path(), "main");
    let runner = Arc::new(FakeRunner::default());
    runner.add_git_project(project.path(), "main", &"b".repeat(40));
    let discovery = FakeDiscoveryRunner::default();
    discovery.set_found("dow 1.2.3");
    let (environment, _dow_dir) = environment();
    let factory = FakeFactory::default();
    let runtime = DevFlowRuntime::new(no_notifier(), runner, Arc::new(discovery), environment, {
        let factory = factory.clone();
        Arc::new(move |_executable| Arc::new(factory.clone()))
    });

    let mut settings = DevFlowSettings::default();
    runtime.reconfigure(&settings).await.unwrap();
    runtime
        .session_started("s1", project.path().to_path_buf())
        .await;
    assert_eq!(factory.starts().len(), 1);

    settings.enabled = false;
    runtime.reconfigure(&settings).await.unwrap();
    let disabled_diagnostics = runtime.diagnostics(&settings).await;
    assert!(disabled_diagnostics.cli.available);
    assert_eq!(disabled_diagnostics.cli.version.as_deref(), Some("1.2.3"));
    assert_eq!(
        factory.starts().len(),
        1,
        "diagnostics must not start a service"
    );
    assert_eq!(factory.shutdown_calls().len(), 1);
    assert!(runtime.session_state("s1").await.is_none());
    assert_eq!(runtime.active_sessions().await, 0);

    settings.enabled = true;
    runtime.reconfigure(&settings).await.unwrap();
    assert_eq!(factory.starts().len(), 2);
    assert_eq!(runtime.active_sessions().await, 1);
    assert_eq!(
        runtime.session_state("s1").await.unwrap().availability,
        DevFlowAvailability::Ready
    );

    let stopped_at = UNIX_EPOCH + Duration::from_secs(5_100_000);
    runtime.session_finished("s1", stopped_at).await;
    settings.enabled = false;
    runtime.reconfigure(&settings).await.unwrap();
    settings.enabled = true;
    runtime.reconfigure(&settings).await.unwrap();
    assert_eq!(runtime.active_sessions().await, 0);
    assert_eq!(runtime.last_stop_at("s1").await, Some(stopped_at));
}

#[tokio::test]
async fn invalid_custom_path_fails_without_falling_back() {
    let project = tempfile::tempdir().unwrap();
    write_status(project.path(), "main");
    let runner = Arc::new(FakeRunner::default());
    runner.add_git_project(project.path(), "main", &"c".repeat(40));
    // PATH discovery would succeed if the runtime fell back.
    let discovery = FakeDiscoveryRunner::default();
    discovery.set_found("dow 1.2.3");
    let (environment, _dow_dir) = environment();
    let factory = FakeFactory::default();
    let runtime = DevFlowRuntime::new(no_notifier(), runner, Arc::new(discovery), environment, {
        let factory = factory.clone();
        Arc::new(move |_executable| Arc::new(factory.clone()))
    });

    let mut settings = DevFlowSettings {
        enabled: true,
        show_sidebar_status: true,
        show_dashboard_button: true,
        executable_path: Some(PathBuf::from("/missing/dow-binary")),
    };
    runtime.reconfigure(&settings).await.unwrap();
    let diagnostics = runtime.diagnostics(&settings).await;
    assert!(!diagnostics.cli.available);
    assert!(diagnostics.cli.error.is_some());
    assert!(
        diagnostics
            .cli
            .error
            .unwrap()
            .contains("missing/dow-binary")
    );

    runtime
        .session_started("s1", project.path().to_path_buf())
        .await;
    assert!(
        factory.starts().is_empty(),
        "no fallback to automatic discovery"
    );
    assert_eq!(runtime.active_sessions().await, 0);

    // Selecting Auto again restores automatic discovery.
    settings.executable_path = None;
    runtime.reconfigure(&settings).await.unwrap();
    let diagnostics = runtime.diagnostics(&settings).await;
    assert!(diagnostics.cli.available);
    assert!(diagnostics.cli.error.is_none());
}

#[tokio::test]
async fn branch_change_rescan_and_switch_reassociate_sessions() {
    let project = tempfile::tempdir().unwrap();
    write_status(project.path(), "main");
    let runner = Arc::new(FakeRunner::default());
    runner.add_git_project(project.path(), "main", &"d".repeat(40));
    let discovery = FakeDiscoveryRunner::default();
    discovery.set_found("dow 1.2.3");
    let (environment, _dow_dir) = environment();
    let factory = FakeFactory::default();
    let runtime = DevFlowRuntime::new(
        no_notifier(),
        runner.clone(),
        Arc::new(discovery),
        environment,
        {
            let factory = factory.clone();
            Arc::new(move |_executable| Arc::new(factory.clone()))
        },
    );
    let settings = DevFlowSettings::default();
    runtime.reconfigure(&settings).await.unwrap();
    runtime
        .session_started("s1", project.path().to_path_buf())
        .await;
    runtime
        .session_started("s2", project.path().to_path_buf())
        .await;
    runtime
        .switch_to_session("s1", project.path().to_path_buf())
        .await;
    assert_eq!(runtime.active_sessions().await, 2);
    assert_eq!(
        runtime.session_state("s1").await.unwrap().project.revision,
        rozsa_app::dev_flow::DevFlowRevisionKey::NamedBranch("main".to_owned())
    );

    // A branch change plus a successful Bash completion re-evaluates every
    // session associated with the worktree.
    write_status(project.path(), "feature");
    runner.set_branch(project.path(), "feature", &"e".repeat(40));
    runtime
        .on_successful_bash("s1", project.path().to_path_buf())
        .await;
    assert_eq!(
        runtime.session_state("s1").await.unwrap().project.revision,
        rozsa_app::dev_flow::DevFlowRevisionKey::NamedBranch("feature".to_owned())
    );
    assert_eq!(
        runtime.session_state("s2").await.unwrap().project.revision,
        rozsa_app::dev_flow::DevFlowRevisionKey::NamedBranch("feature".to_owned())
    );
    assert_eq!(runtime.active_sessions().await, 2);

    // The two-second selected-project probe detects the same change.
    write_status(project.path(), "beta");
    runner.set_branch(project.path(), "beta", &"f".repeat(40));
    runtime.probe_once().await;
    assert_eq!(
        runtime.session_state("s1").await.unwrap().project.revision,
        rozsa_app::dev_flow::DevFlowRevisionKey::NamedBranch("beta".to_owned())
    );
    assert_eq!(runtime.active_sessions().await, 2);

    let stopped_at = UNIX_EPOCH + Duration::from_secs(5_200_000);
    runtime.session_finished("s2", stopped_at).await;
    runtime.probe_once().await;
    assert_eq!(runtime.active_sessions().await, 1);
    assert_eq!(runtime.last_stop_at("s2").await, Some(stopped_at));

    // Session switching re-evaluates identity and protects the new project.
    let other = tempfile::tempdir().unwrap();
    write_status(other.path(), "main");
    runner.add_git_project(other.path(), "main", &"g".repeat(40));
    runtime
        .switch_to_session("s1", other.path().to_path_buf())
        .await;
    assert_eq!(
        runtime.session_state("s1").await.unwrap().project.root,
        std::fs::canonicalize(other.path()).unwrap()
    );
}

#[tokio::test]
async fn late_init_becomes_ready_via_probe() {
    let project = tempfile::tempdir().unwrap();
    let runner = Arc::new(FakeRunner::default());
    runner.add_git_project(project.path(), "main", &"h".repeat(40));
    let discovery = FakeDiscoveryRunner::default();
    discovery.set_found("dow 1.2.3");
    let (environment, _dow_dir) = environment();
    let factory = FakeFactory::default();
    let runtime = DevFlowRuntime::new(
        no_notifier(),
        runner.clone(),
        Arc::new(discovery),
        environment,
        {
            let factory = factory.clone();
            Arc::new(move |_executable| Arc::new(factory.clone()))
        },
    );
    let settings = DevFlowSettings::default();
    runtime.reconfigure(&settings).await.unwrap();
    runtime
        .session_started("s1", project.path().to_path_buf())
        .await;
    runtime
        .switch_to_session("s1", project.path().to_path_buf())
        .await;
    assert_eq!(
        runtime.session_state("s1").await.unwrap().availability,
        DevFlowAvailability::ProjectNotInitialized
    );
    assert!(factory.starts().is_empty());

    write_status(project.path(), "main");
    runtime.probe_once().await;
    assert_eq!(
        runtime.session_state("s1").await.unwrap().availability,
        DevFlowAvailability::Ready
    );
    assert_eq!(factory.starts().len(), 1);
}

#[tokio::test]
async fn cli_error_notification_resolves_on_recovery() {
    let (notifier, mut rx) = notifier();

    let project = tempfile::tempdir().unwrap();
    write_status(project.path(), "main");
    let runner = Arc::new(FakeRunner::default());
    runner.add_git_project(project.path(), "main", &"i".repeat(40));
    let discovery = FakeDiscoveryRunner::default();
    let (environment, _dow_dir) = environment();
    let factory = FakeFactory::default();
    let runtime = DevFlowRuntime::new(
        notifier,
        runner,
        Arc::new(discovery.clone()),
        environment,
        {
            let factory = factory.clone();
            Arc::new(move |_executable| Arc::new(factory.clone()))
        },
    );
    let settings = DevFlowSettings::default();
    runtime.reconfigure(&settings).await.unwrap();

    let event = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        event,
        AppNotificationEvent::Upsert { ref id, severity, .. }
            if id == CLI_ERROR_ID && severity == rozsa_gui::notifications::NotificationSeverity::Error
    ));

    discovery.set_found("dow 1.2.3");
    runtime.reconfigure(&settings).await.unwrap();
    let event = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        event,
        AppNotificationEvent::Resolve {
            id: CLI_ERROR_ID.to_string()
        }
    );
}

#[tokio::test]
async fn routine_ready_emits_no_extra_notification() {
    let (notifier, mut rx) = notifier();

    let project = tempfile::tempdir().unwrap();
    write_status(project.path(), "main");
    let runner = Arc::new(FakeRunner::default());
    runner.add_git_project(project.path(), "main", &"j".repeat(40));
    let discovery = FakeDiscoveryRunner::default();
    discovery.set_found("dow 1.2.3");
    let (environment, _dow_dir) = environment();
    let factory = FakeFactory::default();
    let runtime = DevFlowRuntime::new(notifier, runner, Arc::new(discovery), environment, {
        let factory = factory.clone();
        Arc::new(move |_executable| Arc::new(factory.clone()))
    });
    let settings = DevFlowSettings::default();
    runtime.reconfigure(&settings).await.unwrap();
    runtime
        .session_started("s1", project.path().to_path_buf())
        .await;
    runtime
        .session_started("s2", project.path().to_path_buf())
        .await;
    runtime.session_finished("s1", SystemTime::now()).await;

    assert!(
        tokio::time::timeout(Duration::from_millis(200), rx.recv())
            .await
            .is_err(),
        "routine Ready must stay silent"
    );
    assert!(
        factory.starts().len() >= 1,
        "services started without notifications"
    );
}

#[tokio::test]
async fn dashboard_start_failure_uses_per_project_error_id() {
    let (notifier, mut rx) = notifier();

    let project = tempfile::tempdir().unwrap();
    write_status(project.path(), "main");
    let runner = Arc::new(FakeRunner::default());
    runner.add_git_project(project.path(), "main", &"k".repeat(40));
    let discovery = FakeDiscoveryRunner::default();
    discovery.set_found("dow 1.2.3");
    let (environment, _dow_dir) = environment();
    let factory = FakeFactory::default();
    let failing_factory: DashboardFactoryProvider =
        Arc::new(move |_executable| Arc::new(FailingFactory));
    let runtime = DevFlowRuntime::new(
        notifier,
        runner,
        Arc::new(discovery),
        environment,
        failing_factory,
    );
    let settings = DevFlowSettings::default();
    runtime.reconfigure(&settings).await.unwrap();
    runtime
        .session_started("s1", project.path().to_path_buf())
        .await;

    let event = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        event,
        AppNotificationEvent::Upsert { ref id, severity, .. }
            if id.starts_with(DASHBOARD_START_PREFIX)
                && severity == rozsa_gui::notifications::NotificationSeverity::Error
    ));
    let _ = factory;
}

#[tokio::test]
async fn repeated_project_failure_updates_the_same_notification_id() {
    let (notifier, mut rx) = notifier();
    let project = tempfile::tempdir().unwrap();
    write_status(project.path(), "main");
    let runner = Arc::new(FakeRunner::default());
    runner.add_git_project(project.path(), "main", &"m".repeat(40));
    let discovery = FakeDiscoveryRunner::default();
    discovery.set_found("dow 1.2.3");
    let (environment, _dow_dir) = environment();
    let factory = UpdatingFailingFactory::default();
    let provider: DashboardFactoryProvider = {
        let factory = factory.clone();
        Arc::new(move |_executable| Arc::new(factory.clone()))
    };
    let runtime = DevFlowRuntime::new(notifier, runner, Arc::new(discovery), environment, provider);
    let settings = DevFlowSettings::default();
    runtime.reconfigure(&settings).await.unwrap();
    runtime
        .session_started("s1", project.path().to_path_buf())
        .await;
    let first = rx.recv().await.expect("first failure notification");

    runtime.reconfigure(&settings).await.unwrap();
    let second = loop {
        let event = rx.recv().await.expect("updated failure notification");
        if matches!(event, AppNotificationEvent::Upsert { .. }) {
            break event;
        }
    };

    let unpack = |event| match event {
        AppNotificationEvent::Upsert { id, message, .. } => (id, message),
        other => panic!("expected upsert, got {other:?}"),
    };
    let (first_id, first_message) = unpack(first);
    let (second_id, second_message) = unpack(second);
    assert_eq!(first_id, second_id, "the project owns one stable error ID");
    assert_ne!(
        first_message, second_message,
        "new failure detail is emitted"
    );
}

struct FailingFactory;

#[async_trait]
impl DashboardServiceFactory for FailingFactory {
    async fn start(&self, _project: &DevFlowProjectKey) -> Result<DevFlowServiceHandle, String> {
        Err("boom".to_string())
    }
}

#[derive(Clone, Default)]
struct UpdatingFailingFactory {
    attempts: Arc<AtomicUsize>,
}

#[async_trait]
impl DashboardServiceFactory for UpdatingFailingFactory {
    async fn start(&self, _project: &DevFlowProjectKey) -> Result<DevFlowServiceHandle, String> {
        let attempt = self.attempts.fetch_add(1, Ordering::SeqCst) + 1;
        Err(format!("failure attempt {attempt}"))
    }
}

use rozsa_app::dev_flow::DevFlowSnapshot;
