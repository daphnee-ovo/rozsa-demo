// FrameworkTree
// dashboard.rs
// ├── struct DashboardTiming
// ├── impl DashboardTiming
// ├── default()
// ├── struct DevFlowProjectStatus
// ├── enum DevFlowTaskStatus
// ├── struct DevFlowTask
// ├── enum DevFlowIssueStatus
// ├── struct DevFlowIssue
// ├── struct DevFlowSnapshot
// ├── impl DevFlowSnapshot
// ├── mark_stale()
// ├── enum DevFlowError
// ├── struct DashboardClient
// ├── impl DashboardClient
// ├── new()
// ├── with_timing()
// ├── base_url()
// ├── fetch_snapshot()
// ├── fetch_snapshot_cancellable()
// ├── subscribe()
// ├── subscribe_cancellable()
// ├── fetch_json()
// ├── send_get()
// ├── decode_snapshot()
// ├── record_result()
// ├── last_snapshot()
// ├── struct DevFlowEventStream
// ├── impl DevFlowEventStream
// ├── next_snapshot()
// ├── mark_last_stale()
// ├── struct SseDecoder
// ├── impl SseDecoder
// ├── push()
// ├── finish()
// ├── process_line()
// ├── struct DashboardUpdateDto
// ├── struct DashboardStatusDto
// ├── struct DashboardTasksDto
// ├── struct DashboardIssuesDto
// ├── struct DashboardTaskFilesDto
// ├── struct DashboardIssueFilesDto
// ├── struct DashboardTaskDto
// ├── struct DashboardIssueDto
// ├── decode_snapshot()
// ├── impl DevFlowTask
// ├── try_from()
// ├── impl DevFlowIssue
// ├── try_from()
// ├── normalize_id()
// ├── read_bounded()
// ├── struct ReconnectBackoff
// ├── impl ReconnectBackoff
// ├── next_delay()
// ├── reset()
// ├── should_report_error()
// ├── struct DashboardProcess
// ├── impl DashboardProcess
// ├── id()
// ├── stderr()
// ├── shutdown()
// ├── wait_for_graceful_exit()
// ├── start_dashboard()
// ├── start_dashboard_with_delay()
// ├── port_is_available()
// ├── capture_stderr()
// ├── cleanup_child()
// ├── struct ProcessGroupGuard
// ├── impl ProcessGroupGuard
// ├── new()
// ├── terminate()
// ├── disarm()
// ├── impl ProcessGroupGuard
// └── drop()

//! Bounded, read-only access to the dev-flow dashboard service.

use std::collections::VecDeque;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::ops::RangeInclusive;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

use reqwest::{Client, Response, StatusCode, Url};
use serde::Deserialize;
use thiserror::Error;
use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;
use tokio::time::{Instant, sleep, timeout};
use tokio_util::sync::CancellationToken;

const MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024;
const MAX_STDERR_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug)]
pub struct DashboardTiming {
    pub connect_timeout: Duration,
    pub request_timeout: Duration,
    pub stream_stall_timeout: Duration,
    pub startup_timeout: Duration,
    pub startup_poll_interval: Duration,
}

impl Default for DashboardTiming {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(1),
            request_timeout: Duration::from_secs(5),
            stream_stall_timeout: Duration::from_secs(45),
            startup_timeout: Duration::from_secs(5),
            startup_poll_interval: Duration::from_millis(50),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DevFlowProjectStatus {
    pub name: Option<String>,
    pub phase: Option<String>,
    pub mode: Option<String>,
    pub version: Option<String>,
    pub goals_minor: Option<String>,
    pub updated: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DevFlowTaskStatus {
    Pending,
    InProgress,
    Done,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DevFlowTask {
    pub id: String,
    pub title: String,
    pub status: DevFlowTaskStatus,
    pub priority: Option<String>,
    pub complexity: Option<String>,
    pub task_type: Option<String>,
    pub refs: Option<String>,
    pub depends_on: Vec<String>,
    pub done_when: Vec<String>,
    pub files_create: Vec<String>,
    pub files_modify: Vec<String>,
    pub files_test: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DevFlowIssueStatus {
    Open,
    InProgress,
    Closed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DevFlowIssue {
    pub id: String,
    pub title: String,
    pub status: DevFlowIssueStatus,
    pub severity: Option<String>,
    pub description: Option<String>,
    pub files_create: Vec<String>,
    pub files_modify: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct DevFlowSnapshot {
    pub revision: u64,
    pub project: DevFlowProjectStatus,
    pub tasks: Vec<DevFlowTask>,
    pub issues: Vec<DevFlowIssue>,
    pub received_at: SystemTime,
    pub stale: bool,
}

impl DevFlowSnapshot {
    pub fn mark_stale(&mut self) {
        self.stale = true;
    }
}

#[derive(Debug, Error)]
pub enum DevFlowError {
    #[error("dashboard URL must use loopback HTTP: {0}")]
    NonLoopbackUrl(Url),
    #[error("dashboard request was cancelled")]
    Cancelled,
    #[error("dashboard request timed out after {0:?}")]
    Timeout(Duration),
    #[error("dashboard connection stalled for {0:?}")]
    StreamStalled(Duration),
    #[error("dashboard request failed: {0}")]
    Request(String),
    #[error("dashboard returned HTTP {0}")]
    HttpStatus(StatusCode),
    #[error("dashboard response exceeded 16 MiB")]
    ResponseTooLarge,
    #[error("dashboard API is incompatible: {0}")]
    IncompatibleApi(String),
    #[error("no dashboard port is available in the configured range")]
    NoAvailablePort,
    #[error("failed to start dashboard: {0}")]
    Startup(String),
    #[error("dashboard process {pid:?} did not become ready within {timeout:?}: {stderr}")]
    StartupTimeout {
        timeout: Duration,
        pid: Option<u32>,
        stderr: String,
    },
}

#[derive(Clone)]
pub struct DashboardClient {
    base_url: Url,
    http: Client,
    timing: DashboardTiming,
    revision: Arc<AtomicU64>,
    last_good: Arc<RwLock<Option<DevFlowSnapshot>>>,
}

impl DashboardClient {
    pub fn new(base_url: Url) -> Result<Self, DevFlowError> {
        Self::with_timing(base_url, DashboardTiming::default())
    }

    pub fn with_timing(mut base_url: Url, timing: DashboardTiming) -> Result<Self, DevFlowError> {
        if base_url.scheme() != "http"
            || !base_url
                .host_str()
                .and_then(|host| host.parse::<IpAddr>().ok())
                .is_some_and(|address| address.is_loopback())
        {
            return Err(DevFlowError::NonLoopbackUrl(base_url));
        }
        base_url.set_path("/");
        base_url.set_query(None);
        base_url.set_fragment(None);
        let http = Client::builder()
            .connect_timeout(timing.connect_timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| DevFlowError::Request(error.to_string()))?;
        Ok(Self {
            base_url,
            http,
            timing,
            revision: Arc::new(AtomicU64::new(0)),
            last_good: Arc::new(RwLock::new(None)),
        })
    }

    pub fn base_url(&self) -> &Url {
        &self.base_url
    }

    pub async fn fetch_snapshot(&self) -> Result<DevFlowSnapshot, DevFlowError> {
        self.fetch_snapshot_cancellable(&CancellationToken::new())
            .await
    }

    pub async fn fetch_snapshot_cancellable(
        &self,
        cancellation: &CancellationToken,
    ) -> Result<DevFlowSnapshot, DevFlowError> {
        let fetch = async {
            let status = self.fetch_json("api/v1/status", cancellation).await?;
            let tasks = self.fetch_json("api/v1/tasks", cancellation).await?;
            let issues = self.fetch_json("api/v1/issues", cancellation).await?;
            self.decode_snapshot(status, tasks, issues)
        };
        let result = tokio::select! {
            _ = cancellation.cancelled() => Err(DevFlowError::Cancelled),
            result = timeout(self.timing.request_timeout, fetch) => {
                result.map_err(|_| DevFlowError::Timeout(self.timing.request_timeout))?
            }
        };
        self.record_result(result).await
    }

    pub async fn subscribe(&self) -> Result<DevFlowEventStream, DevFlowError> {
        self.subscribe_cancellable(&CancellationToken::new()).await
    }

    pub async fn subscribe_cancellable(
        &self,
        cancellation: &CancellationToken,
    ) -> Result<DevFlowEventStream, DevFlowError> {
        let response = self
            .send_get("api/v1/events", self.timing.request_timeout, cancellation)
            .await?;
        Ok(DevFlowEventStream {
            response,
            decoder: SseDecoder::default(),
            pending: VecDeque::new(),
            timing: self.timing,
            client: self.clone(),
        })
    }

    async fn fetch_json<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        cancellation: &CancellationToken,
    ) -> Result<T, DevFlowError> {
        let response = self
            .send_get(path, self.timing.request_timeout, cancellation)
            .await?;
        let bytes = read_bounded(response, self.timing.request_timeout, cancellation).await?;
        serde_json::from_slice(&bytes)
            .map_err(|error| DevFlowError::IncompatibleApi(error.to_string()))
    }

    async fn send_get(
        &self,
        path: &str,
        deadline: Duration,
        cancellation: &CancellationToken,
    ) -> Result<Response, DevFlowError> {
        let url = self
            .base_url
            .join(path)
            .map_err(|error| DevFlowError::Request(error.to_string()))?;
        let send = self.http.get(url).send();
        let response = tokio::select! {
            _ = cancellation.cancelled() => return Err(DevFlowError::Cancelled),
            result = timeout(deadline, send) => {
                result.map_err(|_| DevFlowError::Timeout(deadline))?
                    .map_err(|error| DevFlowError::Request(error.to_string()))?
            }
        };
        if !response.status().is_success() {
            return Err(DevFlowError::HttpStatus(response.status()));
        }
        Ok(response)
    }

    fn decode_snapshot(
        &self,
        status: DashboardStatusDto,
        tasks: DashboardTasksDto,
        issues: DashboardIssuesDto,
    ) -> Result<DevFlowSnapshot, DevFlowError> {
        let mut snapshot = decode_snapshot(status, tasks, issues, 0)?;
        snapshot.revision = self.revision.fetch_add(1, Ordering::Relaxed) + 1;
        Ok(snapshot)
    }

    async fn record_result(
        &self,
        result: Result<DevFlowSnapshot, DevFlowError>,
    ) -> Result<DevFlowSnapshot, DevFlowError> {
        match result {
            Ok(snapshot) => {
                *self.last_good.write().await = Some(snapshot.clone());
                Ok(snapshot)
            }
            Err(error) => {
                if let Some(snapshot) = self.last_good.write().await.as_mut() {
                    snapshot.mark_stale();
                }
                Err(error)
            }
        }
    }

    pub async fn last_snapshot(&self) -> Option<DevFlowSnapshot> {
        self.last_good.read().await.clone()
    }
}

pub struct DevFlowEventStream {
    response: Response,
    decoder: SseDecoder,
    pending: VecDeque<Vec<u8>>,
    timing: DashboardTiming,
    client: DashboardClient,
}

impl DevFlowEventStream {
    pub async fn next_snapshot(
        &mut self,
        cancellation: &CancellationToken,
    ) -> Result<Option<DevFlowSnapshot>, DevFlowError> {
        loop {
            if let Some(data) = self.pending.pop_front() {
                let update: DashboardUpdateDto = match serde_json::from_slice(&data) {
                    Ok(update) => update,
                    Err(error) => {
                        self.mark_last_stale().await;
                        return Err(DevFlowError::IncompatibleApi(error.to_string()));
                    }
                };
                if !matches!(
                    update.resource.as_str(),
                    "all" | "status" | "tasks" | "issues"
                ) {
                    self.mark_last_stale().await;
                    return Err(DevFlowError::IncompatibleApi(format!(
                        "unknown update resource `{}`",
                        update.resource
                    )));
                }
                return self
                    .client
                    .fetch_snapshot_cancellable(cancellation)
                    .await
                    .map(Some);
            }
            let chunk = tokio::select! {
                _ = cancellation.cancelled() => {
                    self.mark_last_stale().await;
                    return Err(DevFlowError::Cancelled);
                },
                result = timeout(self.timing.stream_stall_timeout, self.response.chunk()) => {
                    match result {
                        Ok(Ok(chunk)) => chunk,
                        Ok(Err(error)) => {
                            self.mark_last_stale().await;
                            return Err(DevFlowError::Request(error.to_string()));
                        }
                        Err(_) => {
                            self.mark_last_stale().await;
                            return Err(DevFlowError::StreamStalled(self.timing.stream_stall_timeout));
                        }
                    }
                }
            };
            let Some(chunk) = chunk else {
                self.decoder.finish()?;
                self.mark_last_stale().await;
                return Ok(None);
            };
            self.pending.extend(self.decoder.push(&chunk)?);
        }
    }

    async fn mark_last_stale(&self) {
        if let Some(snapshot) = self.client.last_good.write().await.as_mut() {
            snapshot.mark_stale();
        }
    }
}

#[derive(Default)]
struct SseDecoder {
    buffer: Vec<u8>,
    event: Option<String>,
    data: Vec<String>,
    data_bytes: usize,
}

impl SseDecoder {
    fn push(&mut self, chunk: &[u8]) -> Result<Vec<Vec<u8>>, DevFlowError> {
        self.buffer.extend_from_slice(chunk);
        if self.buffer.len() > MAX_RESPONSE_BYTES {
            return Err(DevFlowError::ResponseTooLarge);
        }
        let mut updates = Vec::new();
        while let Some(position) = self.buffer.iter().position(|byte| *byte == b'\n') {
            let mut line = self.buffer.drain(..=position).collect::<Vec<_>>();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            self.process_line(&line, &mut updates)?;
        }
        Ok(updates)
    }

    fn finish(&mut self) -> Result<(), DevFlowError> {
        if !self.buffer.is_empty() {
            let line = std::mem::take(&mut self.buffer);
            self.process_line(&line, &mut Vec::new())?;
        }
        Ok(())
    }

    fn process_line(
        &mut self,
        line: &[u8],
        updates: &mut Vec<Vec<u8>>,
    ) -> Result<(), DevFlowError> {
        let line = std::str::from_utf8(line)
            .map_err(|error| DevFlowError::IncompatibleApi(error.to_string()))?;
        if line.is_empty() {
            if self.event.as_deref() == Some("update") && !self.data.is_empty() {
                updates.push(self.data.join("\n").into_bytes());
            }
            self.event = None;
            self.data.clear();
            self.data_bytes = 0;
            return Ok(());
        }
        if line.starts_with(':') {
            return Ok(());
        }
        let (field, value) = line
            .split_once(':')
            .map(|(field, value)| (field, value.strip_prefix(' ').unwrap_or(value)))
            .unwrap_or((line, ""));
        match field {
            "event" => self.event = Some(value.to_owned()),
            "data" => {
                self.data_bytes = self.data_bytes.saturating_add(value.len());
                if self.data_bytes > MAX_RESPONSE_BYTES {
                    return Err(DevFlowError::ResponseTooLarge);
                }
                self.data.push(value.to_owned());
            }
            _ => {}
        }
        Ok(())
    }
}

#[derive(Deserialize)]
struct DashboardUpdateDto {
    resource: String,
}

#[derive(Deserialize)]
struct DashboardStatusDto {
    name: Option<String>,
    phase: Option<String>,
    mode: Option<String>,
    version: Option<String>,
    goals_minor: Option<String>,
    updated: Option<String>,
}

#[derive(Deserialize)]
struct DashboardTasksDto {
    items: Vec<DashboardTaskDto>,
}

#[derive(Deserialize)]
struct DashboardIssuesDto {
    items: Vec<DashboardIssueDto>,
}

#[derive(Deserialize)]
struct DashboardTaskFilesDto {
    create: Vec<String>,
    modify: Vec<String>,
    test: Vec<String>,
}

#[derive(Deserialize)]
struct DashboardIssueFilesDto {
    create: Vec<String>,
    modify: Vec<String>,
}

#[derive(Deserialize)]
struct DashboardTaskDto {
    id: String,
    title: String,
    status: String,
    priority: Option<String>,
    complexity: Option<String>,
    #[serde(rename = "type")]
    task_type: Option<String>,
    refs: Option<String>,
    #[serde(default)]
    depends_on: Vec<String>,
    #[serde(default)]
    done_when: Vec<String>,
    files: DashboardTaskFilesDto,
}

#[derive(Deserialize)]
struct DashboardIssueDto {
    id: String,
    title: String,
    status: String,
    severity: Option<String>,
    description: Option<String>,
    files: DashboardIssueFilesDto,
}

fn decode_snapshot(
    status: DashboardStatusDto,
    tasks: DashboardTasksDto,
    issues: DashboardIssuesDto,
    revision: u64,
) -> Result<DevFlowSnapshot, DevFlowError> {
    let tasks = tasks
        .items
        .into_iter()
        .map(DevFlowTask::try_from)
        .collect::<Result<Vec<_>, _>>()?;
    let issues = issues
        .items
        .into_iter()
        .map(DevFlowIssue::try_from)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(DevFlowSnapshot {
        revision,
        project: DevFlowProjectStatus {
            name: status.name,
            phase: status.phase,
            mode: status.mode,
            version: status.version,
            goals_minor: status.goals_minor,
            updated: status.updated,
        },
        tasks,
        issues,
        received_at: SystemTime::now(),
        stale: false,
    })
}

impl TryFrom<DashboardTaskDto> for DevFlowTask {
    type Error = DevFlowError;

    fn try_from(dto: DashboardTaskDto) -> Result<Self, Self::Error> {
        let id = normalize_id(&dto.id, "TASK-T")?;
        let status = match dto.status.as_str() {
            "pending" => DevFlowTaskStatus::Pending,
            "in_progress" => DevFlowTaskStatus::InProgress,
            "done" => DevFlowTaskStatus::Done,
            other => {
                return Err(DevFlowError::IncompatibleApi(format!(
                    "unknown task status `{other}`"
                )));
            }
        };
        Ok(Self {
            id,
            title: dto.title,
            status,
            priority: dto.priority,
            complexity: dto.complexity,
            task_type: dto.task_type,
            refs: dto.refs,
            depends_on: dto.depends_on,
            done_when: dto.done_when,
            files_create: dto.files.create,
            files_modify: dto.files.modify,
            files_test: dto.files.test,
        })
    }
}

impl TryFrom<DashboardIssueDto> for DevFlowIssue {
    type Error = DevFlowError;

    fn try_from(dto: DashboardIssueDto) -> Result<Self, Self::Error> {
        let id = normalize_id(&dto.id, "ISSUE-I")?;
        let status = match dto.status.as_str() {
            "open" => DevFlowIssueStatus::Open,
            "in_progress" => DevFlowIssueStatus::InProgress,
            "closed" => DevFlowIssueStatus::Closed,
            other => {
                return Err(DevFlowError::IncompatibleApi(format!(
                    "unknown issue status `{other}`"
                )));
            }
        };
        Ok(Self {
            id,
            title: dto.title,
            status,
            severity: dto.severity,
            description: dto.description,
            files_create: dto.files.create,
            files_modify: dto.files.modify,
        })
    }
}

/// Extracts the canonical item id (`TASK-T007`, `ISSUE-I001`) from the
/// dashboard value. The real dow dashboard may append title text after the id
/// when an issue entry is malformed. Only a known title delimiter (whitespace,
/// `:`, or `：`) may follow the canonical digits; arbitrary suffixes are rejected
/// so two distinct source values cannot silently collapse to one identity.
fn normalize_id(id: &str, prefix: &str) -> Result<String, DevFlowError> {
    let Some(rest) = id.strip_prefix(prefix) else {
        return Err(DevFlowError::IncompatibleApi(format!(
            "invalid item id `{id}`"
        )));
    };
    let digits: String = rest.chars().take_while(|ch| ch.is_ascii_digit()).collect();
    if digits.is_empty() {
        return Err(DevFlowError::IncompatibleApi(format!(
            "invalid item id `{id}`"
        )));
    }
    let suffix = &rest[digits.len()..];
    if !suffix.is_empty()
        && !suffix
            .chars()
            .next()
            .is_some_and(|ch| ch.is_whitespace() || matches!(ch, ':' | '：'))
    {
        return Err(DevFlowError::IncompatibleApi(format!(
            "invalid item id `{id}`"
        )));
    }
    Ok(format!("{prefix}{digits}"))
}

async fn read_bounded(
    mut response: Response,
    deadline: Duration,
    cancellation: &CancellationToken,
) -> Result<Vec<u8>, DevFlowError> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
    {
        return Err(DevFlowError::ResponseTooLarge);
    }
    let read = async {
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|error| DevFlowError::Request(error.to_string()))?
        {
            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
                return Err(DevFlowError::ResponseTooLarge);
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    };
    tokio::select! {
        _ = cancellation.cancelled() => Err(DevFlowError::Cancelled),
        result = timeout(deadline, read) => result.map_err(|_| DevFlowError::Timeout(deadline))?,
    }
}

#[derive(Clone, Debug, Default)]
pub struct ReconnectBackoff {
    attempt: usize,
}

impl ReconnectBackoff {
    pub fn next_delay(&mut self) -> Duration {
        const SECONDS: [u64; 6] = [1, 2, 4, 8, 16, 30];
        let delay = SECONDS[self.attempt.min(SECONDS.len() - 1)];
        self.attempt = self.attempt.saturating_add(1);
        Duration::from_secs(delay)
    }

    pub fn reset(&mut self) {
        self.attempt = 0;
    }

    pub fn should_report_error(&self, disconnected_for: Duration) -> bool {
        self.attempt >= 3 || disconnected_for >= Duration::from_secs(7)
    }
}

pub struct DashboardProcess {
    child: Child,
    process_group: ProcessGroupGuard,
    stderr: Arc<Mutex<Vec<u8>>>,
    stderr_task: JoinHandle<()>,
    pub client: DashboardClient,
    pub initial_snapshot: DevFlowSnapshot,
    pub port: u16,
}

impl DashboardProcess {
    pub fn id(&self) -> Option<u32> {
        self.child.id()
    }

    pub async fn stderr(&self) -> String {
        String::from_utf8_lossy(&self.stderr.lock().await).into_owned()
    }

    pub async fn shutdown(mut self) -> Result<(), DevFlowError> {
        if self
            .child
            .try_wait()
            .map_err(|error| DevFlowError::Startup(error.to_string()))?
            .is_none()
        {
            self.process_group.terminate();
            let _ = self.child.start_kill();
        } else {
            self.process_group.disarm();
        }
        self.child
            .wait()
            .await
            .map_err(|error| DevFlowError::Startup(error.to_string()))?;
        let _ = self.stderr_task.await;
        Ok(())
    }

    /// Wait up to `grace` for the child to exit on its own (for example its
    /// no-client shutdown window). Returns true when the child exited and was
    /// reaped; the process-group guard is disarmed so a later drop or
    /// [`DashboardProcess::shutdown`] is a no-op. Returns false when the child
    /// is still running after `grace`.
    pub async fn wait_for_graceful_exit(&mut self, grace: Duration) -> bool {
        let exited = match timeout(grace, self.child.wait()).await {
            Ok(Ok(_)) => true,
            Ok(Err(_)) | Err(_) => false,
        };
        if exited {
            self.process_group.disarm();
        }
        exited
    }
}

pub async fn start_dashboard(
    executable: &Path,
    project_root: &Path,
    ports: RangeInclusive<u16>,
    timing: DashboardTiming,
    cancellation: &CancellationToken,
) -> Result<DashboardProcess, DevFlowError> {
    start_dashboard_with_delay(
        executable,
        project_root,
        ports,
        timing,
        cancellation,
        Duration::ZERO,
    )
    .await
}

/// Test-only seam: delays each child spawn so startup-window semantics can be
/// verified deterministically without load-dependent races.
#[doc(hidden)]
pub async fn start_dashboard_with_delay(
    executable: &Path,
    project_root: &Path,
    ports: RangeInclusive<u16>,
    timing: DashboardTiming,
    cancellation: &CancellationToken,
    spawn_delay: Duration,
) -> Result<DashboardProcess, DevFlowError> {
    let mut last_failure = None;
    for port in ports {
        if !port_is_available(port).await {
            continue;
        }
        if !spawn_delay.is_zero() {
            sleep(spawn_delay).await;
        }
        let mut command = Command::new(executable);
        command
            .arg("dashboard")
            .arg("--port")
            .arg(port.to_string())
            .arg("--no-open")
            .current_dir(project_root)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        #[cfg(unix)]
        command.process_group(0);
        let mut child = command
            .spawn()
            .map_err(|error| DevFlowError::Startup(error.to_string()))?;
        let mut process_group = ProcessGroupGuard::new(&child);
        let stderr = Arc::new(Mutex::new(Vec::new()));
        let stderr_task = capture_stderr(child.stderr.take(), Arc::clone(&stderr));
        let base_url = Url::parse(&format!("http://127.0.0.1:{port}/"))
            .map_err(|error| DevFlowError::Startup(error.to_string()))?;
        let client = DashboardClient::with_timing(base_url, timing)?;
        let deadline = Instant::now() + timing.startup_timeout;
        loop {
            if cancellation.is_cancelled() {
                cleanup_child(&mut child, &mut process_group, stderr_task).await;
                return Err(DevFlowError::Cancelled);
            }
            if child
                .try_wait()
                .map_err(|error| DevFlowError::Startup(error.to_string()))?
                .is_some()
            {
                process_group.disarm();
                let _ = stderr_task.await;
                last_failure = Some(String::from_utf8_lossy(&stderr.lock().await).into_owned());
                break;
            }
            if Instant::now() >= deadline {
                let pid = child.id();
                cleanup_child(&mut child, &mut process_group, stderr_task).await;
                return Err(DevFlowError::StartupTimeout {
                    timeout: timing.startup_timeout,
                    pid,
                    stderr: String::from_utf8_lossy(&stderr.lock().await).into_owned(),
                });
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            match timeout(remaining, client.fetch_snapshot_cancellable(cancellation)).await {
                Ok(Ok(initial_snapshot)) => {
                    return Ok(DashboardProcess {
                        child,
                        process_group,
                        stderr,
                        stderr_task,
                        client,
                        initial_snapshot,
                        port,
                    });
                }
                Ok(Err(DevFlowError::Cancelled)) => {
                    cleanup_child(&mut child, &mut process_group, stderr_task).await;
                    return Err(DevFlowError::Cancelled);
                }
                Ok(Err(_)) => {
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    sleep(timing.startup_poll_interval.min(remaining)).await;
                }
                Err(_) => {
                    let pid = child.id();
                    cleanup_child(&mut child, &mut process_group, stderr_task).await;
                    return Err(DevFlowError::StartupTimeout {
                        timeout: timing.startup_timeout,
                        pid,
                        stderr: String::from_utf8_lossy(&stderr.lock().await).into_owned(),
                    });
                }
            }
        }
    }
    Err(last_failure
        .map(DevFlowError::Startup)
        .unwrap_or(DevFlowError::NoAvailablePort))
}

async fn port_is_available(port: u16) -> bool {
    tokio::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))
        .await
        .is_ok()
}

fn capture_stderr(
    stderr: Option<tokio::process::ChildStderr>,
    captured: Arc<Mutex<Vec<u8>>>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let Some(mut stderr) = stderr else {
            return;
        };
        let mut buffer = [0u8; 4096];
        loop {
            let Ok(read) = stderr.read(&mut buffer).await else {
                return;
            };
            if read == 0 {
                return;
            }
            let mut captured = captured.lock().await;
            let remaining = MAX_STDERR_BYTES.saturating_sub(captured.len());
            captured.extend_from_slice(&buffer[..read.min(remaining)]);
        }
    })
}

async fn cleanup_child(
    child: &mut Child,
    process_group: &mut ProcessGroupGuard,
    stderr_task: JoinHandle<()>,
) {
    if child.try_wait().ok().flatten().is_none() {
        process_group.terminate();
        let _ = child.start_kill();
    } else {
        process_group.disarm();
    }
    let _ = child.wait().await;
    let _ = stderr_task.await;
}

struct ProcessGroupGuard {
    #[cfg(unix)]
    pid: Option<u32>,
}

impl ProcessGroupGuard {
    fn new(child: &Child) -> Self {
        Self {
            #[cfg(unix)]
            pid: child.id(),
        }
    }

    fn terminate(&mut self) {
        #[cfg(unix)]
        if let Some(pid) = self.pid.take() {
            let _ = std::process::Command::new("/bin/kill")
                .args(["-KILL", &format!("-{pid}")])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }
    }

    fn disarm(&mut self) {
        #[cfg(unix)]
        {
            self.pid = None;
        }
    }
}

impl Drop for ProcessGroupGuard {
    fn drop(&mut self) {
        self.terminate();
    }
}
