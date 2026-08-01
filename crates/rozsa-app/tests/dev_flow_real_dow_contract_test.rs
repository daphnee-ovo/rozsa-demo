// FrameworkTree
// dev_flow_real_dow_contract_test.rs
// ├── struct TreeEntryFingerprint
// ├── struct CommandRecord
// ├── modified_fingerprint()
// ├── fingerprint_tree()
// ├── visit()
// ├── assert_isolated_cwd()
// ├── find_dow_without_running()
// ├── run_command()
// ├── require_success()
// ├── reserve_dashboard_port()
// ├── run_real_contract()
// └── real_dow_dashboard_contract_is_isolated_and_reaped()

use std::collections::hash_map::DefaultHasher;
use std::fs::Metadata;
use std::hash::{Hash, Hasher};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant, SystemTime};

use rozsa_app::dev_flow::DashboardClient;
use tokio::io::AsyncReadExt;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug, Eq, PartialEq)]
struct TreeEntryFingerprint {
    path: PathBuf,
    kind: &'static str,
    len: u64,
    modified: Option<(u64, u32)>,
    content_hash: Option<u64>,
}

#[derive(Debug)]
struct CommandRecord {
    args: Vec<String>,
    cwd: PathBuf,
    status: Option<i32>,
    stdout: String,
    stderr: String,
}

fn modified_fingerprint(metadata: &Metadata) -> Option<(u64, u32)> {
    metadata
        .modified()
        .ok()?
        .duration_since(SystemTime::UNIX_EPOCH)
        .ok()
        .map(|duration| (duration.as_secs(), duration.subsec_nanos()))
}

fn fingerprint_tree(root: &Path) -> Vec<TreeEntryFingerprint> {
    fn visit(root: &Path, path: &Path, entries: &mut Vec<TreeEntryFingerprint>) {
        let metadata = std::fs::symlink_metadata(path).unwrap();
        let relative = path.strip_prefix(root).unwrap_or(path).to_path_buf();
        let (kind, content_hash) = if metadata.is_dir() {
            ("dir", None)
        } else if metadata.is_file() {
            let mut hasher = DefaultHasher::new();
            std::fs::read(path).unwrap().hash(&mut hasher);
            ("file", Some(hasher.finish()))
        } else if metadata.file_type().is_symlink() {
            let mut hasher = DefaultHasher::new();
            std::fs::read_link(path).unwrap().hash(&mut hasher);
            ("symlink", Some(hasher.finish()))
        } else {
            ("other", None)
        };
        entries.push(TreeEntryFingerprint {
            path: relative,
            kind,
            len: metadata.len(),
            modified: modified_fingerprint(&metadata),
            content_hash,
        });
        if metadata.is_dir() {
            let mut children = std::fs::read_dir(path)
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .collect::<Vec<_>>();
            children.sort();
            for child in children {
                visit(root, &child, entries);
            }
        }
    }

    let mut entries = Vec::new();
    visit(root, root, &mut entries);
    entries
}

fn assert_isolated_cwd(cwd: &Path, sandbox: &Path, development_root: &Path) {
    assert!(
        cwd.starts_with(sandbox),
        "cwd escaped sandbox: {}",
        cwd.display()
    );
    assert!(
        cwd != development_root && !cwd.starts_with(development_root.join(".dev-doc")),
        "cwd points at development project state: {}",
        cwd.display()
    );
}

fn find_dow_without_running() -> Option<PathBuf> {
    let mut candidates = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .map(|directory| directory.join("dow"))
        .collect::<Vec<_>>();
    candidates.extend([
        PathBuf::from("/opt/homebrew/bin/dow"),
        PathBuf::from("/usr/local/bin/dow"),
    ]);
    if let Some(home) = dirs_next::home_dir() {
        candidates.push(home.join(".cargo/bin/dow"));
        candidates.push(home.join(".local/bin/dow"));
    }
    candidates
        .into_iter()
        .find(|candidate| candidate.is_file())
        .and_then(|candidate| candidate.canonicalize().ok())
}

fn run_command(
    executable: &Path,
    args: &[&str],
    cwd: &Path,
    sandbox: &Path,
    development_root: &Path,
    isolated_home: &Path,
) -> CommandRecord {
    assert_isolated_cwd(cwd, sandbox, development_root);
    let output = std::process::Command::new(executable)
        .args(args)
        .current_dir(cwd)
        .env("HOME", isolated_home)
        .output()
        .unwrap_or_else(|error| panic!("failed to run {}: {error}", executable.display()));
    CommandRecord {
        args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        cwd: cwd.to_path_buf(),
        status: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

fn require_success(record: &CommandRecord) {
    assert_eq!(
        record.status,
        Some(0),
        "command failed\nargs={:?}\ncwd={}\nstdout={}\nstderr={}",
        record.args,
        record.cwd.display(),
        record.stdout,
        record.stderr
    );
}

fn reserve_dashboard_port() -> (u16, TcpListener) {
    for port in 9800..=9900 {
        if let Ok(listener) = TcpListener::bind(("127.0.0.1", port)) {
            return (port, listener);
        }
    }
    panic!("no exclusive dashboard port available in 9800-9900");
}

async fn run_real_contract(development_root: &Path, sandbox: &Path) -> Result<(), String> {
    let isolated_home = sandbox.join("home");
    let project = sandbox.join("project");
    std::fs::create_dir_all(&isolated_home).map_err(|error| error.to_string())?;
    std::fs::create_dir_all(&project).map_err(|error| error.to_string())?;
    let project = project.canonicalize().map_err(|error| error.to_string())?;
    assert_isolated_cwd(&project, sandbox, development_root);

    for args in [
        &["init"][..],
        &["config", "user.email", "dev-flow-contract@example.invalid"][..],
        &["config", "user.name", "Dev Flow Contract"][..],
    ] {
        let record = run_command(
            Path::new("git"),
            args,
            &project,
            sandbox,
            development_root,
            &isolated_home,
        );
        require_success(&record);
    }

    let dow = find_dow_without_running()
        .ok_or_else(|| "real dow is required but no supported executable path exists".to_owned())?;
    let version = run_command(
        &dow,
        &["--version"],
        &project,
        sandbox,
        development_root,
        &isolated_home,
    );
    require_success(&version);
    if version.stdout.trim().is_empty() {
        return Err(format!("dow --version produced no output: {version:?}"));
    }

    let init = run_command(
        &dow,
        &["init", "--name", "isolated-contract", "--mode", "quick"],
        &project,
        sandbox,
        development_root,
        &isolated_home,
    );
    require_success(&init);
    let status = run_command(
        &dow,
        &["status"],
        &project,
        sandbox,
        development_root,
        &isolated_home,
    );
    require_success(&status);
    let status_json: serde_json::Value = serde_json::from_str(status.stdout.trim())
        .map_err(|error| format!("dow status was not JSON: {error}; command={status:?}"))?;
    if status_json["name"] != "isolated-contract" {
        return Err(format!("unexpected isolated status: {status_json}"));
    }

    let (port, reservation) = reserve_dashboard_port();
    drop(reservation);
    let mut dashboard = tokio::process::Command::new(&dow);
    let port_text = port.to_string();
    dashboard
        .args(["dashboard", "--port", &port_text, "--no-open"])
        .current_dir(&project)
        .env("HOME", &isolated_home)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let started_at = Instant::now();
    let mut child = dashboard.spawn().map_err(|error| error.to_string())?;
    let pid = child.id();
    let mut child_stdout = child.stdout.take().unwrap();
    let mut child_stderr = child.stderr.take().unwrap();
    let stdout_reader = tokio::spawn(async move {
        let mut bytes = Vec::new();
        let _ = child_stdout.read_to_end(&mut bytes).await;
        String::from_utf8_lossy(&bytes).into_owned()
    });
    let stderr_reader = tokio::spawn(async move {
        let mut bytes = Vec::new();
        let _ = child_stderr.read_to_end(&mut bytes).await;
        String::from_utf8_lossy(&bytes).into_owned()
    });

    let base_url = reqwest::Url::parse(&format!("http://127.0.0.1:{port}/")).unwrap();
    let client = DashboardClient::new(base_url).map_err(|error| error.to_string())?;
    let snapshot = tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            match client.fetch_snapshot().await {
                Ok(snapshot) => break snapshot,
                Err(_) => tokio::time::sleep(Duration::from_millis(50)).await,
            }
        }
    })
    .await
    .map_err(|_| format!("dashboard did not become ready: port={port} pid={pid:?}"))?;
    if snapshot.project.name.as_deref() != Some("isolated-contract") {
        return Err(format!(
            "wrong dashboard project: port={port} pid={pid:?} snapshot={snapshot:?}"
        ));
    }

    let cancellation = CancellationToken::new();
    let mut stream = client
        .subscribe_cancellable(&cancellation)
        .await
        .map_err(|error| error.to_string())?;
    let sse_task = tokio::spawn(async move {
        tokio::time::timeout(Duration::from_secs(10), stream.next_snapshot(&cancellation)).await
    });
    tokio::time::sleep(Duration::from_secs(1)).await;
    let trigger = run_command(
        &dow,
        &["status", "set", "--goals-minor", "contract-sse-update"],
        &project,
        sandbox,
        development_root,
        &isolated_home,
    );
    require_success(&trigger);
    let status_file = std::fs::read_dir(project.join(".dev-doc"))
        .map_err(|error| error.to_string())?
        .filter_map(Result::ok)
        .map(|entry| entry.path().join("STATUS.yaml"))
        .find(|path| path.is_file())
        .ok_or_else(|| "isolated init created no branch STATUS.yaml".to_owned())?;
    std::fs::write(
        status_file
            .parent()
            .unwrap()
            .join("sse-contract-trigger.tmp"),
        "trigger\n",
    )
    .map_err(|error| error.to_string())?;
    tokio::time::sleep(Duration::from_millis(700)).await;
    std::fs::write(
        status_file
            .parent()
            .unwrap()
            .join("sse-contract-trigger-2.tmp"),
        "trigger 2\n",
    )
    .map_err(|error| error.to_string())?;
    let sse_result = sse_task.await;
    let sse_snapshot = match sse_result {
        Ok(Ok(Ok(Some(snapshot)))) => snapshot,
        other => {
            let cleanup = child.kill().await;
            let reaped = child.wait().await;
            let stdout = stdout_reader.await.unwrap_or_default();
            let stderr = stderr_reader.await.unwrap_or_default();
            return Err(format!(
                "SSE watcher contract failed: root={} cwd={} port={port} pid={pid:?} result={other:?} cleanup={cleanup:?} reap={reaped:?} stdout={stdout} stderr={stderr}",
                sandbox.display(),
                project.display(),
            ));
        }
    };
    if sse_snapshot.project.name.as_deref() != Some("isolated-contract") {
        return Err(format!("wrong SSE project: {sse_snapshot:?}"));
    }
    if sse_snapshot.project.goals_minor.as_deref() != Some("contract-sse-update") {
        return Err(format!(
            "SSE did not carry the temporary status update: {sse_snapshot:?}"
        ));
    }
    drop(client);

    let status = match tokio::time::timeout(Duration::from_secs(42), child.wait()).await {
        Ok(result) => result.map_err(|error| error.to_string())?,
        Err(_) => {
            let cleanup = child.kill().await;
            let reaped = child.wait().await;
            let stdout = stdout_reader.await.unwrap_or_default();
            let stderr = stderr_reader.await.unwrap_or_default();
            return Err(format!(
                "dashboard exceeded no-client exit window: root={} cwd={} port={port} pid={pid:?} cleanup={cleanup:?} reap={reaped:?} stdout={stdout} stderr={stderr}",
                sandbox.display(),
                project.display(),
            ));
        }
    };
    let stdout = stdout_reader.await.unwrap_or_default();
    let stderr = stderr_reader.await.unwrap_or_default();
    if !status.success() || started_at.elapsed() < Duration::from_secs(34) {
        return Err(format!(
            "unexpected dashboard exit: root={} cwd={} port={port} pid={pid:?} status={status} elapsed={:?} stdout={stdout} stderr={stderr}",
            sandbox.display(),
            project.display(),
            started_at.elapsed(),
        ));
    }
    if !stderr.contains("No connections, shutting down") {
        return Err(format!(
            "dashboard did not report no-client shutdown: root={} cwd={} port={port} pid={pid:?} stdout={stdout} stderr={stderr}",
            sandbox.display(),
            project.display(),
        ));
    }
    Ok(())
}

#[tokio::test]
async fn real_dow_dashboard_contract_is_isolated_and_reaped() {
    let development_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let development_doc = development_root.join(".dev-doc");
    let before = fingerprint_tree(&development_doc);
    let test_env = development_root.join("tmp/test_env");
    std::fs::create_dir_all(&test_env).unwrap();
    let sandbox = tempfile::Builder::new()
        .prefix("dev-flow-real-dow-contract-")
        .tempdir_in(&test_env)
        .unwrap();
    let sandbox_path = sandbox.path().canonicalize().unwrap();

    let result = run_real_contract(&development_root, &sandbox_path).await;
    let after = fingerprint_tree(&development_doc);
    assert_eq!(
        before,
        after,
        "development .dev-doc content or metadata changed; sandbox={} result={result:?}",
        sandbox_path.display()
    );
    result.unwrap_or_else(|error| panic!("real dow contract failed: {error}"));
}
