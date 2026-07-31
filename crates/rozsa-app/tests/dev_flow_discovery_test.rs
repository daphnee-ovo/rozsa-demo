// FrameworkTree
// dev_flow_discovery_test.rs
// ├── struct FakeRunner
// ├── impl FakeRunner
// ├── output()
// ├── failure()
// ├── impl FakeRunner
// ├── run()
// ├── touch()
// ├── environment()
// ├── path_candidate_wins_before_other_install_sources()
// ├── homebrew_prefix_is_checked_before_cargo()
// ├── cargo_is_checked_before_npm()
// ├── npm_prefix_and_standard_prefix_locations_are_supported()
// ├── canonical_candidates_are_deduplicated()
// ├── invalid_custom_path_never_falls_back_to_automatic_discovery()
// └── relative_custom_path_fails_before_execution()

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;
use rozsa_app::dev_flow::{
    CommandExecutionError, CommandOutput, DiscoveryCommandRunner, DiscoveryEnvironment,
    DowDiscoveryError, DowInstallSource, discover_dow_with,
};
use rozsa_app::settings::DevFlowSettings;

#[derive(Default)]
struct FakeRunner {
    outputs: HashMap<(PathBuf, Vec<String>), Result<CommandOutput, CommandExecutionError>>,
    calls: Mutex<Vec<(PathBuf, Vec<String>, Duration)>>,
}

impl FakeRunner {
    fn output(mut self, executable: &Path, args: &[&str], stdout: &str) -> Self {
        self.outputs.insert(
            (
                executable.to_path_buf(),
                args.iter().map(|arg| (*arg).to_owned()).collect(),
            ),
            Ok(CommandOutput {
                success: true,
                code: Some(0),
                stdout: stdout.to_owned(),
                stderr: String::new(),
            }),
        );
        self
    }

    fn failure(mut self, executable: &Path, args: &[&str], stderr: &str) -> Self {
        self.outputs.insert(
            (
                executable.to_path_buf(),
                args.iter().map(|arg| (*arg).to_owned()).collect(),
            ),
            Ok(CommandOutput {
                success: false,
                code: Some(1),
                stdout: String::new(),
                stderr: stderr.to_owned(),
            }),
        );
        self
    }
}

#[async_trait]
impl DiscoveryCommandRunner for FakeRunner {
    async fn run(
        &self,
        executable: &Path,
        args: &[&str],
        deadline: Duration,
    ) -> Result<CommandOutput, CommandExecutionError> {
        let args = args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>();
        self.calls
            .lock()
            .unwrap()
            .push((executable.to_path_buf(), args.clone(), deadline));
        self.outputs
            .get(&(executable.to_path_buf(), args))
            .cloned()
            .unwrap_or_else(|| {
                Err(CommandExecutionError::Launch(
                    "unexpected command".to_owned(),
                ))
            })
    }
}

fn touch(path: &Path) -> PathBuf {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, b"fake executable").unwrap();
    std::fs::canonicalize(path).unwrap()
}

fn environment(path: OsString) -> DiscoveryEnvironment {
    DiscoveryEnvironment {
        path,
        home_dir: None,
        cargo_home: None,
        npm_config_prefix: None,
        app_data: None,
        homebrew_bin_dirs: Vec::new(),
    }
}

#[tokio::test]
async fn path_candidate_wins_before_other_install_sources() {
    let temp = tempfile::tempdir().unwrap();
    let path_dow = touch(&temp.path().join("path/bin/dow"));
    let cargo_dow = touch(&temp.path().join("cargo/bin/dow"));
    let mut environment = environment(std::env::join_paths([path_dow.parent().unwrap()]).unwrap());
    environment.cargo_home = Some(temp.path().join("cargo"));
    let runner = FakeRunner::default()
        .output(&path_dow, &["--version"], "dow 0.3.9\n")
        .output(&cargo_dow, &["--version"], "dow 9.9.9\n");

    let found = discover_dow_with(&DevFlowSettings::default(), &environment, &runner)
        .await
        .unwrap();

    assert_eq!(found.executable, path_dow);
    assert_eq!(found.source, DowInstallSource::Path);
    assert_eq!(found.version.to_string(), "0.3.9");
    let calls = runner.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].2, Duration::from_secs(2));
}

#[tokio::test]
async fn homebrew_prefix_is_checked_before_cargo() {
    let temp = tempfile::tempdir().unwrap();
    let brew = touch(&temp.path().join("brew-bin/brew"));
    let brew_dow = touch(&temp.path().join("brew-prefix/bin/dow"));
    let cargo_dow = touch(&temp.path().join("cargo/bin/dow"));
    let mut environment = environment(OsString::new());
    environment.homebrew_bin_dirs = vec![temp.path().join("brew-bin")];
    environment.cargo_home = Some(temp.path().join("cargo"));
    let runner = FakeRunner::default()
        .output(
            &brew,
            &["--prefix"],
            &format!("{}\n", temp.path().join("brew-prefix").display()),
        )
        .output(&brew_dow, &["--version"], "dow 1.2.3\n")
        .output(&cargo_dow, &["--version"], "dow 9.9.9\n");

    let found = discover_dow_with(&DevFlowSettings::default(), &environment, &runner)
        .await
        .unwrap();

    assert_eq!(found.executable, brew_dow);
    assert_eq!(found.source, DowInstallSource::Homebrew);
    assert_eq!(runner.calls.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn cargo_is_checked_before_npm() {
    let temp = tempfile::tempdir().unwrap();
    let npm = touch(&temp.path().join("path/npm"));
    let cargo_dow = touch(&temp.path().join("cargo/bin/dow"));
    let npm_dow = touch(&temp.path().join("npm-prefix/bin/dow"));
    let mut environment = environment(std::env::join_paths([npm.parent().unwrap()]).unwrap());
    environment.cargo_home = Some(temp.path().join("cargo"));
    let runner = FakeRunner::default()
        .output(&cargo_dow, &["--version"], "dow 2.0.0\n")
        .output(
            &npm,
            &["prefix", "-g"],
            &format!("{}\n", temp.path().join("npm-prefix").display()),
        )
        .output(&npm_dow, &["--version"], "dow 9.9.9\n");

    let found = discover_dow_with(&DevFlowSettings::default(), &environment, &runner)
        .await
        .unwrap();

    assert_eq!(found.executable, cargo_dow);
    assert_eq!(found.source, DowInstallSource::Cargo);
    assert_eq!(runner.calls.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn npm_prefix_and_standard_prefix_locations_are_supported() {
    let temp = tempfile::tempdir().unwrap();
    let npm = touch(&temp.path().join("path/npm"));
    let npm_dow = touch(&temp.path().join("npm-prefix/bin/dow"));
    let npm_environment = environment(std::env::join_paths([npm.parent().unwrap()]).unwrap());
    let runner = FakeRunner::default()
        .output(
            &npm,
            &["prefix", "-g"],
            &format!("{}\n", temp.path().join("npm-prefix").display()),
        )
        .output(&npm_dow, &["--version"], "dow 3.4.5-beta.1\n");

    let found = discover_dow_with(&DevFlowSettings::default(), &npm_environment, &runner)
        .await
        .unwrap();

    assert_eq!(found.executable, npm_dow);
    assert_eq!(found.source, DowInstallSource::Npm);

    let standard_dow = touch(&temp.path().join("configured-prefix/bin/dow"));
    let mut standard_environment = environment(OsString::new());
    standard_environment.npm_config_prefix = Some(temp.path().join("configured-prefix"));
    let standard_runner =
        FakeRunner::default().output(&standard_dow, &["--version"], "dow 4.5.6\n");
    let standard_found = discover_dow_with(
        &DevFlowSettings::default(),
        &standard_environment,
        &standard_runner,
    )
    .await
    .unwrap();
    assert_eq!(standard_found.executable, standard_dow);
    assert_eq!(standard_found.source, DowInstallSource::Npm);
}

#[tokio::test]
async fn canonical_candidates_are_deduplicated() {
    let temp = tempfile::tempdir().unwrap();
    let dow = touch(&temp.path().join("path/dow"));
    let repeated_path =
        std::env::join_paths([dow.parent().unwrap(), dow.parent().unwrap()]).unwrap();
    let environment = environment(repeated_path);
    let runner = FakeRunner::default().failure(&dow, &["--version"], "broken");

    assert!(matches!(
        discover_dow_with(&DevFlowSettings::default(), &environment, &runner).await,
        Err(DowDiscoveryError::NotFound)
    ));
    assert_eq!(runner.calls.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn invalid_custom_path_never_falls_back_to_automatic_discovery() {
    let temp = tempfile::tempdir().unwrap();
    let automatic = touch(&temp.path().join("path/dow"));
    let custom = touch(&temp.path().join("custom/dow"));
    let environment = environment(std::env::join_paths([automatic.parent().unwrap()]).unwrap());
    let runner = FakeRunner::default()
        .failure(&custom, &["--version"], "not dow")
        .output(&automatic, &["--version"], "dow 0.3.9\n");
    let settings = DevFlowSettings {
        executable_path: Some(custom.clone()),
        ..Default::default()
    };

    let error = discover_dow_with(&settings, &environment, &runner)
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        DowDiscoveryError::InvalidCustomExecutable { .. }
    ));
    let calls = runner.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, custom);
}

#[tokio::test]
async fn relative_custom_path_fails_before_execution() {
    let runner = FakeRunner::default();
    let settings = DevFlowSettings {
        executable_path: Some(PathBuf::from("dow")),
        ..Default::default()
    };

    assert!(matches!(
        discover_dow_with(&settings, &environment(OsString::new()), &runner).await,
        Err(DowDiscoveryError::CustomPathNotAbsolute(_))
    ));
    assert!(runner.calls.lock().unwrap().is_empty());
}
