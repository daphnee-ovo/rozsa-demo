#[cfg(unix)]
mod unix {
    use std::sync::Arc;
    use std::time::Duration;

    use rozsa_app::tools::bash::BashTool;
    use rozsa_core::tool::Tool;
    use serde_json::json;

    #[tokio::test]
    async fn dropping_a_bash_future_kills_its_descendant_process_group() {
        let workspace = tempfile::tempdir().unwrap();
        let pid_file = workspace.path().join("descendant.pid");
        let command = format!(
            "sleep 30 & child=$!; printf '%s' \"$child\" > '{}'; wait",
            pid_file.display()
        );
        let tool = Arc::new(BashTool::new(
            workspace.path().to_string_lossy().to_string(),
        ));
        let task = tokio::spawn(async move {
            tool.execute("bash-abort", json!({"command": command}), None, None)
                .await
        });

        let pid = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if let Ok(text) = std::fs::read_to_string(&pid_file)
                    && let Ok(pid) = text.parse::<u32>()
                {
                    break pid;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("bash descendant should start");

        task.abort();
        let _ = task.await;

        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let alive = std::process::Command::new("/bin/kill")
                    .args(["-0", &pid.to_string()])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()
                    .is_ok_and(|status| status.success());
                if !alive {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("dropping the Bash future should kill its descendants");
    }
}
