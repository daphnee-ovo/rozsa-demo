// FrameworkTree
// dev_flow_dashboard_test.rs
// ├── struct ResponsePlan
// ├── spawn_server()
// ├── json_response()
// ├── rest_plans()
// ├── fast_timing()
// ├── snapshot_adapter_accepts_unknown_fields_and_uses_only_data_get()
// ├── invalid_update_preserves_the_last_good_snapshot_as_stale()
// ├── missing_required_fields_and_invalid_ids_are_incompatible()
// ├── polluted_item_ids_are_normalized_to_canonical_identity()
// ├── ambiguous_item_id_suffixes_are_incompatible()
// ├── oversized_content_length_is_rejected_before_body_read()
// ├── sse_supports_comments_crlf_and_complete_update_events()
// ├── response_header_deadline_is_enforced()
// ├── combined_rest_snapshot_has_one_overall_deadline()
// ├── malformed_sse_signal_marks_the_last_snapshot_stale()
// ├── stalled_sse_marks_the_last_snapshot_stale()
// ├── oversized_sse_event_is_rejected()
// ├── cancellation_interrupts_waiting_for_response_headers()
// ├── reconnect_backoff_is_bounded_and_reports_at_the_defined_threshold()
// ├── non_loopback_or_redirectable_base_urls_are_rejected()
// ├── failed_startup_kills_and_reaps_the_owned_child()
// ├── stalled_initial_snapshot_respects_the_startup_deadline()
// ├── startup_window_starts_at_spawn_not_before()
// └── free_dashboard_port()

use std::sync::{Arc, Mutex};
use std::time::Duration;

use rozsa_app::dev_flow::dashboard::start_dashboard_with_delay;
use rozsa_app::dev_flow::{
    DashboardClient, DashboardTiming, DevFlowError, DevFlowIssueStatus, DevFlowTaskStatus,
    ReconnectBackoff, start_dashboard,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

struct ResponsePlan {
    delay: Duration,
    response: String,
    hold_open: Duration,
}

async fn spawn_server(
    plans: Vec<ResponsePlan>,
) -> (reqwest::Url, Arc<Mutex<Vec<String>>>, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&requests);
    let handle = tokio::spawn(async move {
        for plan in plans {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut buffer = [0u8; 1024];
            loop {
                let read = socket.read(&mut buffer).await.unwrap();
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..read]);
                if request.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }
            let first_line = String::from_utf8_lossy(&request)
                .lines()
                .next()
                .unwrap_or_default()
                .to_owned();
            captured.lock().unwrap().push(first_line);
            tokio::time::sleep(plan.delay).await;
            if socket.write_all(plan.response.as_bytes()).await.is_ok() {
                let _ = socket.flush().await;
                tokio::time::sleep(plan.hold_open).await;
            }
        }
    });
    (
        reqwest::Url::parse(&format!("http://{address}/")).unwrap(),
        requests,
        handle,
    )
}

fn json_response(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    )
}

fn rest_plans(task_status: &str) -> Vec<ResponsePlan> {
    let status = serde_json::json!({
        "name": "demo",
        "phase": "DEV",
        "unknownFutureField": true
    });
    let tasks = serde_json::json!({
        "items": [{
            "id": "TASK-T001",
            "title": "Build integration",
            "status": task_status,
            "priority": "P0",
            "documentBody": "must not be retained",
            "files": {"create": [], "modify": ["src/main.rs"], "test": []}
        }],
        "total": 1
    });
    let issues = serde_json::json!({
        "items": [{
            "id": "ISSUE-I001",
            "title": "Connection failed",
            "status": "open",
            "severity": "P1",
            "files": {"create": [], "modify": ["src/main.rs"]}
        }],
        "total": 1
    });
    [status, tasks, issues]
        .into_iter()
        .map(|body| ResponsePlan {
            delay: Duration::ZERO,
            response: json_response(&body.to_string()),
            hold_open: Duration::ZERO,
        })
        .collect()
}

fn fast_timing() -> DashboardTiming {
    DashboardTiming {
        connect_timeout: Duration::from_millis(30),
        request_timeout: Duration::from_millis(100),
        stream_stall_timeout: Duration::from_millis(50),
        startup_timeout: Duration::from_millis(150),
        startup_poll_interval: Duration::from_millis(5),
    }
}

#[tokio::test]
async fn snapshot_adapter_accepts_unknown_fields_and_uses_only_data_get() {
    let (url, requests, server) = spawn_server(rest_plans("pending")).await;
    let client = DashboardClient::with_timing(url, fast_timing()).unwrap();

    let snapshot = client.fetch_snapshot().await.unwrap();

    assert_eq!(snapshot.revision, 1);
    assert_eq!(snapshot.project.name.as_deref(), Some("demo"));
    assert_eq!(snapshot.tasks[0].status, DevFlowTaskStatus::Pending);
    assert_eq!(snapshot.issues[0].status, DevFlowIssueStatus::Open);
    server.await.unwrap();
    assert_eq!(
        requests.lock().unwrap().as_slice(),
        [
            "GET /api/v1/status HTTP/1.1",
            "GET /api/v1/tasks HTTP/1.1",
            "GET /api/v1/issues HTTP/1.1",
        ]
    );
}

#[tokio::test]
async fn invalid_update_preserves_the_last_good_snapshot_as_stale() {
    let mut plans = rest_plans("in_progress");
    let mut invalid = rest_plans("future_status");
    plans.append(&mut invalid);
    let (url, _, server) = spawn_server(plans).await;
    let client = DashboardClient::with_timing(url, fast_timing()).unwrap();

    let first = client.fetch_snapshot().await.unwrap();
    assert_eq!(first.tasks[0].status, DevFlowTaskStatus::InProgress);
    let invalid_result = client.fetch_snapshot().await;
    assert!(
        matches!(invalid_result, Err(DevFlowError::IncompatibleApi(_))),
        "unexpected invalid update result: {invalid_result:?}"
    );
    let retained = client.last_snapshot().await.unwrap();
    assert_eq!(retained.revision, 1);
    assert!(retained.stale);
    server.await.unwrap();
}

#[tokio::test]
async fn missing_required_fields_and_invalid_ids_are_incompatible() {
    let status = serde_json::json!({}).to_string();
    let missing_title = serde_json::json!({
        "items": [{"id": "TASK-T001", "status": "pending"}]
    })
    .to_string();
    let empty_tasks = serde_json::json!({"items": []}).to_string();
    let invalid_id = serde_json::json!({
        "items": [{"id": "I001", "title": "bad", "status": "open", "files": {"create": [], "modify": []}}]
    })
    .to_string();
    let plan = |body: &str| ResponsePlan {
        delay: Duration::ZERO,
        response: json_response(body),
        hold_open: Duration::ZERO,
    };
    let (url, _, server) = spawn_server(vec![
        plan(&status),
        plan(&missing_title),
        plan(&status),
        plan(&empty_tasks),
        plan(&invalid_id),
    ])
    .await;
    let client = DashboardClient::with_timing(url, fast_timing()).unwrap();

    assert!(matches!(
        client.fetch_snapshot().await,
        Err(DevFlowError::IncompatibleApi(_))
    ));
    assert!(matches!(
        client.fetch_snapshot().await,
        Err(DevFlowError::IncompatibleApi(_))
    ));
    server.await.unwrap();
}

#[tokio::test]
async fn polluted_item_ids_are_normalized_to_canonical_identity() {
    // The real dow dashboard can emit an issue id with trailing title text
    // (e.g. `ISSUE-I001：Test TASK-T002 fail`). One polluted entry must not
    // make the whole snapshot incompatible: the canonical id is extracted and
    // the remaining items still decode.
    let status = serde_json::json!({
            "name": "rozsa",
            "phase": "DEV",
            "mode": "quick",
            "version": "1.4.1",
            "goals_minor": "ok",
            "updated": "2026-07-31 03:18"
    });
    let tasks = serde_json::json!({"items": [
            {"id": "TASK-T007", "title": "sidebar task", "status": "pending", "files": {"create": [], "modify": [], "test": []}},
            {"id": "TASK-T008 generated title", "title": "space suffix", "status": "pending", "files": {"create": [], "modify": [], "test": []}},
            {"id": "TASK-T009: generated title", "title": "colon suffix", "status": "pending", "files": {"create": [], "modify": [], "test": []}}
    ]});
    let issues = serde_json::json!({"items": [
            {"id": "ISSUE-I001：Test TASK-T002 fail", "title": "running 12 tests", "status": "closed", "files": {"create": [], "modify": []}},
            {"id": "ISSUE-I002", "title": "clean issue", "status": "open", "files": {"create": [], "modify": []}}
    ]});
    let (url, _, server) = spawn_server(
        [status, tasks, issues]
            .into_iter()
            .map(|body| ResponsePlan {
                delay: Duration::ZERO,
                response: json_response(&body.to_string()),
                hold_open: Duration::ZERO,
            })
            .collect(),
    )
    .await;
    let client = DashboardClient::with_timing(url, fast_timing()).unwrap();

    let snapshot = client.fetch_snapshot().await.expect("snapshot decodes");
    assert_eq!(snapshot.tasks[0].id, "TASK-T007");
    assert_eq!(snapshot.tasks[0].title, "sidebar task");
    assert_eq!(snapshot.tasks[1].id, "TASK-T008");
    assert_eq!(snapshot.tasks[2].id, "TASK-T009");
    assert_eq!(snapshot.issues.len(), 2);
    assert_eq!(snapshot.issues[0].id, "ISSUE-I001");
    assert_eq!(snapshot.issues[0].title, "running 12 tests");
    assert_eq!(snapshot.issues[1].id, "ISSUE-I002");
    server.await.unwrap();
}

#[tokio::test]
async fn ambiguous_item_id_suffixes_are_incompatible() {
    let letter_suffix = serde_json::json!({
        "items": [{"id": "TASK-T001evil", "title": "bad", "status": "pending", "files": {"create": [], "modify": [], "test": []}}]
    })
    .to_string();
    let underscore_suffix = serde_json::json!({
        "items": [{"id": "ISSUE-I001_title", "title": "bad", "status": "open", "files": {"create": [], "modify": []}}]
    })
    .to_string();
    let status = serde_json::json!({}).to_string();
    let empty_tasks = serde_json::json!({"items": []}).to_string();
    let empty_issues = serde_json::json!({"items": []}).to_string();
    let plan = |body: &str| ResponsePlan {
        delay: Duration::ZERO,
        response: json_response(body),
        hold_open: Duration::ZERO,
    };
    let (url, _, server) = spawn_server(vec![
        plan(&status),
        plan(&letter_suffix),
        plan(&empty_issues),
        plan(&status),
        plan(&empty_tasks),
        plan(&underscore_suffix),
    ])
    .await;
    let client = DashboardClient::with_timing(url, fast_timing()).unwrap();

    for _ in 0..2 {
        let result = client.fetch_snapshot().await;
        assert!(
            matches!(result, Err(DevFlowError::IncompatibleApi(_))),
            "unexpected ambiguous id result: {result:?}"
        );
    }
    server.await.unwrap();
}

#[tokio::test]
async fn oversized_content_length_is_rejected_before_body_read() {
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        16 * 1024 * 1024 + 1
    );
    let (url, _, server) = spawn_server(vec![ResponsePlan {
        delay: Duration::ZERO,
        response,
        hold_open: Duration::ZERO,
    }])
    .await;
    let client = DashboardClient::with_timing(url, fast_timing()).unwrap();

    assert!(matches!(
        client.fetch_snapshot().await,
        Err(DevFlowError::ResponseTooLarge)
    ));
    server.await.unwrap();
}

#[tokio::test]
async fn sse_supports_comments_crlf_and_complete_update_events() {
    let sse = ": keep-alive\r\nevent: update\r\ndata: {\"resource\":\r\ndata: \"all\"}\r\n\r\n";
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        sse.len(),
        sse
    );
    let mut plans = vec![ResponsePlan {
        delay: Duration::ZERO,
        response,
        hold_open: Duration::ZERO,
    }];
    plans.extend(rest_plans("done"));
    let (url, requests, server) = spawn_server(plans).await;
    let client = DashboardClient::with_timing(url, fast_timing()).unwrap();
    let mut stream = client.subscribe().await.unwrap();

    let snapshot = stream
        .next_snapshot(&CancellationToken::new())
        .await
        .unwrap()
        .unwrap();

    assert_eq!(snapshot.tasks[0].status, DevFlowTaskStatus::Done);
    server.await.unwrap();
    assert_eq!(
        requests.lock().unwrap().as_slice(),
        [
            "GET /api/v1/events HTTP/1.1",
            "GET /api/v1/status HTTP/1.1",
            "GET /api/v1/tasks HTTP/1.1",
            "GET /api/v1/issues HTTP/1.1",
        ]
    );
}

#[tokio::test]
async fn response_header_deadline_is_enforced() {
    let body = serde_json::json!({"name": "demo"}).to_string();
    let (url, _, server) = spawn_server(vec![ResponsePlan {
        delay: Duration::from_millis(200),
        response: json_response(&body),
        hold_open: Duration::ZERO,
    }])
    .await;
    let client = DashboardClient::with_timing(url, fast_timing()).unwrap();

    assert!(matches!(
        client.fetch_snapshot().await,
        Err(DevFlowError::Timeout(duration)) if duration == Duration::from_millis(100)
    ));
    server.await.unwrap();
}

#[tokio::test]
async fn combined_rest_snapshot_has_one_overall_deadline() {
    let mut plans = rest_plans("pending");
    for plan in &mut plans {
        plan.delay = Duration::from_millis(60);
    }
    plans.truncate(2);
    let (url, _, server) = spawn_server(plans).await;
    let client = DashboardClient::with_timing(url, fast_timing()).unwrap();

    assert!(matches!(
        client.fetch_snapshot().await,
        Err(DevFlowError::Timeout(duration)) if duration == Duration::from_millis(100)
    ));
    server.await.unwrap();
}

#[tokio::test]
async fn malformed_sse_signal_marks_the_last_snapshot_stale() {
    let sse = "event: update\ndata: not-json\n\n";
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        sse.len(),
        sse
    );
    let mut plans = rest_plans("pending");
    plans.push(ResponsePlan {
        delay: Duration::ZERO,
        response,
        hold_open: Duration::ZERO,
    });
    let (url, _, server) = spawn_server(plans).await;
    let client = DashboardClient::with_timing(url, fast_timing()).unwrap();
    client.fetch_snapshot().await.unwrap();
    let mut stream = client.subscribe().await.unwrap();

    assert!(matches!(
        stream.next_snapshot(&CancellationToken::new()).await,
        Err(DevFlowError::IncompatibleApi(_))
    ));
    assert!(client.last_snapshot().await.unwrap().stale);
    server.await.unwrap();
}

#[tokio::test]
async fn stalled_sse_marks_the_last_snapshot_stale() {
    let sse_headers =
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: keep-alive\r\n\r\n";
    let mut plans = rest_plans("pending");
    plans.push(ResponsePlan {
        delay: Duration::ZERO,
        response: sse_headers.to_owned(),
        hold_open: Duration::from_millis(200),
    });
    let (url, _, server) = spawn_server(plans).await;
    let client = DashboardClient::with_timing(url, fast_timing()).unwrap();
    client.fetch_snapshot().await.unwrap();
    let mut stream = client.subscribe().await.unwrap();

    assert!(matches!(
        stream.next_snapshot(&CancellationToken::new()).await,
        Err(DevFlowError::StreamStalled(_))
    ));
    assert!(client.last_snapshot().await.unwrap().stale);
    server.await.unwrap();
}

#[tokio::test]
async fn oversized_sse_event_is_rejected() {
    let data = "x".repeat(16 * 1024 * 1024 + 1);
    let sse = format!("event: update\ndata: {data}\n\n");
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        sse.len(),
        sse
    );
    let (url, _, server) = spawn_server(vec![ResponsePlan {
        delay: Duration::ZERO,
        response,
        hold_open: Duration::ZERO,
    }])
    .await;
    let timing = DashboardTiming {
        stream_stall_timeout: Duration::from_secs(5),
        ..fast_timing()
    };
    let client = DashboardClient::with_timing(url, timing).unwrap();
    let mut stream = client.subscribe().await.unwrap();

    assert!(matches!(
        stream.next_snapshot(&CancellationToken::new()).await,
        Err(DevFlowError::ResponseTooLarge)
    ));
    server.await.unwrap();
}

#[tokio::test]
async fn cancellation_interrupts_waiting_for_response_headers() {
    let body = serde_json::json!({"resource": "all"}).to_string();
    let (url, _, server) = spawn_server(vec![ResponsePlan {
        delay: Duration::from_millis(200),
        response: json_response(&body),
        hold_open: Duration::ZERO,
    }])
    .await;
    let client = DashboardClient::with_timing(url, fast_timing()).unwrap();
    let cancellation = CancellationToken::new();
    let cancel = cancellation.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(10)).await;
        cancel.cancel();
    });

    assert!(matches!(
        client.subscribe_cancellable(&cancellation).await,
        Err(DevFlowError::Cancelled)
    ));
    server.await.unwrap();
}

#[test]
fn reconnect_backoff_is_bounded_and_reports_at_the_defined_threshold() {
    let defaults = DashboardTiming::default();
    assert_eq!(defaults.connect_timeout, Duration::from_secs(1));
    assert_eq!(defaults.request_timeout, Duration::from_secs(5));
    assert_eq!(defaults.stream_stall_timeout, Duration::from_secs(45));
    assert_eq!(defaults.startup_timeout, Duration::from_secs(5));

    let mut backoff = ReconnectBackoff::default();
    let delays = (0..8)
        .map(|_| backoff.next_delay().as_secs())
        .collect::<Vec<_>>();
    assert_eq!(delays, [1, 2, 4, 8, 16, 30, 30, 30]);
    assert!(backoff.should_report_error(Duration::from_secs(1)));
    backoff.reset();
    assert!(!backoff.should_report_error(Duration::from_secs(6)));
    assert!(backoff.should_report_error(Duration::from_secs(7)));
    assert_eq!(backoff.next_delay(), Duration::from_secs(1));
}

#[test]
fn non_loopback_or_redirectable_base_urls_are_rejected() {
    let url = reqwest::Url::parse("https://example.com/").unwrap();
    assert!(matches!(
        DashboardClient::new(url),
        Err(DevFlowError::NonLoopbackUrl(_))
    ));
}

#[cfg(unix)]
#[tokio::test]
async fn failed_startup_kills_and_reaps_the_owned_child() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    let script = temp.path().join("fake-dow");
    std::fs::write(&script, "#!/bin/sh\nsleep 60\n").unwrap();
    let mut permissions = std::fs::metadata(&script).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&script, permissions).unwrap();
    let port = free_dashboard_port().await;
    let process_timing = DashboardTiming {
        startup_timeout: Duration::from_secs(1),
        ..fast_timing()
    };

    let result = start_dashboard(
        &script,
        temp.path(),
        port..=port,
        process_timing,
        &CancellationToken::new(),
    )
    .await;

    let pid = match result {
        Err(DevFlowError::StartupTimeout { pid: Some(pid), .. }) => pid,
        _ => panic!("startup did not return its owned pid"),
    };
    let status = std::process::Command::new("/bin/kill")
        .args(["-0", &pid.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(!status.success(), "owned child was not reaped");
}

#[cfg(unix)]
#[tokio::test]
async fn stalled_initial_snapshot_respects_the_startup_deadline() {
    use std::os::unix::fs::PermissionsExt;
    use std::time::Instant;

    let temp = tempfile::tempdir().unwrap();
    let script = temp.path().join("fake-dow");
    std::fs::write(
        &script,
        "#!/bin/sh\nexec python3 -c 'import socket,sys,time; s=socket.socket(); s.setsockopt(socket.SOL_SOCKET,socket.SO_REUSEADDR,1); s.bind((\"127.0.0.1\",int(sys.argv[1]))); s.listen(1); s.accept(); time.sleep(60)' \"$3\"\n",
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&script).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&script, permissions).unwrap();
    let port = free_dashboard_port().await;
    let timing = DashboardTiming {
        request_timeout: Duration::from_secs(2),
        // Leave enough time for the spawned interpreter to bind and accept the
        // request. A 200 ms window made this a scheduler race rather than a
        // test of a stalled response body.
        startup_timeout: Duration::from_secs(1),
        startup_poll_interval: Duration::from_millis(5),
        ..fast_timing()
    };

    let started = Instant::now();
    let result = start_dashboard(
        &script,
        temp.path(),
        port..=port,
        timing,
        &CancellationToken::new(),
    )
    .await;
    let elapsed = started.elapsed();

    let pid = match result {
        Err(DevFlowError::StartupTimeout { pid: Some(pid), .. }) => pid,
        _ => panic!("stalled startup did not return its owned pid"),
    };
    assert!(
        elapsed < Duration::from_millis(2500),
        "startup request deadline plus mandatory child reaping was exceeded: {elapsed:?}"
    );
    let status = std::process::Command::new("/bin/kill")
        .args(["-0", &pid.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(!status.success(), "owned child was not reaped");
}

#[cfg(unix)]
#[tokio::test]
async fn startup_window_starts_at_spawn_not_before() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    let script = temp.path().join("fake-dow");
    std::fs::write(&script, "#!/bin/sh\nsleep 0.2\nsleep 60\n").unwrap();
    let mut permissions = std::fs::metadata(&script).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&script, permissions).unwrap();
    let port = free_dashboard_port().await;
    let process_timing = DashboardTiming {
        startup_timeout: Duration::from_secs(1),
        ..fast_timing()
    };

    let result = start_dashboard_with_delay(
        &script,
        temp.path(),
        port..=port,
        process_timing,
        &CancellationToken::new(),
        Duration::from_millis(1500),
    )
    .await;

    let pid = match result {
        Err(DevFlowError::StartupTimeout { pid: Some(pid), .. }) => pid,
        _ => panic!("delayed startup did not return its owned pid"),
    };
    let status = std::process::Command::new("/bin/kill")
        .args(["-0", &pid.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(!status.success(), "owned child was not reaped");
}

#[cfg(unix)]
async fn free_dashboard_port() -> u16 {
    for port in 9800..=9900 {
        if let Ok(listener) = TcpListener::bind(("127.0.0.1", port)).await {
            drop(listener);
            return port;
        }
    }
    panic!("no free dashboard test port");
}
