// FrameworkTree
// dev_flow_sidebar_test.rs
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
// ├── set_snapshot()
// ├── starts()
// ├── empty_snapshot()
// ├── task()
// ├── issue()
// ├── impl FakeFactory
// ├── start()
// ├── environment()
// ├── no_notifier()
// ├── write_status()
// ├── run_sidebar_behavior_harness()
// ├── runtime_with()
// ├── ready_runtime()
// ├── sidebar_snapshot_counts_open_work_and_claimed_rows_once()
// ├── empty_ready_project_shows_zero_counts_and_stale_flag()
// ├── settings_diagnostics_report_live_dashboard_and_snapshot_metadata()
// ├── uninitialized_project_reports_not_initialized_without_counts()
// ├── detail_rejects_stale_wrong_project_and_invalid_targets()
// ├── dashboard_url_returns_loopback_only_when_ready()
// ├── dashboard_url_fails_explicitly_for_uninitialized_projects()
// ├── loopback_url_validation_is_restricted()
// ├── short_ids_normalize_canonical_task_and_issue_ids()
// ├── sidebar_row_budget_adapts_without_sacrificing_sessions_space()
// └── sidebar_and_detail_static_contracts()

//! Dev-flow sidebar tests: open/claimed summary counts, read-only detail
//! validation, loopback dashboard opening, and the responsive sidebar/overlay
//! contracts for TASK-T007.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, UNIX_EPOCH};

use async_trait::async_trait;
use rozsa_app::dev_flow::registry::{DashboardServiceControl, ServiceShutdownOutcome};
use rozsa_app::dev_flow::{
    CommandExecutionError, CommandOutput, DashboardServiceFactory, DevFlowAvailability,
    DevFlowIssue, DevFlowIssueStatus, DevFlowProjectKey, DevFlowServiceHandle, DevFlowSnapshot,
    DevFlowTask, DevFlowTaskStatus, DiscoveryCommandRunner, DiscoveryEnvironment,
    ProjectCommandRunner,
};
use rozsa_app::settings::DevFlowSettings;
use rozsa_gui::dev_flow::{
    DashboardFactoryProvider, DevFlowDetailRequest, DevFlowDetailTarget, DevFlowRuntime,
    SharedNotificationSink, short_dev_flow_id, validate_loopback_url,
};

#[derive(Clone, Debug)]
enum FakeRevision {
    Named { branch: String, oid: String },
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
                    FakeRevision::Named { branch, .. } => success(&branch),
                },
                ["rev-parse", "--verify", "HEAD"] => match revision {
                    FakeRevision::Named { oid, .. } => success(&oid),
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
    alive: bool,
}

#[derive(Clone, Default)]
struct FakeControl {
    state: Arc<StdMutex<FakeControlState>>,
}

#[async_trait]
impl DashboardServiceControl for FakeControl {
    async fn shutdown(&self, _grace: Duration) -> ServiceShutdownOutcome {
        ServiceShutdownOutcome::Exited
    }

    async fn is_alive(&self) -> bool {
        self.state.lock().unwrap().alive
    }
}

#[derive(Default)]
struct FakeFactoryState {
    starts: usize,
    snapshots: HashMap<PathBuf, DevFlowSnapshot>,
    base_urls: HashMap<PathBuf, String>,
}

#[derive(Clone, Default)]
struct FakeFactory {
    state: Arc<StdMutex<FakeFactoryState>>,
}

impl FakeFactory {
    fn set_snapshot(&self, root: &Path, snapshot: DevFlowSnapshot, url: &str) {
        let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        let mut state = self.state.lock().unwrap();
        state.snapshots.insert(root.clone(), snapshot);
        state.base_urls.insert(root, url.to_owned());
    }

    fn starts(&self) -> usize {
        self.state.lock().unwrap().starts
    }
}

fn empty_snapshot(revision: u64) -> DevFlowSnapshot {
    DevFlowSnapshot {
        revision,
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

fn task(id: &str, title: &str, status: DevFlowTaskStatus) -> DevFlowTask {
    DevFlowTask {
        id: id.to_owned(),
        title: title.to_owned(),
        status,
        priority: Some("P1".to_owned()),
        complexity: Some("M".to_owned()),
        task_type: Some("feat".to_owned()),
        refs: Some("SPEC-AC-001".to_owned()),
        depends_on: vec!["TASK-T001".to_owned()],
        done_when: vec!["acceptance passes".to_owned()],
        files_create: vec!["src/new.rs".to_owned()],
        files_modify: vec!["src/lib.rs".to_owned()],
        files_test: vec!["tests/lib_test.rs".to_owned()],
    }
}

fn issue(id: &str, title: &str, status: DevFlowIssueStatus) -> DevFlowIssue {
    DevFlowIssue {
        id: id.to_owned(),
        title: title.to_owned(),
        status,
        severity: Some("P1".to_owned()),
        description: Some("issue description".to_owned()),
        files_create: vec!["docs/issue.md".to_owned()],
        files_modify: vec!["src/issue.rs".to_owned()],
    }
}

#[async_trait]
impl DashboardServiceFactory for FakeFactory {
    async fn start(&self, project: &DevFlowProjectKey) -> Result<DevFlowServiceHandle, String> {
        let control = FakeControl::default();
        let mut state = self.state.lock().unwrap();
        state.starts += 1;
        let root = project.root.clone();
        let snapshot = state
            .snapshots
            .get(&root)
            .cloned()
            .unwrap_or_else(|| empty_snapshot(1));
        let snapshot = Arc::new(tokio::sync::RwLock::new(Some(snapshot)));
        let url = state
            .base_urls
            .get(&root)
            .cloned()
            .unwrap_or_else(|| "http://127.0.0.1:9800/".to_owned());
        let url = reqwest::Url::parse(&url).expect("fake dashboard url");
        Ok(DevFlowServiceHandle::with_base_url(
            state.starts as u64,
            snapshot,
            Arc::new(control),
            None,
            Some(url),
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

fn no_notifier() -> SharedNotificationSink {
    Arc::new(StdMutex::new(None))
}

fn write_status(root: &Path, branch: &str) {
    let directory = root.join(".dev-doc").join(branch);
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("STATUS.yaml"), "phase: DEV\n").unwrap();
}

fn run_sidebar_behavior_harness(assertions: &str) {
    let source = include_str!("../frontend/sidebar.js");
    let start = source.find("function renderDevFlowClaimedRows()").unwrap();
    let end = source[start..]
        .find("function applySidebarThemeState(snapshot)")
        .unwrap()
        + start;
    let harness = r#"
class FakeElement {
  constructor(id = '') {
    this.id = id; this.children = []; this.hidden = false; this.textContent = '';
    this.title = ''; this.className = ''; this.style = {}; this.onclick = null;
    this.onpointerenter = null; this.onpointerleave = null;
    this.attributes = new Map(); this.observed = false;
  }
  append(...children) { this.children.push(...children); }
  appendChild(child) { this.children.push(child); return child; }
  replaceChildren(...children) { this.children = children; }
  setAttribute(name, value) { this.attributes.set(name, String(value)); }
  getBoundingClientRect() {
    if (this.id === 'sidebarStatusSection') {
      return { height: 100 + claimed.children.length * rowHeight + (more.hidden ? 0 : rowHeight) };
    }
    if (this.id === 'sidebarDevFlowClaimed') return { height: claimed.children.length * rowHeight };
    if (this.id === 'sidebarDevFlowMore') return { height: rowHeight };
    if (this.id === 'sidebarBottom') return { height: 40 };
    if (this.className === 'dev-flow-claimed-row') return { height: rowHeight };
    return { height: 0 };
  }
}
let rowHeight = 20;
let createdProbes = 0;
const claimed = new FakeElement('sidebarDevFlowClaimed');
const more = new FakeElement('sidebarDevFlowMore'); more.hidden = true;
const status = new FakeElement('sidebarStatusSection');
const main = new FakeElement('mainSidebarScene');
const bottom = new FakeElement('sidebarBottom');
const dashboard = new FakeElement('sidebarDevFlowDashboard');
const ids = new Map([
  ['sidebarDevFlowClaimed', claimed], ['sidebarDevFlowMore', more],
  ['sidebarStatusSection', status], ['mainSidebarScene', main],
  ['sidebarDevFlowDashboard', dashboard],
]);
const body = { classList: { contains: () => false } };
const document = {
  body,
  fonts: { addEventListener: () => {} },
  getElementById: id => ids.get(id) || null,
  querySelector: selector => selector === '.sidebar-bottom' ? bottom : null,
  createElement: tag => {
    const element = new FakeElement();
    if (tag === 'button') createdProbes += 1;
    return element;
  },
};
const window = { innerHeight: 300 };
let nextTimer = 1;
const delayedCallbacks = new Map();
function setTimeout(callback, delay) {
  const id = nextTimer++;
  delayedCallbacks.set(id, { callback, delay });
  return id;
}
function clearTimeout(id) { delayedCallbacks.delete(id); }
function runDelayed(milliseconds) {
  const due = Array.from(delayedCallbacks.entries())
    .filter(([, timer]) => timer.delay <= milliseconds);
  due.forEach(([id, timer]) => { delayedCallbacks.delete(id); timer.callback(); });
}
class ResizeObserver { constructor(callback) { this.callback = callback; } observe(element) { element.observed = true; } }
const sidebarDevFlowResizeObserver = new ResizeObserver(() => invalidateDevFlowRowMetrics());
let sidebarDevFlow = null;
let sidebarLastDevFlowLayoutKey = '';
let sidebarDevFlowRowHeight = 0;
let sidebarDevFlowMeasureProbe = null;
let sidebarDevFlowResizePending = false;
const SIDEBAR_SESSIONS_MIN_PX = 96;
function requestAnimationFrame(callback) { callback(); }
const sidebarInvocations = [];
const sidebarInvoke = async (name, args) => { sidebarInvocations.push({ name, args }); };
const sidebarEvents = [];
const sidebarEmit = async name => { sidebarEvents.push(name); };
const DEV_FLOW_HOVER_DELAY_MS = 180;
let sidebarDevFlowHoverTimer = null;
let sidebarDevFlowHoverElement = null;
let sidebarDevFlowDetailPinned = false;
function check(condition, message) { if (!condition) throw new Error(message); }
function items(count) {
  return Array.from({ length: count }, (_, index) => ({
    id: 'TASK-T' + String(index + 1).padStart(3, '0'),
    shortId: 'T' + String(index + 1).padStart(3, '0'),
    title: 'A very long claimed task title ' + index,
  }));
}
"#;
    let mut script = String::with_capacity(harness.len() + end - start + assertions.len());
    script.push_str(harness);
    script.push_str(&source[start..end]);
    script.push_str(assertions);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("sidebar_behavior_test.js");
    std::fs::write(&path, script).unwrap();
    let output = Command::new("node")
        .arg(&path)
        .output()
        .expect("Node.js is required for frontend behavior tests");
    assert!(
        output.status.success(),
        "sidebar behavior harness failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn runtime_with(project: &Path, factory: &FakeFactory) -> Arc<DevFlowRuntime> {
    let runner = Arc::new(FakeRunner::default());
    runner.add_git_project(project, "main", &"a".repeat(40));
    let discovery = FakeDiscoveryRunner::default();
    discovery.set_found("dow 1.2.3");
    let (environment, _dow_dir) = environment();
    DevFlowRuntime::new(no_notifier(), runner, Arc::new(discovery), environment, {
        let factory = factory.clone();
        let provider: DashboardFactoryProvider =
            Arc::new(move |_executable| Arc::new(factory.clone()));
        provider
    })
}

async fn ready_runtime(
    project: &Path,
    factory: &FakeFactory,
) -> (Arc<DevFlowRuntime>, DevFlowProjectKey) {
    write_status(project, "main");
    let runtime = runtime_with(project, factory);
    let settings = DevFlowSettings::default();
    runtime.reconfigure(&settings).await.unwrap();
    runtime.session_started("s1", project.to_path_buf()).await;
    let state = runtime.session_state("s1").await.unwrap();
    assert_eq!(state.availability, DevFlowAvailability::Ready);
    (runtime, state.project)
}

#[tokio::test]
async fn sidebar_snapshot_counts_open_work_and_claimed_rows_once() {
    let project = tempfile::tempdir().unwrap();
    let factory = FakeFactory::default();
    let mut snapshot = empty_snapshot(7);
    snapshot.tasks = vec![
        task("TASK-T001", "pending task", DevFlowTaskStatus::Pending),
        task("TASK-T002", "claimed task", DevFlowTaskStatus::InProgress),
        task("TASK-T003", "done task", DevFlowTaskStatus::Done),
    ];
    snapshot.issues = vec![
        issue("ISSUE-I001", "open issue", DevFlowIssueStatus::Open),
        issue(
            "ISSUE-I002",
            "claimed issue",
            DevFlowIssueStatus::InProgress,
        ),
        issue("ISSUE-I003", "closed issue", DevFlowIssueStatus::Closed),
    ];
    factory.set_snapshot(project.path(), snapshot, "http://127.0.0.1:9801/");
    let (runtime, _key) = ready_runtime(project.path(), &factory).await;

    let sidebar = runtime
        .sidebar_snapshot("s1", true)
        .await
        .expect("sidebar snapshot");
    assert_eq!(sidebar.open_tasks, 2);
    assert_eq!(sidebar.open_issues, 2);
    assert_eq!(sidebar.revision, 7);
    assert!(!sidebar.stale);
    assert_eq!(sidebar.availability, "ready");
    assert!(sidebar.dashboard_ready);
    assert!(sidebar.show_sidebar_status);

    let ids = sidebar
        .claimed
        .iter()
        .map(|item| (item.kind.as_str(), item.short_id.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(
        ids,
        vec![("task", "T002"), ("issue", "I002")],
        "InProgress items appear once; done/closed are excluded"
    );
    assert_eq!(sidebar.claimed[0].id, "TASK-T002");
    assert_eq!(sidebar.claimed[0].title, "claimed task");
    assert_eq!(sidebar.claimed[1].id, "ISSUE-I002");
    assert_eq!(factory.starts(), 1);
}

#[tokio::test]
async fn empty_ready_project_shows_zero_counts_and_stale_flag() {
    let project = tempfile::tempdir().unwrap();
    let factory = FakeFactory::default();
    let mut snapshot = empty_snapshot(3);
    snapshot.stale = true;
    factory.set_snapshot(project.path(), snapshot, "http://127.0.0.1:9802/");
    let (runtime, _key) = ready_runtime(project.path(), &factory).await;

    let sidebar = runtime
        .sidebar_snapshot("s1", false)
        .await
        .expect("sidebar snapshot");
    assert_eq!((sidebar.open_tasks, sidebar.open_issues), (0, 0));
    assert!(sidebar.claimed.is_empty());
    assert!(sidebar.stale);
    assert_eq!(sidebar.availability, "ready");
    assert!(!sidebar.show_sidebar_status);
}

#[tokio::test]
async fn settings_diagnostics_report_live_dashboard_and_snapshot_metadata() {
    let project = tempfile::tempdir().unwrap();
    let factory = FakeFactory::default();
    factory.set_snapshot(project.path(), empty_snapshot(12), "http://127.0.0.1:9812/");
    let (runtime, _key) = ready_runtime(project.path(), &factory).await;
    runtime
        .switch_to_session("s1", project.path().to_path_buf())
        .await;

    let diagnostics = runtime.diagnostics(&DevFlowSettings::default()).await;
    let project = diagnostics.project.expect("active project diagnostics");
    assert_eq!(
        project.dashboard_url.as_deref(),
        Some("http://127.0.0.1:9812/")
    );
    assert_eq!(project.snapshot_revision, Some(12));
    assert_eq!(project.last_sync_unix_ms, Some(0));
    assert!(project.memory_use_bytes.is_some());
}

#[tokio::test]
async fn uninitialized_project_reports_not_initialized_without_counts() {
    let project = tempfile::tempdir().unwrap();
    let factory = FakeFactory::default();
    let runtime = runtime_with(project.path(), &factory);
    let settings = DevFlowSettings::default();
    runtime.reconfigure(&settings).await.unwrap();
    runtime
        .session_started("s1", project.path().to_path_buf())
        .await;

    let sidebar = runtime
        .sidebar_snapshot("s1", true)
        .await
        .expect("sidebar snapshot");
    assert_eq!(sidebar.availability, "not-initialized");
    assert_eq!((sidebar.open_tasks, sidebar.open_issues), (0, 0));
    assert!(sidebar.claimed.is_empty());
    assert!(!sidebar.dashboard_ready);
    assert_eq!(factory.starts(), 0);
}

#[tokio::test]
async fn detail_rejects_stale_wrong_project_and_invalid_targets() {
    let project = tempfile::tempdir().unwrap();
    let factory = FakeFactory::default();
    let mut snapshot = empty_snapshot(9);
    snapshot.tasks = vec![
        task("TASK-T010", "pending ten", DevFlowTaskStatus::Pending),
        task("TASK-T011", "claimed eleven", DevFlowTaskStatus::InProgress),
    ];
    snapshot.issues = vec![issue("ISSUE-I004", "open four", DevFlowIssueStatus::Open)];
    factory.set_snapshot(project.path(), snapshot, "http://127.0.0.1:9803/");
    let (runtime, _key) = ready_runtime(project.path(), &factory).await;

    let sidebar = runtime
        .sidebar_snapshot("s1", true)
        .await
        .expect("sidebar snapshot");

    // A valid request returns the read-only payload with focus support.
    let payload = runtime
        .detail(
            "s1",
            &DevFlowDetailRequest {
                project_key: sidebar.project.project_key.clone(),
                revision: sidebar.revision,
                target: DevFlowDetailTarget {
                    kind: "item".to_owned(),
                    id: Some("TASK-T011".to_owned()),
                },
            },
        )
        .await
        .expect("valid detail");
    assert_eq!(payload.revision, 9);
    assert_eq!(payload.open_tasks, 2);
    assert_eq!(payload.open_issues, 1);
    assert_eq!(payload.focus_id.as_deref(), Some("TASK-T011"));
    assert_eq!(payload.claimed_ids, vec!["TASK-T011"]);
    assert_eq!(payload.items.len(), 3);
    assert_eq!(payload.items[0].short_id, "T010");
    assert_eq!(payload.items[1].short_id, "T011");
    assert_eq!(payload.items[1].status, "in-progress");
    assert_eq!(payload.items[1].done_when, vec!["acceptance passes"]);
    assert_eq!(payload.items[1].files_create, vec!["src/new.rs"]);
    assert_eq!(payload.items[1].files_modify, vec!["src/lib.rs"]);
    assert_eq!(payload.items[1].files_test, vec!["tests/lib_test.rs"]);
    assert_eq!(payload.items[2].kind, "issue");
    assert_eq!(
        payload.items[2].description.as_deref(),
        Some("issue description")
    );
    assert_eq!(payload.items[2].files_create, vec!["docs/issue.md"]);
    assert_eq!(payload.items[2].files_modify, vec!["src/issue.rs"]);

    // Stale revision is rejected before the main view renders anything.
    let stale = runtime
        .detail(
            "s1",
            &DevFlowDetailRequest {
                project_key: sidebar.project.project_key.clone(),
                revision: sidebar.revision - 1,
                target: DevFlowDetailTarget {
                    kind: "summary".to_owned(),
                    id: None,
                },
            },
        )
        .await;
    assert!(stale.unwrap_err().contains("stale"));

    // A different project key is rejected.
    let wrong = runtime
        .detail(
            "s1",
            &DevFlowDetailRequest {
                project_key: "0000000000000000".to_owned(),
                revision: sidebar.revision,
                target: DevFlowDetailTarget {
                    kind: "summary".to_owned(),
                    id: None,
                },
            },
        )
        .await;
    assert!(wrong.unwrap_err().contains("different project"));

    // Unknown target kinds are rejected.
    let invalid = runtime
        .detail(
            "s1",
            &DevFlowDetailRequest {
                project_key: sidebar.project.project_key.clone(),
                revision: sidebar.revision,
                target: DevFlowDetailTarget {
                    kind: "mutate".to_owned(),
                    id: None,
                },
            },
        )
        .await;
    assert!(invalid.unwrap_err().contains("target is invalid"));

    // A syntactically valid focus must still identify open work from the same
    // validated snapshot.
    let missing = runtime
        .detail(
            "s1",
            &DevFlowDetailRequest {
                project_key: sidebar.project.project_key.clone(),
                revision: sidebar.revision,
                target: DevFlowDetailTarget {
                    kind: "item".to_owned(),
                    id: Some("TASK-T999".to_owned()),
                },
            },
        )
        .await;
    assert!(missing.unwrap_err().contains("does not exist"));
}

#[tokio::test]
async fn dashboard_url_returns_loopback_only_when_ready() {
    let project = tempfile::tempdir().unwrap();
    let factory = FakeFactory::default();
    factory.set_snapshot(project.path(), empty_snapshot(1), "http://127.0.0.1:9842/");
    let (runtime, _key) = ready_runtime(project.path(), &factory).await;

    let (url, project_key) = runtime
        .dashboard_url("s1", project.path().to_path_buf())
        .await
        .expect("dashboard url");
    assert!(url.starts_with("http://127.0.0.1:"), "{url}");
    assert!(url.ends_with('/'));
    assert_eq!(project_key.len(), 16);
    assert!(validate_loopback_url(&url));
    assert_eq!(factory.starts(), 1, "one click reuses the existing service");
}

#[tokio::test]
async fn dashboard_url_fails_explicitly_for_uninitialized_projects() {
    let project = tempfile::tempdir().unwrap();
    let factory = FakeFactory::default();
    let runtime = runtime_with(project.path(), &factory);
    let settings = DevFlowSettings::default();
    runtime.reconfigure(&settings).await.unwrap();
    runtime
        .session_started("s1", project.path().to_path_buf())
        .await;

    let error = runtime
        .dashboard_url("s1", project.path().to_path_buf())
        .await
        .expect_err("uninitialized project must fail");
    assert!(error.contains("not been initialized"), "{error}");
    assert_eq!(factory.starts(), 0);
}

#[test]
fn loopback_url_validation_is_restricted() {
    assert!(validate_loopback_url("http://127.0.0.1:9800/"));
    assert!(validate_loopback_url("http://localhost:9800/api/data"));
    assert!(validate_loopback_url("http://[::1]:9800/"));
    assert!(!validate_loopback_url("https://127.0.0.1:9800/"));
    assert!(!validate_loopback_url("http://127.0.0.1/"));
    assert!(!validate_loopback_url("http://example.com:9800/"));
    assert!(!validate_loopback_url("ftp://127.0.0.1:9800/"));
    assert!(!validate_loopback_url("http://127.0.0.1:notaport/"));
}

#[test]
fn short_ids_normalize_canonical_task_and_issue_ids() {
    assert_eq!(short_dev_flow_id("TASK-T007"), "T007");
    assert_eq!(short_dev_flow_id("ISSUE-I003"), "I003");
    assert_eq!(short_dev_flow_id("T001"), "T001");
}

#[test]
fn sidebar_row_budget_adapts_without_sacrificing_sessions_space() {
    run_sidebar_behavior_harness(
        r#"
sidebarDevFlow = {
  availability: 'ready',
  claimed: items(5),
  project: { projectKey: 'test-project' },
  revision: 7,
};
renderDevFlowClaimedRows();
check(claimed.children.length === 1, '300px height must leave 96px for Sessions and show one row');
check(!more.hidden && more.textContent === 'more 4', 'remaining rows must be summarized exactly');
check(claimed.children[0].children[2].textContent.includes('very long'), 'narrow-width title content must remain intact for CSS ellipsis');
claimed.children[0].onpointerenter();
check(sidebarInvocations.length === 0, 'hover must wait before requesting detail');
runDelayed(179);
check(sidebarInvocations.length === 0, 'hover duration must filter pointer sweeps');
runDelayed(180);
check(sidebarInvocations.length === 1 && sidebarInvocations[0].name === 'dev_flow_detail', 'sustained hover must request item detail');
claimed.children[0].onpointerleave();
check(sidebarEvents.includes('dev-flow-detail-dismiss'), 'hover preview must close when the pointer leaves');
claimed.children[0].onclick();
check(sidebarInvocations.length === 2, 'click must request detail immediately');
claimed.children[0].onpointerleave();
check(sidebarEvents.length === 1, 'click must pin detail across pointer leave');

window.innerHeight = 360;
invalidateDevFlowRowMetrics();
check(claimed.children.length === 5 && more.hidden, 'taller windows must consume all available row slots');

sidebarDevFlow.claimed = [];
sidebarLastDevFlowLayoutKey = '';
renderDevFlowClaimedRows();
check(claimed.children.length === 0 && more.hidden, 'zero claimed work must render no rows or more link');

sidebarDevFlow.claimed = items(5);
rowHeight = 30;
invalidateDevFlowRowMetrics();
check(claimed.children.length === 2 && more.textContent === 'more 3', 'font metric changes must invalidate the row-height cache');
invalidateDevFlowRowMetrics();
check(main.children.length === 1, 'stable resize must reuse one persistent measurement probe');

updateDevFlowDashboardButton(null);
check(dashboard.disabled && dashboard.title === 'Dev-flow is disabled', 'missing integration state must disable Dashboard');
for (const availability of ['starting', 'loading', 'error']) {
  updateDevFlowDashboardButton({ availability, availabilityMessage: availability + ' state' });
  check(dashboard.disabled && dashboard.title === availability + ' state', availability + ' must remain explicit');
}
updateDevFlowDashboardButton({ availability: 'ready' });
check(!dashboard.disabled && dashboard.title === 'Open dev-flow dashboard', 'ready state must enable Dashboard');
"#,
    );
}

#[test]
fn sidebar_and_detail_static_contracts() {
    let sidebar_html = include_str!("../frontend/sidebar.html");
    let sidebar_js = include_str!("../frontend/sidebar.js");
    let index_html = include_str!("../frontend/index.html");
    let app_js = include_str!("../frontend/app.js");
    let commands = include_str!("../src/commands.rs");
    let lib = include_str!("../src/lib.rs");
    let dev_flow = include_str!("../src/dev_flow.rs");
    let capabilities = include_str!("../capabilities/default.json");

    // Summary, claimed rows, more N, and the Dashboard entry live in the
    // sidebar; Dashboard sits immediately above Settings.
    for id in [
        "sidebarDevFlowGroup",
        "sidebarDevFlowSummary",
        "sidebarDevFlowClaimed",
        "sidebarDevFlowMore",
        "sidebarDevFlowDashboard",
        "sidebarStatusSection",
    ] {
        assert!(
            sidebar_html.contains(&format!("id=\"{id}\"")),
            "{id} in sidebar.html"
        );
    }
    let dashboard = sidebar_html.find("sidebarDevFlowDashboard").unwrap();
    let settings = sidebar_html.find("openSidebarSettings()").unwrap();
    assert!(dashboard < settings, "Dashboard must be above Settings");

    // Responsive fitting uses ResizeObserver, measured row height, font-aware
    // layout, and preserves a usable Sessions area.
    assert!(sidebar_js.contains("new ResizeObserver("));
    assert!(sidebar_js.contains("measureDevFlowRowHeight"));
    assert!(sidebar_js.contains("SIDEBAR_SESSIONS_MIN_PX"));
    assert!(sidebar_js.contains("sidebarStatusSection"));
    assert!(sidebar_js.contains("sidebarInvoke('dev_flow_detail'"));
    assert!(sidebar_js.contains("const DEV_FLOW_HOVER_DELAY_MS = 180;"));
    assert!(sidebar_js.contains("element.onpointerenter = () =>"));
    assert!(sidebar_js.contains("element.onpointerleave = () =>"));
    assert!(sidebar_js.contains("sidebarEmit('dev-flow-detail-dismiss')"));
    assert!(app_js.contains("listen('dev-flow-detail-dismiss'"));
    assert!(sidebar_js.contains("sidebarInvoke('open_dev_flow_dashboard'"));
    assert!(sidebar_js.contains("snapshot.showDevFlowDashboard"));
    assert!(sidebar_js.contains("button.hidden = !visible;"));
    assert!(sidebar_html.contains(".sidebar-action[hidden] { display: none; }"));
    assert!(sidebar_html.contains("min-height: var(--dev-flow-sessions-min, 96px)"));

    // The main view owns the read-only overlay with focus, Escape, outside
    // click, and responsive narrow/wide presentation.
    for id in [
        "devFlowDetail",
        "devFlowDetailRevision",
        "devFlowDetailProject",
        "devFlowDetailSummary",
        "devFlowDetailList",
        "devFlowDetailClose",
    ] {
        assert!(
            index_html.contains(&format!("id=\"{id}\"")),
            "{id} in index.html"
        );
    }
    assert!(index_html.contains("@media (max-width: 720px)"));
    assert!(app_js.contains("listen('dev-flow-detail'"));
    assert!(app_js.contains("payload.revision < baseline"));
    assert!(app_js.contains("DEV_FLOW_DETAIL_BASELINE_LIMIT = 32"));
    assert!(app_js.contains("devFlowDetailBaselines.size > DEV_FLOW_DETAIL_BASELINE_LIMIT"));
    assert!(app_js.contains("item.filesModify"));
    assert!(app_js.contains("item.doneWhen"));
    assert!(app_js.contains("event.key === 'Escape'"));
    assert!(app_js.contains("closest('#devFlowDetail')"));
    assert!(app_js.contains("document.getElementById('devFlowDetailClose')"));
    assert!(app_js.contains("addEventListener('click', closeDevFlowDetail)"));

    // The typed IPC commands exist, are registered, and never mutate work.
    for command in ["dev_flow_detail", "open_dev_flow_dashboard"] {
        assert!(
            commands.contains(&format!("#[tauri::command]\npub async fn {command}")),
            "{command} in commands.rs"
        );
        assert!(
            lib.contains(&format!("commands::{command}")),
            "{command} registered in lib.rs"
        );
    }
    assert!(lib.contains("tauri_plugin_opener::init()"));
    assert!(capabilities.contains("opener:allow-open-url"));
    assert!(capabilities.contains("http://127.0.0.1:*/*"));
    assert!(
        dev_flow.contains("pub const DASHBOARD_OPEN_PREFIX: &str = \"dev-flow.dashboard-open:\""),
        "stable open-error ID"
    );
}
