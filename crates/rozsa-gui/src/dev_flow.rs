// FrameworkTree
// dev_flow.rs
// ├── struct RuntimeDiagnostics
// ├── struct RuntimeSessionBinding
// ├── struct DevFlowRuntimeInner
// ├── struct DynDiscoveryRunner
// ├── impl DynDiscoveryRunner
// ├── run()
// ├── struct DevFlowRuntime
// ├── _assert_registry_send_sync()
// ├── assert_send_sync()
// ├── impl DevFlowRuntime
// ├── new()
// ├── new_with_sidebar_refresh()
// ├── attach_notifier()
// ├── attach_sidebar_refresh()
// ├── shutdown()
// ├── reconfigure()
// ├── session_started()
// ├── session_resumed()
// ├── session_finished()
// ├── session_closed()
// ├── switch_to_session()
// ├── on_successful_bash()
// ├── diagnostics()
// ├── sidebar_snapshot()
// ├── dashboard_url()
// ├── detail()
// ├── session_state()
// ├── last_stop_at()
// ├── active_sessions()
// ├── reconfigure_with()
// ├── sync_current_project()
// ├── sweep_once()
// ├── probe_once()
// ├── note_state()
// ├── record_cli_error()
// ├── resolve_cli_error()
// ├── resolve_all_project_errors()
// ├── emit()
// ├── spawn_maintenance()
// ├── struct RealDashboardServiceFactory
// ├── impl RealDashboardServiceFactory
// ├── new()
// ├── impl RealDashboardServiceFactory
// ├── start()
// ├── struct DashboardProcessControl
// ├── impl DashboardProcessControl
// ├── shutdown()
// ├── is_alive()
// ├── run_connection_loop()
// ├── mark_stale()
// ├── notify_sidebar_refresh()
// ├── maybe_report_connection_error()
// ├── project_hash()
// ├── install_source_label()
// ├── revision_label()
// ├── availability_label()
// ├── validate_loopback_url()
// ├── struct DevFlowProjectIdentity
// ├── struct DevFlowClaimedSummary
// ├── struct DevFlowSidebarSnapshot
// ├── struct DevFlowDetailTarget
// ├── struct DevFlowDetailRequest
// ├── struct DevFlowDetailItem
// ├── struct DevFlowDetailPayload
// ├── validate_detail_request()
// ├── project_identity()
// ├── sidebar_snapshot_from_state()
// ├── summarize_work()
// ├── short_dev_flow_id()
// ├── task_status_label()
// ├── issue_status_label()
// ├── struct DevFlowSettingsSnapshot
// ├── struct DevFlowCliDiagnostics
// ├── struct DevFlowProjectDiagnostics
// ├── system_runtime()
// ├── real_factory_provider()
// ├── real_notifier()
// └── real_sidebar_refresher()

//! Dev-flow runtime orchestration, diagnostics, and settings commands for the
//! GUI. Owns the registry lifecycle, session activity wiring, and the real
//! dashboard process factory used while the integration is enabled.

use std::collections::{HashMap, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime};

use async_trait::async_trait;
use rozsa_app::dev_flow::dashboard::{DashboardTiming, ReconnectBackoff};
use rozsa_app::dev_flow::registry::{DashboardServiceControl, ServiceShutdownOutcome};
use rozsa_app::dev_flow::{
    CommandExecutionError, CommandOutput, DashboardClient, DashboardProcess,
    DashboardServiceFactory, DevFlowAvailability, DevFlowError, DevFlowIssueStatus,
    DevFlowProjectKey, DevFlowRegistry, DevFlowRevisionKey, DevFlowServiceHandle, DevFlowSnapshot,
    DevFlowTaskStatus, DiscoveryCommandRunner, DiscoveryEnvironment, DowDiscoveryError,
    DowInstallSource, ProjectCommandRunner, SessionDevFlowState, SystemCommandRunner,
    discover_dow_with, start_dashboard,
};
use rozsa_app::settings::DevFlowSettings;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tokio::time::{MissedTickBehavior, sleep};
use tokio_util::sync::CancellationToken;

use crate::notifications::{AppNotificationEvent, NotificationSeverity, emit_notification};
use crate::state::GuiState;

pub const CLI_ERROR_ID: &str = "dev-flow.cli";
pub const DASHBOARD_START_PREFIX: &str = "dev-flow.dashboard-start:";
pub const CONNECTION_PREFIX: &str = "dev-flow.connection:";
pub const DASHBOARD_OPEN_PREFIX: &str = "dev-flow.dashboard-open:";

const DASHBOARD_PORTS: RangeInclusive<u16> = 9800..=9900;
const SWEEP_INTERVAL: Duration = Duration::from_secs(60);
const PROBE_INTERVAL: Duration = Duration::from_secs(2);
const ERROR_TIMEOUT_MS: u64 = 6_000;

/// Provider that maps an adopted `dow` executable to the dashboard service
/// factory used by a freshly built registry.
pub type DashboardFactoryProvider =
    Arc<dyn Fn(&Path) -> Arc<dyn DashboardServiceFactory> + Send + Sync>;

/// Notification sink used by the runtime and its dashboard factories. The app
/// attaches the real Tauri emitter during setup; tests attach channel sinks.
pub type NotificationSink = Arc<dyn Fn(AppNotificationEvent) + Send + Sync>;
pub type SharedNotificationSink = Arc<std::sync::Mutex<Option<NotificationSink>>>;

/// Push hook invoked whenever a dashboard snapshot changes or goes stale so
/// the GUI can re-emit the sidebar summary without polling.
pub type SidebarRefreshSink = Arc<dyn Fn() + Send + Sync>;
pub type SharedSidebarRefreshSink = Arc<std::sync::Mutex<Option<SidebarRefreshSink>>>;

#[derive(Clone, Debug, Default)]
struct RuntimeDiagnostics {
    cli_error: Option<String>,
    executable: Option<PathBuf>,
    version: Option<String>,
    source: Option<DowInstallSource>,
}

#[derive(Clone, Debug)]
struct RuntimeSessionBinding {
    cwd: PathBuf,
    active: bool,
    last_stop_at: Option<SystemTime>,
}

struct DevFlowRuntimeInner {
    notifier: SharedNotificationSink,
    project_runner: Arc<dyn ProjectCommandRunner>,
    discovery_runner: Arc<dyn DiscoveryCommandRunner>,
    discovery_environment: DiscoveryEnvironment,
    factory_provider: DashboardFactoryProvider,
    registry: Mutex<Option<Arc<DevFlowRegistry>>>,
    reconfigure: Mutex<()>,
    sessions: Mutex<HashMap<String, RuntimeSessionBinding>>,
    current_session: Mutex<Option<String>>,
    diagnostics: Mutex<RuntimeDiagnostics>,
    sidebar_refresh: SharedSidebarRefreshSink,
    project_errors: Mutex<HashMap<String, String>>,
    shutdown: CancellationToken,
}

/// Sized adapter that lets the runtime call `discover_dow_with` through a
/// trait-object discovery runner.
struct DynDiscoveryRunner(Arc<dyn DiscoveryCommandRunner>);

#[async_trait]
impl DiscoveryCommandRunner for DynDiscoveryRunner {
    async fn run(
        &self,
        executable: &Path,
        args: &[&str],
        deadline: Duration,
    ) -> Result<CommandOutput, CommandExecutionError> {
        self.0.run(executable, args, deadline).await
    }
}

/// Session-agnostic runtime that owns the dev-flow registry and background
/// maintenance tasks (one-minute sweep, two-second selected-project probe).
pub struct DevFlowRuntime {
    inner: Arc<DevFlowRuntimeInner>,
}

#[allow(dead_code)]
fn _assert_registry_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<DevFlowRegistry>();
}

impl DevFlowRuntime {
    pub fn new(
        notifier: SharedNotificationSink,
        project_runner: Arc<dyn ProjectCommandRunner>,
        discovery_runner: Arc<dyn DiscoveryCommandRunner>,
        discovery_environment: DiscoveryEnvironment,
        factory_provider: DashboardFactoryProvider,
    ) -> Arc<Self> {
        Self::new_with_sidebar_refresh(
            notifier,
            project_runner,
            discovery_runner,
            discovery_environment,
            factory_provider,
            Arc::new(std::sync::Mutex::new(None)),
        )
    }

    /// Build a runtime around a caller-owned refresh slot. Production uses
    /// this constructor so dashboard SSE updates and late GUI attachment share
    /// one sink rather than capturing independent empty slots.
    fn new_with_sidebar_refresh(
        notifier: SharedNotificationSink,
        project_runner: Arc<dyn ProjectCommandRunner>,
        discovery_runner: Arc<dyn DiscoveryCommandRunner>,
        discovery_environment: DiscoveryEnvironment,
        factory_provider: DashboardFactoryProvider,
        sidebar_refresh: SharedSidebarRefreshSink,
    ) -> Arc<Self> {
        let inner = Arc::new(DevFlowRuntimeInner {
            notifier,
            project_runner,
            discovery_runner,
            discovery_environment,
            factory_provider,
            registry: Mutex::new(None),
            reconfigure: Mutex::new(()),
            sessions: Mutex::new(HashMap::new()),
            current_session: Mutex::new(None),
            diagnostics: Mutex::new(RuntimeDiagnostics::default()),
            sidebar_refresh,
            project_errors: Mutex::new(HashMap::new()),
            shutdown: CancellationToken::new(),
        });
        let runtime = Arc::new(Self { inner });
        runtime.spawn_maintenance();
        runtime
    }

    /// Attach the application notification sink. Safe to call before the
    /// first reconfigure; replaces any previously attached sink.
    pub fn attach_notifier(&self, sink: NotificationSink) {
        *self.inner.notifier.lock().unwrap() = Some(sink);
    }

    /// Attach a sink invoked after every dashboard snapshot update so the GUI
    /// can push a fresh sidebar summary. Idempotent; replaces any previous sink.
    pub fn attach_sidebar_refresh(&self, sink: SidebarRefreshSink) {
        *self.inner.sidebar_refresh.lock().unwrap() = Some(sink);
    }

    /// Stop the background maintenance loop. Idempotent.
    pub fn shutdown(&self) {
        self.inner.shutdown.cancel();
    }

    /// Validate the current settings and adopt or stop the runtime. Discovery
    /// failures produce the stable `dev-flow.cli` error without falling back.
    pub async fn reconfigure(&self, settings: &DevFlowSettings) -> Result<(), String> {
        self.reconfigure_with(settings).await
    }

    /// Report the session as active and (re)associate it when its cwd moved.
    pub async fn session_started(&self, session_id: &str, cwd: PathBuf) {
        let changed = {
            let mut sessions = self.inner.sessions.lock().await;
            let changed = sessions
                .get(session_id)
                .is_none_or(|binding| binding.cwd != cwd);
            sessions
                .entry(session_id.to_owned())
                .and_modify(|binding| {
                    binding.cwd = cwd.clone();
                    binding.active = true;
                    binding.last_stop_at = None;
                })
                .or_insert_with(|| RuntimeSessionBinding {
                    cwd: cwd.clone(),
                    active: true,
                    last_stop_at: None,
                });
            changed
        };
        let Some(registry) = self.inner.registry.lock().await.clone() else {
            return;
        };
        if changed && let Ok(state) = registry.associate_session(session_id.to_owned(), cwd).await {
            self.note_state(state).await;
        }
        registry.session_active(session_id).await;
    }

    /// Permission or user-question resolution keeps a waiting session active.
    pub async fn session_resumed(&self, session_id: &str) {
        if let Some(binding) = self.inner.sessions.lock().await.get_mut(session_id) {
            binding.active = true;
            binding.last_stop_at = None;
        }
        if let Some(registry) = self.inner.registry.lock().await.clone() {
            registry.session_active(session_id).await;
        }
    }

    pub async fn session_finished(&self, session_id: &str, stopped_at: SystemTime) {
        if let Some(binding) = self.inner.sessions.lock().await.get_mut(session_id) {
            binding.active = false;
            binding.last_stop_at = Some(stopped_at);
        }
        if let Some(registry) = self.inner.registry.lock().await.clone() {
            registry.session_finished(session_id, stopped_at).await;
        }
    }

    pub async fn session_closed(&self, session_id: &str) {
        self.inner.sessions.lock().await.remove(session_id);
        if self.inner.current_session.lock().await.as_deref() == Some(session_id) {
            *self.inner.current_session.lock().await = None;
        }
        if let Some(registry) = self.inner.registry.lock().await.clone() {
            registry.session_closed(session_id, SystemTime::now()).await;
        }
        self.sync_current_project().await;
    }

    /// Session selection: re-evaluate identity for the selected session and
    /// protect its project from reclamation.
    pub async fn switch_to_session(&self, session_id: &str, cwd: PathBuf) {
        self.inner
            .sessions
            .lock()
            .await
            .entry(session_id.to_owned())
            .and_modify(|binding| binding.cwd = cwd.clone())
            .or_insert_with(|| RuntimeSessionBinding {
                cwd: cwd.clone(),
                active: false,
                last_stop_at: None,
            });
        *self.inner.current_session.lock().await = Some(session_id.to_owned());
        if let Some(registry) = self.inner.registry.lock().await.clone()
            && let Ok(state) = registry.associate_session(session_id.to_owned(), cwd).await
        {
            self.note_state(state).await;
        }
        self.sync_current_project().await;
    }

    /// A successful Bash completion re-evaluates the identity of every session
    /// associated with the resolved worktree.
    pub async fn on_successful_bash(&self, session_id: &str, cwd: PathBuf) {
        self.inner
            .sessions
            .lock()
            .await
            .entry(session_id.to_owned())
            .and_modify(|binding| binding.cwd = cwd.clone())
            .or_insert_with(|| RuntimeSessionBinding {
                cwd: cwd.clone(),
                active: false,
                last_stop_at: None,
            });
        if let Some(registry) = self.inner.registry.lock().await.clone()
            && let Ok(states) = registry.rescan_after_successful_bash(&cwd).await
        {
            for (_, state) in states {
                self.note_state(state).await;
            }
        }
        self.sync_current_project().await;
    }

    pub async fn diagnostics(&self, settings: &DevFlowSettings) -> DevFlowSettingsSnapshot {
        // A disabled integration owns no registry, but Settings must still be
        // able to report an installed CLI so the master switch can be restored.
        let diag = if settings.enabled {
            self.inner.diagnostics.lock().await.clone()
        } else {
            match discover_dow_with(
                settings,
                &self.inner.discovery_environment,
                &DynDiscoveryRunner(self.inner.discovery_runner.clone()),
            )
            .await
            {
                Ok(found) => RuntimeDiagnostics {
                    cli_error: None,
                    executable: Some(found.executable),
                    version: Some(found.version.to_string()),
                    source: Some(found.source),
                },
                Err(error) => RuntimeDiagnostics {
                    cli_error: Some(error.to_string()),
                    ..RuntimeDiagnostics::default()
                },
            }
        };
        let project = {
            let current = self.inner.current_session.lock().await.clone();
            let registry = self.inner.registry.lock().await.clone();
            match (current, registry) {
                (Some(session_id), Some(registry)) => {
                    if let Some(state) = registry.session_state(&session_id).await {
                        let (availability, message) = availability_label(&state.availability);
                        let memory_use_bytes = registry.project_usage_bytes(&state.project).await;
                        let dashboard_url = state
                            .service
                            .as_ref()
                            .and_then(|service| service.base_url())
                            .map(ToString::to_string);
                        let snapshot = match &state.service {
                            Some(service) => service.snapshot().read().await.clone(),
                            None => None,
                        };
                        Some(DevFlowProjectDiagnostics {
                            session_id,
                            root: Some(state.project.root.to_string_lossy().into_owned()),
                            revision: Some(revision_label(&state.project.revision)),
                            availability,
                            message,
                            dashboard_url,
                            snapshot_revision: snapshot.as_ref().map(|snapshot| snapshot.revision),
                            last_sync_unix_ms: snapshot
                                .as_ref()
                                .and_then(|snapshot| {
                                    snapshot
                                        .received_at
                                        .duration_since(SystemTime::UNIX_EPOCH)
                                        .ok()
                                })
                                .map(|duration| {
                                    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
                                }),
                            memory_use_bytes,
                        })
                    } else {
                        None
                    }
                }
                _ => None,
            }
        };
        DevFlowSettingsSnapshot {
            enabled: settings.enabled,
            show_sidebar_status: settings.show_sidebar_status,
            show_dashboard_button: settings.show_dashboard_button,
            executable_path: settings
                .executable_path
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned()),
            cli: DevFlowCliDiagnostics {
                available: diag.executable.is_some(),
                executable: diag
                    .executable
                    .as_ref()
                    .map(|path| path.to_string_lossy().into_owned()),
                version: diag.version.clone(),
                source: diag.source.map(install_source_label).map(str::to_owned),
                error: diag.cli_error.clone(),
            },
            project,
        }
    }

    /// Read-only sidebar summary for one associated session. Returns `None`
    /// when the integration owns no registry (disabled or no CLI).
    pub async fn sidebar_snapshot(
        &self,
        session_id: &str,
        show_sidebar_status: bool,
    ) -> Option<DevFlowSidebarSnapshot> {
        let state = self.session_state(session_id).await?;
        let snapshot = match &state.service {
            Some(service) => service.snapshot().read().await.clone(),
            None => None,
        };
        Some(sidebar_snapshot_from_state(
            &state,
            snapshot.as_ref(),
            show_sidebar_status,
        ))
    }

    /// Loopback dashboard URL for the session's project, starting or reusing
    /// the shared service, plus the stable project key used for notifications.
    /// Never touches a dashboard mutation route.
    pub async fn dashboard_url(
        &self,
        session_id: &str,
        cwd: PathBuf,
    ) -> Result<(String, String), String> {
        let Some(registry) = self.inner.registry.lock().await.clone() else {
            return Err("dev-flow is disabled or no compatible CLI is available".to_string());
        };
        let state = registry
            .associate_session(session_id.to_owned(), cwd)
            .await
            .map_err(|error| error.to_string())?;
        self.note_state(state.clone()).await;
        self.sync_current_project().await;
        if state.availability != DevFlowAvailability::Ready {
            let (_, message) = availability_label(&state.availability);
            return Err(message.unwrap_or_else(|| state.availability.to_string()));
        }
        let Some(service) = &state.service else {
            return Err("dev-flow dashboard is not running".to_string());
        };
        let Some(url) = service.base_url() else {
            return Err("dev-flow dashboard URL is unavailable".to_string());
        };
        Ok((url.to_string(), project_hash(&state.project)))
    }

    /// Read-only detail payload for one validated request. The request must
    /// match the live project key and snapshot revision exactly; anything else
    /// is rejected before any detail reaches the main view.
    pub async fn detail(
        &self,
        session_id: &str,
        request: &DevFlowDetailRequest,
    ) -> Result<DevFlowDetailPayload, String> {
        let Some(state) = self.session_state(session_id).await else {
            return Err("dev-flow is unavailable".to_string());
        };
        let snapshot = match &state.service {
            Some(service) => service.snapshot().read().await.clone(),
            None => None,
        };
        let Some(snapshot) = snapshot else {
            return Err("dev-flow dashboard has no snapshot yet".to_string());
        };
        let current = sidebar_snapshot_from_state(&state, Some(&snapshot), true);
        validate_detail_request(&current, request)?;
        let (open_tasks, open_issues, _claimed) = summarize_work(&snapshot);
        let mut items = Vec::new();
        let mut claimed_ids = Vec::new();
        for task in &snapshot.tasks {
            if task.status != DevFlowTaskStatus::Done {
                if task.status == DevFlowTaskStatus::InProgress {
                    claimed_ids.push(task.id.clone());
                }
                items.push(DevFlowDetailItem {
                    kind: "task".to_string(),
                    id: task.id.clone(),
                    short_id: short_dev_flow_id(&task.id),
                    title: task.title.clone(),
                    status: task_status_label(task.status),
                    priority: task.priority.clone(),
                    complexity: task.complexity.clone(),
                    task_type: task.task_type.clone(),
                    refs: task.refs.clone(),
                    depends_on: task.depends_on.clone(),
                    done_when: task.done_when.clone(),
                    files_create: task.files_create.clone(),
                    files_modify: task.files_modify.clone(),
                    files_test: task.files_test.clone(),
                    severity: None,
                    description: None,
                });
            }
        }
        for issue in &snapshot.issues {
            if issue.status != DevFlowIssueStatus::Closed {
                if issue.status == DevFlowIssueStatus::InProgress {
                    claimed_ids.push(issue.id.clone());
                }
                items.push(DevFlowDetailItem {
                    kind: "issue".to_string(),
                    id: issue.id.clone(),
                    short_id: short_dev_flow_id(&issue.id),
                    title: issue.title.clone(),
                    status: issue_status_label(issue.status),
                    priority: None,
                    complexity: None,
                    task_type: None,
                    refs: None,
                    depends_on: Vec::new(),
                    done_when: Vec::new(),
                    files_create: issue.files_create.clone(),
                    files_modify: issue.files_modify.clone(),
                    files_test: Vec::new(),
                    severity: issue.severity.clone(),
                    description: issue.description.clone(),
                });
            }
        }
        let focus_id = match &request.target {
            DevFlowDetailTarget { kind, id } if kind == "item" => id.clone(),
            _ => None,
        };
        if let Some(focus_id) = &focus_id
            && !items.iter().any(|item| &item.id == focus_id)
        {
            return Err("dev-flow detail item does not exist in this snapshot".to_string());
        }
        Ok(DevFlowDetailPayload {
            project: current.project.clone(),
            revision: snapshot.revision,
            open_tasks,
            open_issues,
            items,
            claimed_ids,
            focus_id,
            stale: snapshot.stale,
            availability: current.availability.clone(),
            availability_message: current.availability_message.clone(),
        })
    }

    /// Current registry state for one associated session, if any.
    pub async fn session_state(&self, session_id: &str) -> Option<SessionDevFlowState> {
        match self.inner.registry.lock().await.clone() {
            Some(registry) => registry.session_state(session_id).await,
            None => None,
        }
    }

    /// Exact runtime stop time recorded for one associated session, if any.
    pub async fn last_stop_at(&self, session_id: &str) -> Option<SystemTime> {
        match self.inner.registry.lock().await.clone() {
            Some(registry) => registry.last_stop_at(session_id).await,
            None => None,
        }
    }

    /// Number of sessions currently reported active to the registry.
    pub async fn active_sessions(&self) -> usize {
        match self.inner.registry.lock().await.clone() {
            Some(registry) => registry.diagnostics().await.active_sessions,
            None => 0,
        }
    }

    async fn reconfigure_with(&self, settings: &DevFlowSettings) -> Result<(), String> {
        let _reconfigure_guard = self.inner.reconfigure.lock().await;
        let discovered = if settings.enabled {
            match discover_dow_with(
                settings,
                &self.inner.discovery_environment,
                &DynDiscoveryRunner(self.inner.discovery_runner.clone()),
            )
            .await
            {
                Ok(found) => Some(found),
                Err(error) => {
                    self.record_cli_error(error).await;
                    None
                }
            }
        } else {
            None
        };

        let adopted = self.inner.diagnostics.lock().await.executable.clone();
        let existing = self.inner.registry.lock().await.clone();
        let restart = match (&discovered, &existing) {
            (Some(found), Some(_)) => adopted.as_ref() != Some(&found.executable),
            (None, Some(_)) | (Some(_), None) => true,
            (None, None) => false,
        };
        if restart {
            let old = self.inner.registry.lock().await.take();
            if let Some(old) = old {
                let report = old.shutdown_all().await;
                tracing::info!(
                    "dev-flow: stopped {} owned dashboard services ({} remain protected)",
                    report.terminated.len(),
                    report.still_running.len()
                );
            }
        }

        match &discovered {
            Some(found) => {
                let registry = if restart {
                    let factory = (self.inner.factory_provider)(&found.executable);
                    Arc::new(DevFlowRegistry::new(
                        found.executable.clone(),
                        self.inner.project_runner.clone(),
                        factory,
                    ))
                } else {
                    existing.expect("unchanged dev-flow registry exists")
                };
                if restart {
                    *self.inner.registry.lock().await = Some(registry.clone());
                }
                let sessions = self.inner.sessions.lock().await.clone();
                for (session_id, binding) in sessions {
                    if let Ok(state) = registry.associate_session(&session_id, binding.cwd).await {
                        if binding.active {
                            registry.session_active(&session_id).await;
                        } else if let Some(stopped_at) = binding.last_stop_at {
                            registry.session_finished(&session_id, stopped_at).await;
                        }
                        self.note_state(state).await;
                    }
                }
                {
                    let mut diag = self.inner.diagnostics.lock().await;
                    diag.executable = Some(found.executable.clone());
                    diag.version = Some(found.version.to_string());
                    diag.source = Some(found.source);
                }
                self.resolve_cli_error().await;
            }
            None => {
                *self.inner.registry.lock().await = None;
                {
                    let mut diag = self.inner.diagnostics.lock().await;
                    diag.executable = None;
                    diag.version = None;
                    diag.source = None;
                }
                if !settings.enabled {
                    // Intentional disable resolves integration-owned errors.
                    self.resolve_cli_error().await;
                    self.resolve_all_project_errors().await;
                }
            }
        }
        self.sync_current_project().await;
        Ok(())
    }

    async fn sync_current_project(&self) {
        let Some(registry) = self.inner.registry.lock().await.clone() else {
            return;
        };
        let project = {
            let current = self.inner.current_session.lock().await.clone();
            match current {
                Some(session_id) => registry.session_state(&session_id).await.map(|s| s.project),
                None => None,
            }
        };
        registry.set_current_project(project).await;
    }

    async fn sweep_once(&self) {
        let Some(registry) = self.inner.registry.lock().await.clone() else {
            return;
        };
        let report = registry.sweep(SystemTime::now()).await;
        if !report.reclaimed.is_empty() {
            tracing::debug!(
                "dev-flow: reclaimed {} idle dashboard services",
                report.reclaimed.len()
            );
        }
    }

    /// One selected-project probe iteration: re-evaluates identity for every
    /// session associated with the selected worktree. Runs on a two-second
    /// timer while a registry is active; exposed for tests and diagnostics.
    pub async fn probe_once(&self) {
        let Some(registry) = self.inner.registry.lock().await.clone() else {
            return;
        };
        let cwd = {
            let current = self.inner.current_session.lock().await.clone();
            let sessions = self.inner.sessions.lock().await;
            current
                .and_then(|session_id| sessions.get(&session_id).map(|binding| binding.cwd.clone()))
        };
        let Some(cwd) = cwd else {
            return;
        };
        match registry.rescan_after_successful_bash(&cwd).await {
            Ok(states) => {
                for (_, state) in states {
                    self.note_state(state).await;
                }
            }
            Err(error) => {
                tracing::debug!("dev-flow: selected-project probe failed: {error}");
            }
        }
        self.sync_current_project().await;
    }

    async fn note_state(&self, state: SessionDevFlowState) {
        let id = format!("{DASHBOARD_START_PREFIX}{}", project_hash(&state.project));
        let mut errors = self.inner.project_errors.lock().await;
        match &state.availability {
            DevFlowAvailability::DashboardStartFailed(error) => {
                if errors.get(&id) != Some(error) {
                    errors.insert(id.clone(), error.clone());
                    drop(errors);
                    self.emit(AppNotificationEvent::Upsert {
                        id,
                        severity: NotificationSeverity::Error,
                        title: "Dev-flow dashboard failed to start".to_string(),
                        message: error.clone(),
                        timeout_ms: ERROR_TIMEOUT_MS,
                    })
                    .await;
                }
            }
            _ => {
                if errors.remove(&id).is_some() {
                    drop(errors);
                    self.emit(AppNotificationEvent::Resolve { id }).await;
                }
            }
        }
    }

    async fn record_cli_error(&self, error: DowDiscoveryError) {
        let message = error.to_string();
        let changed = {
            let mut diag = self.inner.diagnostics.lock().await;
            let changed = diag.cli_error.as_deref() != Some(&message);
            diag.cli_error = Some(message.clone());
            changed
        };
        if !changed {
            return;
        }
        self.emit(AppNotificationEvent::Upsert {
            id: CLI_ERROR_ID.to_string(),
            severity: NotificationSeverity::Error,
            title: "Dev-flow CLI unavailable".to_string(),
            message,
            timeout_ms: ERROR_TIMEOUT_MS,
        })
        .await;
    }

    async fn resolve_cli_error(&self) {
        let had_error = self
            .inner
            .diagnostics
            .lock()
            .await
            .cli_error
            .take()
            .is_some();
        if had_error {
            self.emit(AppNotificationEvent::Resolve {
                id: CLI_ERROR_ID.to_string(),
            })
            .await;
        }
    }

    async fn resolve_all_project_errors(&self) {
        let ids = self
            .inner
            .project_errors
            .lock()
            .await
            .drain()
            .collect::<Vec<_>>();
        for (id, _) in ids {
            self.emit(AppNotificationEvent::Resolve { id }).await;
        }
    }

    async fn emit(&self, event: AppNotificationEvent) {
        let sink = self.inner.notifier.lock().unwrap().clone();
        if let Some(sink) = sink {
            sink(event);
        }
    }

    fn spawn_maintenance(self: &Arc<Self>) {
        let weak = Arc::downgrade(self);
        let token = self.inner.shutdown.clone();
        tokio::spawn(async move {
            let mut sweep = tokio::time::interval(SWEEP_INTERVAL);
            let mut probe = tokio::time::interval(PROBE_INTERVAL);
            sweep.set_missed_tick_behavior(MissedTickBehavior::Skip);
            probe.set_missed_tick_behavior(MissedTickBehavior::Skip);
            loop {
                let Some(runtime) = weak.upgrade() else {
                    break;
                };
                tokio::select! {
                    _ = token.cancelled() => break,
                    _ = sweep.tick() => runtime.sweep_once().await,
                    _ = probe.tick() => runtime.probe_once().await,
                }
            }
        });
    }
}

/// Real dashboard service factory bound to the adopted `dow` executable.
/// Starts the child, keeps its SSE stream alive, and reports connection
/// failures through the stable per-project notification IDs.
pub struct RealDashboardServiceFactory {
    executable: PathBuf,
    timing: DashboardTiming,
    notifier: SharedNotificationSink,
    sidebar_refresh: SharedSidebarRefreshSink,
    ports: RangeInclusive<u16>,
}

impl RealDashboardServiceFactory {
    pub fn new(
        executable: PathBuf,
        notifier: SharedNotificationSink,
        sidebar_refresh: SharedSidebarRefreshSink,
    ) -> Self {
        Self {
            executable,
            timing: DashboardTiming::default(),
            notifier,
            sidebar_refresh,
            ports: DASHBOARD_PORTS,
        }
    }
}

static NEXT_SERVICE_ID: AtomicU64 = AtomicU64::new(0);

#[async_trait]
impl DashboardServiceFactory for RealDashboardServiceFactory {
    async fn start(&self, project: &DevFlowProjectKey) -> Result<DevFlowServiceHandle, String> {
        let cancellation = CancellationToken::new();
        let process = start_dashboard(
            &self.executable,
            &project.root,
            self.ports.clone(),
            self.timing,
            &cancellation,
        )
        .await
        .map_err(|error| error.to_string())?;
        let project_id = project_hash(project);
        let snapshot = Arc::new(tokio::sync::RwLock::new(Some(
            process.initial_snapshot.clone(),
        )));
        let pid = process.id();

        if let Some(sink) = self.notifier.lock().unwrap().clone() {
            sink(AppNotificationEvent::Resolve {
                id: format!("{DASHBOARD_START_PREFIX}{project_id}"),
            });
        }

        let stream_snapshot = snapshot.clone();
        let stream_client = process.client.clone();
        let dashboard_base_url = stream_client.base_url().clone();
        let stream_cancellation = cancellation.clone();
        let connection_id = format!("{CONNECTION_PREFIX}{project_id}");
        let notifier = self.notifier.clone();
        let sidebar_refresh = self.sidebar_refresh.clone();
        tokio::spawn(async move {
            run_connection_loop(
                &stream_client,
                stream_snapshot,
                &stream_cancellation,
                &notifier,
                &connection_id,
                &sidebar_refresh,
            )
            .await;
        });

        let control = Arc::new(DashboardProcessControl {
            process: Mutex::new(Some(process)),
            cancellation,
        });
        let id = NEXT_SERVICE_ID.fetch_add(1, Ordering::Relaxed);
        Ok(DevFlowServiceHandle::with_base_url(
            id,
            snapshot,
            control,
            pid,
            Some(dashboard_base_url),
        ))
    }
}

struct DashboardProcessControl {
    process: Mutex<Option<DashboardProcess>>,
    cancellation: CancellationToken,
}

#[async_trait]
impl DashboardServiceControl for DashboardProcessControl {
    async fn shutdown(&self, grace: Duration) -> ServiceShutdownOutcome {
        self.cancellation.cancel();
        let mut guard = self.process.lock().await;
        let Some(process) = guard.as_mut() else {
            return ServiceShutdownOutcome::Exited;
        };
        if process.wait_for_graceful_exit(grace).await {
            guard.take();
            ServiceShutdownOutcome::Exited
        } else {
            ServiceShutdownOutcome::StillRunning
        }
    }

    async fn is_alive(&self) -> bool {
        let guard = self.process.lock().await;
        match guard.as_ref() {
            Some(process) => process.client.fetch_snapshot().await.is_ok(),
            None => false,
        }
    }
}

async fn run_connection_loop(
    client: &DashboardClient,
    snapshot: Arc<tokio::sync::RwLock<Option<DevFlowSnapshot>>>,
    cancellation: &CancellationToken,
    notifier: &SharedNotificationSink,
    connection_id: &str,
    sidebar_refresh: &SharedSidebarRefreshSink,
) {
    let mut backoff = ReconnectBackoff::default();
    let mut disconnected_since: Option<Instant> = None;
    let mut error_reported = false;
    let mut delay = Duration::ZERO;
    loop {
        if cancellation.is_cancelled() {
            return;
        }
        if !delay.is_zero() {
            tokio::select! {
                _ = sleep(delay) => {}
                _ = cancellation.cancelled() => return,
            }
        }

        let mut stream = match client.subscribe_cancellable(cancellation).await {
            Ok(stream) => stream,
            Err(DevFlowError::Cancelled) => return,
            Err(_) => {
                delay = backoff.next_delay();
                maybe_report_connection_error(
                    notifier,
                    connection_id,
                    &mut backoff,
                    &mut disconnected_since,
                    &mut error_reported,
                )
                .await;
                continue;
            }
        };

        loop {
            match stream.next_snapshot(cancellation).await {
                Ok(Some(next)) => {
                    *snapshot.write().await = Some(next);
                    notify_sidebar_refresh(sidebar_refresh);
                    backoff.reset();
                    disconnected_since = None;
                    if error_reported {
                        if let Some(sink) = notifier.lock().unwrap().clone() {
                            sink(AppNotificationEvent::Resolve {
                                id: connection_id.to_string(),
                            });
                        }
                        error_reported = false;
                    }
                }
                Ok(None) => {
                    mark_stale(&snapshot).await;
                    notify_sidebar_refresh(sidebar_refresh);
                    break;
                }
                Err(DevFlowError::Cancelled) => return,
                Err(_) => {
                    mark_stale(&snapshot).await;
                    notify_sidebar_refresh(sidebar_refresh);
                    break;
                }
            }
        }
        if cancellation.is_cancelled() {
            return;
        }
        delay = backoff.next_delay();
        maybe_report_connection_error(
            notifier,
            connection_id,
            &mut backoff,
            &mut disconnected_since,
            &mut error_reported,
        )
        .await;
    }
}

async fn mark_stale(snapshot: &Arc<tokio::sync::RwLock<Option<DevFlowSnapshot>>>) {
    if let Some(current) = snapshot.write().await.as_mut() {
        current.mark_stale();
    }
}

fn notify_sidebar_refresh(sink: &SharedSidebarRefreshSink) {
    if let Some(refresh) = sink.lock().unwrap().clone() {
        refresh();
    }
}

async fn maybe_report_connection_error(
    notifier: &SharedNotificationSink,
    connection_id: &str,
    backoff: &mut ReconnectBackoff,
    disconnected_since: &mut Option<Instant>,
    error_reported: &mut bool,
) {
    if disconnected_since.is_none() {
        *disconnected_since = Some(Instant::now());
    }
    let disconnected_for = disconnected_since
        .map(|started| started.elapsed())
        .unwrap_or_default();
    if backoff.should_report_error(disconnected_for) && !*error_reported {
        *error_reported = true;
        if let Some(sink) = notifier.lock().unwrap().clone() {
            sink(AppNotificationEvent::Upsert {
                id: connection_id.to_string(),
                severity: NotificationSeverity::Error,
                title: "Dev-flow dashboard disconnected".to_string(),
                message: "The dev-flow dashboard connection was lost; retrying.".to_string(),
                timeout_ms: ERROR_TIMEOUT_MS,
            });
        }
    }
}

/// Stable per-project hash covering canonical root and full revision identity.
pub fn project_hash(project: &DevFlowProjectKey) -> String {
    let mut hasher = DefaultHasher::new();
    project.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn install_source_label(source: DowInstallSource) -> &'static str {
    match source {
        DowInstallSource::Custom => "custom",
        DowInstallSource::Path => "path",
        DowInstallSource::Homebrew => "homebrew",
        DowInstallSource::Cargo => "cargo",
        DowInstallSource::Npm => "npm",
    }
}

fn revision_label(revision: &DevFlowRevisionKey) -> String {
    match revision {
        DevFlowRevisionKey::NamedBranch(branch) => branch.clone(),
        DevFlowRevisionKey::UnbornBranch(branch) => format!("unborn: {branch}"),
        DevFlowRevisionKey::DetachedCommit(oid) => {
            format!("detached: {}", &oid[..oid.len().min(7)])
        }
        DevFlowRevisionKey::NonGit => "non-git".to_string(),
    }
}

fn availability_label(availability: &DevFlowAvailability) -> (String, Option<String>) {
    let key = match availability {
        DevFlowAvailability::Ready => "ready",
        DevFlowAvailability::ProjectNotInitialized => "not-initialized",
        DevFlowAvailability::AmbiguousNonGitProject => "ambiguous-non-git",
        DevFlowAvailability::UnsupportedRevision => "unsupported-revision",
        DevFlowAvailability::StatusUnreadable(_) => "status-unreadable",
        DevFlowAvailability::StatusProbeFailed(_) => "status-probe-failed",
        DevFlowAvailability::DashboardStartFailed(_) => "dashboard-start-failed",
    };
    let message = match availability {
        DevFlowAvailability::Ready => None,
        other => Some(other.to_string()),
    };
    (key.to_string(), message)
}

/// True when the URL is an HTTP loopback URL (127.0.0.1, localhost, or ::1)
/// with a numeric port. The dashboard opener must never leave loopback.
pub fn validate_loopback_url(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("http://") else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.is_empty() || authority.contains("://") {
        return false;
    }
    let host = authority.rsplit_once(':').unwrap_or((authority, ""));
    if !host.1.parse::<u16>().is_ok() || host.1.is_empty() {
        return false;
    }
    let host = host.0.trim_start_matches('[').trim_end_matches(']');
    matches!(host, "127.0.0.1" | "localhost" | "::1")
}

/// Canonical project identity shared by the sidebar summary and detail UI.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevFlowProjectIdentity {
    pub project_key: String,
    pub root: String,
    pub revision: String,
}

/// One claimed (InProgress) Task or Issue shown as a sidebar row.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevFlowClaimedSummary {
    pub kind: String,
    pub id: String,
    pub short_id: String,
    pub title: String,
    pub priority: Option<String>,
    pub complexity: Option<String>,
}

/// Read-only sidebar summary of open and claimed dev-flow work.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevFlowSidebarSnapshot {
    pub project: DevFlowProjectIdentity,
    pub revision: u64,
    pub open_tasks: u32,
    pub open_issues: u32,
    pub claimed: Vec<DevFlowClaimedSummary>,
    pub stale: bool,
    pub availability: String,
    pub availability_message: Option<String>,
    pub dashboard_ready: bool,
    pub show_sidebar_status: bool,
}

/// Which part of the summary opened the detail UI.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevFlowDetailTarget {
    pub kind: String,
    pub id: Option<String>,
}

/// Typed detail request carrying the rendered project key, snapshot revision,
/// and target so the main view never renders stale or cross-project data.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevFlowDetailRequest {
    pub project_key: String,
    pub revision: u64,
    pub target: DevFlowDetailTarget,
}

/// One open Task or Issue row in the read-only detail UI.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevFlowDetailItem {
    pub kind: String,
    pub id: String,
    pub short_id: String,
    pub title: String,
    pub status: String,
    pub priority: Option<String>,
    pub complexity: Option<String>,
    pub task_type: Option<String>,
    pub refs: Option<String>,
    pub depends_on: Vec<String>,
    pub done_when: Vec<String>,
    pub files_create: Vec<String>,
    pub files_modify: Vec<String>,
    pub files_test: Vec<String>,
    pub severity: Option<String>,
    pub description: Option<String>,
}

/// Read-only detail payload emitted to the main WebView overlay.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevFlowDetailPayload {
    pub project: DevFlowProjectIdentity,
    pub revision: u64,
    pub open_tasks: u32,
    pub open_issues: u32,
    pub items: Vec<DevFlowDetailItem>,
    pub claimed_ids: Vec<String>,
    pub focus_id: Option<String>,
    pub stale: bool,
    pub availability: String,
    pub availability_message: Option<String>,
}

/// Rejects detail requests whose project key or snapshot revision no longer
/// match the live sidebar state; stale or cross-project requests never render.
pub fn validate_detail_request(
    current: &DevFlowSidebarSnapshot,
    request: &DevFlowDetailRequest,
) -> Result<(), String> {
    if request.project_key != current.project.project_key {
        return Err("dev-flow detail request targets a different project".to_string());
    }
    if request.revision != current.revision {
        return Err("dev-flow detail request is stale".to_string());
    }
    match &request.target {
        DevFlowDetailTarget { kind, id: Some(_) } if kind == "item" => {}
        DevFlowDetailTarget { kind, id: None } if kind == "summary" || kind == "more" => {}
        _ => return Err("dev-flow detail target is invalid".to_string()),
    }
    Ok(())
}

pub fn project_identity(project: &DevFlowProjectKey) -> DevFlowProjectIdentity {
    DevFlowProjectIdentity {
        project_key: project_hash(project),
        root: project.root.to_string_lossy().into_owned(),
        revision: revision_label(&project.revision),
    }
}

fn sidebar_snapshot_from_state(
    state: &SessionDevFlowState,
    snapshot: Option<&DevFlowSnapshot>,
    show_sidebar_status: bool,
) -> DevFlowSidebarSnapshot {
    let (availability, availability_message) = availability_label(&state.availability);
    let (open_tasks, open_issues, claimed) = snapshot
        .map(summarize_work)
        .unwrap_or_else(|| (0, 0, Vec::new()));
    DevFlowSidebarSnapshot {
        project: project_identity(&state.project),
        revision: snapshot.map(|snapshot| snapshot.revision).unwrap_or(0),
        open_tasks,
        open_issues,
        claimed,
        stale: snapshot.map(|snapshot| snapshot.stale).unwrap_or(false),
        availability,
        availability_message,
        dashboard_ready: state.service.is_some(),
        show_sidebar_status,
    }
}

/// Open work counts and claimed (InProgress) summaries. Claimed items are not
/// double-counted; closed/done work is excluded entirely.
fn summarize_work(snapshot: &DevFlowSnapshot) -> (u32, u32, Vec<DevFlowClaimedSummary>) {
    let mut open_tasks = 0;
    let mut open_issues = 0;
    let mut claimed = Vec::new();
    for task in &snapshot.tasks {
        if task.status == DevFlowTaskStatus::Done {
            continue;
        }
        open_tasks += 1;
        if task.status == DevFlowTaskStatus::InProgress {
            claimed.push(DevFlowClaimedSummary {
                kind: "task".to_string(),
                id: task.id.clone(),
                short_id: short_dev_flow_id(&task.id),
                title: task.title.clone(),
                priority: task.priority.clone(),
                complexity: task.complexity.clone(),
            });
        }
    }
    for issue in &snapshot.issues {
        if issue.status == DevFlowIssueStatus::Closed {
            continue;
        }
        open_issues += 1;
        if issue.status == DevFlowIssueStatus::InProgress {
            claimed.push(DevFlowClaimedSummary {
                kind: "issue".to_string(),
                id: issue.id.clone(),
                short_id: short_dev_flow_id(&issue.id),
                title: issue.title.clone(),
                priority: None,
                complexity: None,
            });
        }
    }
    (open_tasks, open_issues, claimed)
}

/// `TASK-T007` → `T007`, `ISSUE-I003` → `I003`; other ids pass through.
pub fn short_dev_flow_id(id: &str) -> String {
    if let Some(rest) = id.strip_prefix("TASK-") {
        return rest.to_string();
    }
    if let Some(rest) = id.strip_prefix("ISSUE-") {
        return rest.to_string();
    }
    id.to_string()
}

fn task_status_label(status: DevFlowTaskStatus) -> String {
    match status {
        DevFlowTaskStatus::Pending => "pending",
        DevFlowTaskStatus::InProgress => "in-progress",
        DevFlowTaskStatus::Done => "done",
    }
    .to_string()
}

fn issue_status_label(status: DevFlowIssueStatus) -> String {
    match status {
        DevFlowIssueStatus::Open => "open",
        DevFlowIssueStatus::InProgress => "in-progress",
        DevFlowIssueStatus::Closed => "closed",
    }
    .to_string()
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevFlowSettingsSnapshot {
    pub enabled: bool,
    pub show_sidebar_status: bool,
    pub show_dashboard_button: bool,
    pub executable_path: Option<String>,
    pub cli: DevFlowCliDiagnostics,
    pub project: Option<DevFlowProjectDiagnostics>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevFlowCliDiagnostics {
    pub available: bool,
    pub executable: Option<String>,
    pub version: Option<String>,
    pub source: Option<String>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevFlowProjectDiagnostics {
    pub session_id: String,
    pub root: Option<String>,
    pub revision: Option<String>,
    pub availability: String,
    pub message: Option<String>,
    pub dashboard_url: Option<String>,
    pub snapshot_revision: Option<u64>,
    pub last_sync_unix_ms: Option<u64>,
    pub memory_use_bytes: Option<u64>,
}

/// System-backed runtime using process discovery, used by the app and tests.
pub fn system_runtime(
    notifier: SharedNotificationSink,
    project_runner: Arc<dyn ProjectCommandRunner>,
    sidebar_refresh: SharedSidebarRefreshSink,
) -> Arc<DevFlowRuntime> {
    DevFlowRuntime::new_with_sidebar_refresh(
        notifier.clone(),
        project_runner,
        Arc::new(SystemCommandRunner),
        DiscoveryEnvironment::from_process(),
        real_factory_provider(notifier, sidebar_refresh.clone()),
        sidebar_refresh,
    )
}

pub fn real_factory_provider(
    notifier: SharedNotificationSink,
    sidebar_refresh: SharedSidebarRefreshSink,
) -> DashboardFactoryProvider {
    Arc::new(move |executable| {
        Arc::new(RealDashboardServiceFactory::new(
            executable.to_path_buf(),
            notifier.clone(),
            sidebar_refresh.clone(),
        ))
    })
}

/// Real Tauri emitter sink bound to the running application.
pub fn real_notifier(app: tauri::AppHandle) -> NotificationSink {
    Arc::new(move |event| {
        let _ = emit_notification(&app, event);
    })
}

/// Real sidebar refresh sink bound to the running application. Invoked after
/// dashboard snapshot changes so the sidebar summary stays current without
/// polling; failures are logged by the emitter, never surfaced as toasts.
pub fn real_sidebar_refresher(app: tauri::AppHandle, gui_state: GuiState) -> SidebarRefreshSink {
    Arc::new(move || {
        let app = app.clone();
        let gui_state = gui_state.clone();
        tokio::spawn(async move {
            let _ = crate::events::emit_sidebar_state(&app, &gui_state).await;
        });
    })
}
