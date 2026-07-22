use std::path::Path;
use std::process::Command;

fn run(command: &mut Command) {
    let output = command.output().expect("command should start");
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn models_request_sends_codex_compatible_client_version() {
    let source = include_str!("../src/models_endpoint.rs");

    assert!(source.contains("const CODEX_MODELS_CLIENT_VERSION: &str = \"0.146.0\";"));
    assert!(source.contains(".query(&[(\"client_version\", CODEX_MODELS_CLIENT_VERSION)])"));
}

#[test]
fn sync_tool_uses_latest_valid_codex_release_tag() {
    let temp = tempfile::tempdir().expect("temp directory should be created");
    let codex_repo = temp.path().join("codex");
    std::fs::create_dir(&codex_repo).expect("fake Codex repository should be created");
    run(Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(&codex_repo));
    run(Command::new("git")
        .args([
            "-c",
            "user.name=Rózsa Test",
            "-c",
            "user.email=rozsa-test@example.invalid",
            "commit",
            "--quiet",
            "--allow-empty",
            "-m",
            "fixture",
        ])
        .current_dir(&codex_repo));
    for tag in [
        "rust-v0.144.0",
        "rust-v0.145.0-alpha.30",
        "rust-vrust-v9.0.0",
    ] {
        run(Command::new("git")
            .args(["tag", tag])
            .current_dir(&codex_repo));
    }

    let target = temp.path().join("models_endpoint.rs");
    std::fs::write(
        &target,
        "const CODEX_MODELS_CLIENT_VERSION: &str = \"0.144.0\";\n",
    )
    .expect("fixture target should be written");
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let script = workspace.join("devtools/sync-codex-model-client-version.sh");

    let stale_check = Command::new("bash")
        .arg(&script)
        .args([
            "--repo-url",
            codex_repo.to_str().unwrap(),
            "--target",
            target.to_str().unwrap(),
            "--check",
        ])
        .output()
        .expect("stale check should start");
    assert!(!stale_check.status.success());
    assert!(String::from_utf8_lossy(&stale_check.stderr).contains("is stale"));

    run(Command::new("bash").arg(&script).args([
        "--repo-url",
        codex_repo.to_str().unwrap(),
        "--target",
        target.to_str().unwrap(),
    ]));
    let updated = std::fs::read_to_string(&target).expect("updated target should be readable");
    assert!(updated.contains("CODEX_MODELS_CLIENT_VERSION: &str = \"0.145.0\""));

    run(Command::new("bash").arg(&script).args([
        "--repo-url",
        codex_repo.to_str().unwrap(),
        "--target",
        target.to_str().unwrap(),
        "--check",
    ]));
}

#[test]
fn sync_tool_defaults_to_openai_codex_github() {
    let script = include_str!("../../../devtools/sync-codex-model-client-version.sh");

    assert!(script.contains("https://github.com/openai/codex.git"));
    assert!(script.contains("git ls-remote --tags --refs"));
    assert!(!script.contains("../codex"));
}
