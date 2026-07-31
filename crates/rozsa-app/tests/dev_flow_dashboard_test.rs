// FrameworkTree
// dev_flow_dashboard_test.rs
// ├── struct ResponsePlan
// ├── spawn_server()
// ├── json_response()
// ├── snapshot_json()
// ├── fast_timing()
// ├── snapshot_adapter_accepts_unknown_fields_and_uses_only_data_get()
// ├── invalid_update_preserves_the_last_good_snapshot_as_stale()
// ├── missing_required_fields_and_invalid_ids_are_incompatible()
// ├── oversized_content_length_is_rejected_before_body_read()
// ├── sse_supports_comments_crlf_and_complete_update_events()
// ├── response_header_deadline_is_enforced()
// ├── stalled_sse_marks_the_last_snapshot_stale()
// ├── oversized_sse_event_is_rejected()
// ├── cancellation_interrupts_waiting_for_response_headers()
// ├── reconnect_backoff_is_bounded_and_reports_at_the_defined_threshold()
// ├── non_loopback_or_redirectable_base_urls_are_rejected()
// ├── failed_startup_kills_and_reaps_the_owned_child()
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

fn snapshot_json(task_status: &str) -> String {
    serde_json::json!({
        "status": {
            "name": "demo",
            "phase": "DEV",
            "unknownFutureField": true
        },
        "tasks": [{
            "id": "TASK-T001",
            "title": "Build integration",
            "status": task_status,
            "priority": "P0",
            "documentBody": "must not be retained"
        }],
        "issues": [{
            "id": "ISSUE-I001",
            "title": "Connection failed",
            "status": "open",
            "severity": "P1"
        }],
        "docs": {"spec": {"content": "ignored"}}
    })
    .to_string()
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
    let body = snapshot_json("pending");
    let (url, requests, server) = spawn_server(vec![ResponsePlan {
        delay: Duration::ZERO,
        response: json_response(&body),
        hold_open: Duration::ZERO,
    }])
    .await;
    let client = DashboardClient::with_timing(url, fast_timing()).unwrap();

    let snapshot = client.fetch_snapshot().await.unwrap();

    assert_eq!(snapshot.revision, 1);
    assert_eq!(snapshot.project.name.as_deref(), Some("demo"));
    assert_eq!(snapshot.tasks[0].status, DevFlowTaskStatus::Pending);
    assert_eq!(snapshot.issues[0].status, DevFlowIssueStatus::Open);
    server.await.unwrap();
    assert_eq!(
        requests.lock().unwrap().as_slice(),
        ["GET /api/data HTTP/1.1"]
    );
}

#[tokio::test]
async fn invalid_update_preserves_the_last_good_snapshot_as_stale() {
    let valid = snapshot_json("in_progress");
    let invalid = snapshot_json("future_status");
    let (url, _, server) = spawn_server(vec![
        ResponsePlan {
            delay: Duration::ZERO,
            response: json_response(&valid),
            hold_open: Duration::ZERO,
        },
        ResponsePlan {
            delay: Duration::ZERO,
            response: json_response(&invalid),
            hold_open: Duration::ZERO,
        },
    ])
    .await;
    let client = DashboardClient::with_timing(url, fast_timing()).unwrap();

    let first = client.fetch_snapshot().await.unwrap();
    assert_eq!(first.tasks[0].status, DevFlowTaskStatus::InProgress);
    assert!(matches!(
        client.fetch_snapshot().await,
        Err(DevFlowError::IncompatibleApi(_))
    ));
    let retained = client.last_snapshot().await.unwrap();
    assert_eq!(retained.revision, 1);
    assert!(retained.stale);
    server.await.unwrap();
}

#[tokio::test]
async fn missing_required_fields_and_invalid_ids_are_incompatible() {
    let missing_title = serde_json::json!({
        "status": {},
        "tasks": [{"id": "TASK-T001", "status": "pending"}],
        "issues": []
    })
    .to_string();
    let invalid_id = serde_json::json!({
        "status": {},
        "tasks": [],
        "issues": [{"id": "I001", "title": "bad", "status": "open"}]
    })
    .to_string();
    let (url, _, server) = spawn_server(vec![
        ResponsePlan {
            delay: Duration::ZERO,
            response: json_response(&missing_title),
            hold_open: Duration::ZERO,
        },
        ResponsePlan {
            delay: Duration::ZERO,
            response: json_response(&invalid_id),
            hold_open: Duration::ZERO,
        },
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
    let body = snapshot_json("done");
    let split_at = body.find(",\"tasks\"").unwrap() + 1;
    let (first, second) = body.split_at(split_at);
    let sse = format!(": keep-alive\r\nevent: update\r\ndata: {first}\r\ndata: {second}\r\n\r\n");
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        sse.len(),
        sse
    );
    let (url, requests, server) = spawn_server(vec![ResponsePlan {
        delay: Duration::ZERO,
        response,
        hold_open: Duration::ZERO,
    }])
    .await;
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
        ["GET /api/events HTTP/1.1"]
    );
}

#[tokio::test]
async fn response_header_deadline_is_enforced() {
    let body = snapshot_json("pending");
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
async fn stalled_sse_marks_the_last_snapshot_stale() {
    let body = snapshot_json("pending");
    let sse_headers =
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: keep-alive\r\n\r\n";
    let (url, _, server) = spawn_server(vec![
        ResponsePlan {
            delay: Duration::ZERO,
            response: json_response(&body),
            hold_open: Duration::ZERO,
        },
        ResponsePlan {
            delay: Duration::ZERO,
            response: sse_headers.to_owned(),
            hold_open: Duration::from_millis(200),
        },
    ])
    .await;
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
    let body = snapshot_json("pending");
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
    let pid_file = temp.path().join("pid");
    std::fs::write(
        &script,
        format!("#!/bin/sh\necho $$ > '{}'\nsleep 60\n", pid_file.display()),
    )
    .unwrap();
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

    assert!(matches!(result, Err(DevFlowError::StartupTimeout { .. })));
    let pid = std::fs::read_to_string(pid_file).unwrap();
    let status = std::process::Command::new("/bin/kill")
        .args(["-0", pid.trim()])
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
    let pid_file = temp.path().join("pid");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\nsleep 0.2\necho $$ > '{}'\nsleep 60\n",
            pid_file.display()
        ),
    )
    .unwrap();
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

    assert!(matches!(result, Err(DevFlowError::StartupTimeout { .. })));
    let pid = std::fs::read_to_string(pid_file).unwrap();
    let status = std::process::Command::new("/bin/kill")
        .args(["-0", pid.trim()])
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
