// FrameworkTree
// dev_flow.rs
// ├── struct RuntimeDiagnostics
// ├── struct DevFlowRuntimeInner
// ├── struct DynDiscoveryRunner
// ├── impl DynDiscoveryRunner
// ├── run()
// ├── struct DevFlowRuntime
// ├── _assert_registry_send_sync()
// ├── assert_send_sync()
// ├── impl DevFlowRuntime
// ├── new()
// ├── attach_notifier()
// ├── shutdown()
// ├── reconfigure()
// ├── session_started()
// ├── session_resumed()
// ├── session_finished()
// ├── session_closed()
// ├── switch_to_session()
// ├── on_successful_bash()
// ├── diagnostics()
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
// ├── maybe_report_connection_error()
// ├── project_hash()
// ├── install_source_label()
// ├── revision_label()
// ├── availability_label()
// ├── struct DevFlowSettingsSnapshot
// ├── struct DevFlowCliDiagnostics
// ├── struct DevFlowProjectDiagnostics
// ├── system_runtime()
// ├── real_factory_provider()
// └── real_notifier()

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
    DashboardServiceFactory, DevFlowAvailability, DevFlowError, DevFlowProjectKey, DevFlowRegistry,
    DevFlowRevisionKey, DevFlowServiceHandle, DevFlowSnapshot, DiscoveryCommandRunner,
    DiscoveryEnvironment, DowDiscoveryError, DowInstallSource, ProjectCommandRunner,
    SessionDevFlowState, SystemCommandRunner, discover_dow_with, start_dashboard,
};
use rozsa_app::settings::DevFlowSettings;
use serde::Serialize;
use tokio::sync::Mutex;
use tokio::time::{MissedTickBehavior, sleep};
use tokio_util::sync::CancellationToken;

use crate::notifications::{AppNotificationEvent, NotificationSeverity, emit_notification};

pub const CLI_ERROR_ID: &str = "dev-flow.cli";
pub const DASHBOARD_START_PREFIX: &str = "dev-flow.dashboard-start:";
pub const CONNECTION_PREFIX: &str = "dev-flow.connection:";

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

#[derive(Clone, Debug, Default)]
struct RuntimeDiagnostics {
    cli_error: Option<String>,
    executable: Option<PathBuf>,
    version: Option<String>,
    source: Option<DowInstallSource>,
}

struct DevFlowRuntimeInner {
    notifier: SharedNotificationSink,
    project_runner: Arc<dyn ProjectCommandRunner>,
    discovery_runner: Arc<dyn DiscoveryCommandRunner>,
    discovery_environment: DiscoveryEnvironment,
    factory_provider: DashboardFactoryProvider,
    registry: Mutex<Option<Arc<DevFlowRegistry>>>,
    sessions: Mutex<HashMap<String, PathBuf>>,
    current_session: Mutex<Option<String>>,
    diagnostics: Mutex<RuntimeDiagnostics>,
    project_errors: Mutex<std::collections::HashSet<String>>,
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
        let inner = Arc::new(DevFlowRuntimeInner {
            notifier,
            project_runner,
            discovery_runner,
            discovery_environment,
            factory_provider,
            registry: Mutex::new(None),
            sessions: Mutex::new(HashMap::new()),
            current_session: Mutex::new(None),
            diagnostics: Mutex::new(RuntimeDiagnostics::default()),
            project_errors: Mutex::new(std::collections::HashSet::new()),
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
            let changed = sessions.get(session_id) != Some(&cwd);
            if changed {
                sessions.insert(session_id.to_owned(), cwd.clone());
            }
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
        if let Some(registry) = self.inner.registry.lock().await.clone() {
            registry.session_active(session_id).await;
        }
    }

    pub async fn session_finished(&self, session_id: &str, stopped_at: SystemTime) {
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
            .insert(session_id.to_owned(), cwd.clone());
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
            .insert(session_id.to_owned(), cwd.clone());
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
        let diag = self.inner.diagnostics.lock().await.clone();
        let project = {
            let current = self.inner.current_session.lock().await.clone();
            let registry = self.inner.registry.lock().await.clone();
            match (current, registry) {
                (Some(session_id), Some(registry)) => {
                    registry.session_state(&session_id).await.map(|state| {
                        let (availability, message) = availability_label(&state.availability);
                        DevFlowProjectDiagnostics {
                            session_id,
                            root: Some(state.project.root.to_string_lossy().into_owned()),
                            revision: Some(revision_label(&state.project.revision)),
                            availability,
                            message,
                        }
                    })
                }
                _ => None,
            }
        };
        DevFlowSettingsSnapshot {
            enabled: settings.enabled,
            show_sidebar_status: settings.show_sidebar_status,
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

        let mut registry_guard = self.inner.registry.lock().await;
        let adopted = self.inner.diagnostics.lock().await.executable.clone();
        let restart = match (&discovered, &*registry_guard) {
            (Some(found), Some(_)) => adopted.as_ref() != Some(&found.executable),
            (None, Some(_)) | (Some(_), None) => true,
            (None, None) => false,
        };
        if restart && let Some(old) = registry_guard.take() {
            let report = old.shutdown_all().await;
            tracing::info!(
                "dev-flow: stopped {} owned dashboard services ({} remain protected)",
                report.terminated.len(),
                report.still_running.len()
            );
        }

        match &discovered {
            Some(found) => {
                if restart {
                    let factory = (self.inner.factory_provider)(&found.executable);
                    let registry = Arc::new(DevFlowRegistry::new(
                        found.executable.clone(),
                        self.inner.project_runner.clone(),
                        factory,
                    ));
                    let sessions = self.inner.sessions.lock().await.clone();
                    for (session_id, cwd) in sessions {
                        if let Ok(state) = registry.associate_session(&session_id, cwd).await {
                            self.note_state(state).await;
                        }
                    }
                    *registry_guard = Some(registry);
                } else if let Some(registry) = registry_guard.as_ref() {
                    let sessions = self.inner.sessions.lock().await.clone();
                    for (session_id, cwd) in sessions {
                        if let Ok(state) = registry.associate_session(&session_id, cwd).await {
                            self.note_state(state).await;
                        }
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
                *registry_guard = None;
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
        drop(registry_guard);
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
            current.and_then(|session_id| sessions.get(&session_id).cloned())
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
                if errors.insert(id.clone()) {
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
                if errors.remove(&id) {
                    drop(errors);
                    self.emit(AppNotificationEvent::Resolve { id }).await;
                }
            }
        }
    }

    async fn record_cli_error(&self, error: DowDiscoveryError) {
        let message = error.to_string();
        {
            let mut diag = self.inner.diagnostics.lock().await;
            if diag.cli_error.is_some() {
                diag.cli_error = Some(message.clone());
                return;
            }
            diag.cli_error = Some(message.clone());
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
        for id in ids {
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
    ports: RangeInclusive<u16>,
}

impl RealDashboardServiceFactory {
    pub fn new(executable: PathBuf, notifier: SharedNotificationSink) -> Self {
        Self {
            executable,
            timing: DashboardTiming::default(),
            notifier,
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
        let stream_cancellation = cancellation.clone();
        let connection_id = format!("{CONNECTION_PREFIX}{project_id}");
        let notifier = self.notifier.clone();
        tokio::spawn(async move {
            run_connection_loop(
                &stream_client,
                stream_snapshot,
                &stream_cancellation,
                &notifier,
                &connection_id,
            )
            .await;
        });

        let control = Arc::new(DashboardProcessControl {
            process: Mutex::new(Some(process)),
            cancellation,
        });
        let id = NEXT_SERVICE_ID.fetch_add(1, Ordering::Relaxed);
        Ok(DevFlowServiceHandle::with_child(id, snapshot, control, pid))
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
                    break;
                }
                Err(DevFlowError::Cancelled) => return,
                Err(_) => {
                    mark_stale(&snapshot).await;
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

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevFlowSettingsSnapshot {
    pub enabled: bool,
    pub show_sidebar_status: bool,
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
}

/// System-backed runtime using process discovery, used by the app and tests.
pub fn system_runtime(
    notifier: SharedNotificationSink,
    project_runner: Arc<dyn ProjectCommandRunner>,
) -> Arc<DevFlowRuntime> {
    DevFlowRuntime::new(
        notifier.clone(),
        project_runner,
        Arc::new(SystemCommandRunner),
        DiscoveryEnvironment::from_process(),
        real_factory_provider(notifier),
    )
}

pub fn real_factory_provider(notifier: SharedNotificationSink) -> DashboardFactoryProvider {
    Arc::new(move |executable| {
        Arc::new(RealDashboardServiceFactory::new(
            executable.to_path_buf(),
            notifier.clone(),
        ))
    })
}

/// Real Tauri emitter sink bound to the running application.
pub fn real_notifier(app: tauri::AppHandle) -> NotificationSink {
    Arc::new(move |event| {
        let _ = emit_notification(&app, event);
    })
}
