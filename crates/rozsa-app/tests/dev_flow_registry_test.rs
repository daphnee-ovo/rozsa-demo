// FrameworkTree
// dev_flow_registry_test.rs
// ├── enum FakeRevision
// ├── struct FakeStatus
// ├── impl FakeStatus
// ├── default()
// ├── struct FakeRunnerState
// ├── struct FakeRunner
// ├── impl FakeRunner
// ├── add_git_project()
// ├── set_revision()
// ├── add_non_git_project()
// ├── set_status()
// ├── deadlines()
// ├── impl FakeRunner
// ├── run()
// ├── success()
// ├── failure()
// ├── struct FakeFactory
// ├── impl FakeFactory
// ├── starts()
// ├── impl FakeFactory
// ├── start()
// ├── temp_project()
// ├── write_status()
// ├── registry()
// ├── struct TimeoutRunner
// ├── impl TimeoutRunner
// ├── run()
// ├── revision_resolution_distinguishes_named_unborn_detached_and_non_git()
// ├── identity_command_failure_is_not_misreported_as_non_git()
// ├── detached_and_ambiguous_non_git_projects_never_start_dashboard()
// ├── readable_marker_and_valid_two_second_status_probe_gate_startup()
// ├── unreadable_status_marker_does_not_start_dashboard()
// ├── selected_probe_detects_dow_init_completed_during_runtime()
// ├── same_project_revision_shares_service_and_snapshot()
// ├── different_projects_do_not_share_services()
// └── successful_bash_reassociates_worktree_sessions_and_keeps_old_service()

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use rozsa_app::dev_flow::{
    CommandExecutionError, CommandOutput, DashboardServiceFactory, DevFlowAvailability,
    DevFlowProjectKey, DevFlowRegistry, DevFlowRevisionKey, DevFlowServiceHandle,
    ProjectCommandRunner, ProjectResolutionError, resolve_project_with,
};
use tempfile::TempDir;
use tokio::sync::RwLock;

#[derive(Clone)]
enum FakeRevision {
    Named { branch: String, oid: String },
    Unborn { branch: String },
    Detached { oid: String },
}

#[derive(Clone)]
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

#[derive(Default)]
struct FakeRunnerState {
    git_projects: HashMap<PathBuf, FakeRevision>,
    statuses: HashMap<PathBuf, FakeStatus>,
    deadlines: Vec<Duration>,
}

#[derive(Clone, Default)]
struct FakeRunner {
    state: Arc<Mutex<FakeRunnerState>>,
}

impl FakeRunner {
    fn add_git_project(&self, root: &Path, revision: FakeRevision) {
        let root = std::fs::canonicalize(root).unwrap();
        let mut state = self.state.lock().unwrap();
        state.git_projects.insert(root.clone(), revision);
        state.statuses.entry(root).or_default();
    }

    fn set_revision(&self, root: &Path, revision: FakeRevision) {
        self.state
            .lock()
            .unwrap()
            .git_projects
            .insert(std::fs::canonicalize(root).unwrap(), revision);
    }

    fn add_non_git_project(&self, root: &Path) {
        self.state
            .lock()
            .unwrap()
            .statuses
            .entry(std::fs::canonicalize(root).unwrap())
            .or_default();
    }

    fn set_status(&self, root: &Path, status: FakeStatus) {
        self.state
            .lock()
            .unwrap()
            .statuses
            .insert(std::fs::canonicalize(root).unwrap(), status);
    }

    fn deadlines(&self) -> Vec<Duration> {
        self.state.lock().unwrap().deadlines.clone()
    }
}

#[async_trait]
impl ProjectCommandRunner for FakeRunner {
    async fn run(
        &self,
        cwd: &Path,
        executable: &Path,
        args: &[&str],
        deadline: Duration,
    ) -> Result<CommandOutput, CommandExecutionError> {
        let cwd = std::fs::canonicalize(cwd)
            .map_err(|error| CommandExecutionError::Launch(error.to_string()))?;
        let mut state = self.state.lock().unwrap();
        state.deadlines.push(deadline);
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
                    FakeRevision::Named { branch, .. } | FakeRevision::Unborn { branch } => {
                        success(&branch)
                    }
                    FakeRevision::Detached { .. } => failure("detached"),
                },
                ["rev-parse", "--verify", "HEAD"] => match revision {
                    FakeRevision::Named { oid, .. } | FakeRevision::Detached { oid } => {
                        success(&oid)
                    }
                    FakeRevision::Unborn { .. } => failure("unborn"),
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
struct FakeFactory {
    next_id: AtomicU64,
    starts: Mutex<Vec<DevFlowProjectKey>>,
}

impl FakeFactory {
    fn starts(&self) -> Vec<DevFlowProjectKey> {
        self.starts.lock().unwrap().clone()
    }
}

#[async_trait]
impl DashboardServiceFactory for FakeFactory {
    async fn start(&self, project: &DevFlowProjectKey) -> Result<DevFlowServiceHandle, String> {
        self.starts.lock().unwrap().push(project.clone());
        let id = self.next_id.fetch_add(1, Ordering::SeqCst) + 1;
        Ok(DevFlowServiceHandle::new(id, Arc::new(RwLock::new(None))))
    }
}

fn temp_project() -> TempDir {
    tempfile::tempdir_in("tmp")
        .or_else(|_| tempfile::tempdir())
        .unwrap()
}

fn write_status(root: &Path, branch: &str) {
    let directory = root.join(".dev-doc").join(branch);
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("STATUS.yaml"), "phase: DEV\n").unwrap();
}

fn registry(runner: Arc<FakeRunner>, factory: Arc<FakeFactory>) -> DevFlowRegistry {
    DevFlowRegistry::new(PathBuf::from("/fake/dow"), runner, factory)
}

struct TimeoutRunner;

#[async_trait]
impl ProjectCommandRunner for TimeoutRunner {
    async fn run(
        &self,
        _cwd: &Path,
        _executable: &Path,
        _args: &[&str],
        deadline: Duration,
    ) -> Result<CommandOutput, CommandExecutionError> {
        Err(CommandExecutionError::Timeout(deadline))
    }
}

#[tokio::test]
async fn revision_resolution_distinguishes_named_unborn_detached_and_non_git() {
    let git_project = temp_project();
    let non_git = temp_project();
    let runner = FakeRunner::default();
    runner.add_git_project(
        git_project.path(),
        FakeRevision::Named {
            branch: "main".to_owned(),
            oid: "a".repeat(40),
        },
    );

    let named = resolve_project_with(git_project.path(), &runner)
        .await
        .unwrap();
    assert_eq!(
        named.revision,
        DevFlowRevisionKey::NamedBranch("main".to_owned())
    );

    runner.set_revision(
        git_project.path(),
        FakeRevision::Unborn {
            branch: "feature/new".to_owned(),
        },
    );
    let unborn = resolve_project_with(git_project.path(), &runner)
        .await
        .unwrap();
    assert_eq!(
        unborn.revision,
        DevFlowRevisionKey::UnbornBranch("feature/new".to_owned())
    );

    runner.set_revision(
        git_project.path(),
        FakeRevision::Detached {
            oid: "b".repeat(40),
        },
    );
    let detached = resolve_project_with(git_project.path(), &runner)
        .await
        .unwrap();
    assert_eq!(
        detached.revision,
        DevFlowRevisionKey::DetachedCommit("b".repeat(40))
    );

    let non_git = resolve_project_with(non_git.path(), &runner).await.unwrap();
    assert_eq!(non_git.revision, DevFlowRevisionKey::NonGit);
}

#[tokio::test]
async fn identity_command_failure_is_not_misreported_as_non_git() {
    let project = temp_project();
    let error = resolve_project_with(project.path(), &TimeoutRunner)
        .await
        .unwrap_err();
    assert!(matches!(error, ProjectResolutionError::Command(_)));
}

#[tokio::test]
async fn detached_and_ambiguous_non_git_projects_never_start_dashboard() {
    let detached = temp_project();
    let non_git = temp_project();
    let runner = Arc::new(FakeRunner::default());
    runner.add_git_project(
        detached.path(),
        FakeRevision::Detached {
            oid: "c".repeat(40),
        },
    );
    runner.add_non_git_project(non_git.path());
    write_status(non_git.path(), "main");
    write_status(non_git.path(), "other");
    let factory = Arc::new(FakeFactory::default());
    let registry = registry(runner, factory.clone());

    let detached_state = registry
        .associate_session("detached", detached.path().to_path_buf())
        .await
        .unwrap();
    assert_eq!(
        detached_state.availability,
        DevFlowAvailability::UnsupportedRevision
    );
    let ambiguous_state = registry
        .associate_session("non-git", non_git.path().to_path_buf())
        .await
        .unwrap();
    assert_eq!(
        ambiguous_state.availability,
        DevFlowAvailability::AmbiguousNonGitProject
    );
    assert!(factory.starts().is_empty());
}

#[tokio::test]
async fn readable_marker_and_valid_two_second_status_probe_gate_startup() {
    let project = temp_project();
    let runner = Arc::new(FakeRunner::default());
    runner.add_git_project(
        project.path(),
        FakeRevision::Named {
            branch: "main".to_owned(),
            oid: "d".repeat(40),
        },
    );
    let factory = Arc::new(FakeFactory::default());
    let registry = registry(runner.clone(), factory.clone());

    let missing = registry
        .associate_session("session", project.path().to_path_buf())
        .await
        .unwrap();
    assert_eq!(
        missing.availability,
        DevFlowAvailability::ProjectNotInitialized
    );

    write_status(project.path(), "main");
    runner.set_status(
        project.path(),
        FakeStatus {
            stdout: "not-json".to_owned(),
            ..FakeStatus::default()
        },
    );
    let invalid = registry.probe_selected("session").await.unwrap().unwrap();
    assert!(matches!(
        invalid.availability,
        DevFlowAvailability::StatusProbeFailed(_)
    ));
    assert!(factory.starts().is_empty());

    runner.set_status(project.path(), FakeStatus::default());
    let ready = registry.probe_selected("session").await.unwrap().unwrap();
    assert_eq!(ready.availability, DevFlowAvailability::Ready);
    assert!(ready.service.is_some());
    assert_eq!(factory.starts().len(), 1);
    assert_eq!(DevFlowRegistry::probe_interval(), Duration::from_secs(2));
    assert!(
        runner
            .deadlines()
            .into_iter()
            .all(|deadline| deadline == Duration::from_secs(2))
    );
}

#[tokio::test]
async fn unreadable_status_marker_does_not_start_dashboard() {
    let project = temp_project();
    let runner = Arc::new(FakeRunner::default());
    runner.add_git_project(
        project.path(),
        FakeRevision::Unborn {
            branch: "new".to_owned(),
        },
    );
    let marker = project.path().join(".dev-doc/new/STATUS.yaml");
    std::fs::create_dir_all(&marker).unwrap();
    let factory = Arc::new(FakeFactory::default());
    let registry = registry(runner, factory.clone());

    let state = registry
        .associate_session("session", project.path().to_path_buf())
        .await
        .unwrap();
    let canonical_marker = std::fs::canonicalize(project.path())
        .unwrap()
        .join(".dev-doc/new/STATUS.yaml");
    assert_eq!(
        state.availability,
        DevFlowAvailability::StatusUnreadable(canonical_marker)
    );
    assert!(factory.starts().is_empty());
}

#[tokio::test]
async fn selected_probe_detects_dow_init_completed_during_runtime() {
    let project = temp_project();
    let runner = Arc::new(FakeRunner::default());
    runner.add_git_project(
        project.path(),
        FakeRevision::Named {
            branch: "main".to_owned(),
            oid: "e".repeat(40),
        },
    );
    let factory = Arc::new(FakeFactory::default());
    let registry = registry(runner, factory.clone());
    let first = registry
        .associate_session("session", project.path().to_path_buf())
        .await
        .unwrap();
    assert_eq!(
        first.availability,
        DevFlowAvailability::ProjectNotInitialized
    );

    write_status(project.path(), "main");
    let rescanned = registry.probe_selected("session").await.unwrap().unwrap();
    assert_eq!(rescanned.availability, DevFlowAvailability::Ready);
    assert_eq!(factory.starts().len(), 1);
}

#[tokio::test]
async fn same_project_revision_shares_service_and_snapshot() {
    let project = temp_project();
    let runner = Arc::new(FakeRunner::default());
    runner.add_git_project(
        project.path(),
        FakeRevision::Named {
            branch: "main".to_owned(),
            oid: "f".repeat(40),
        },
    );
    write_status(project.path(), "main");
    let factory = Arc::new(FakeFactory::default());
    let registry = registry(runner, factory.clone());

    let first = registry
        .associate_session("one", project.path().to_path_buf())
        .await
        .unwrap();
    let second = registry
        .associate_session("two", project.path().to_path_buf())
        .await
        .unwrap();
    let first_service = first.service.unwrap();
    let second_service = second.service.unwrap();
    assert_eq!(first_service.id(), second_service.id());
    assert!(Arc::ptr_eq(
        &first_service.snapshot(),
        &second_service.snapshot()
    ));
    assert_eq!(factory.starts().len(), 1);
}

#[tokio::test]
async fn different_projects_do_not_share_services() {
    let first_project = temp_project();
    let second_project = temp_project();
    let runner = Arc::new(FakeRunner::default());
    for (project, oid) in [
        (&first_project, "1".repeat(40)),
        (&second_project, "2".repeat(40)),
    ] {
        runner.add_git_project(
            project.path(),
            FakeRevision::Named {
                branch: "main".to_owned(),
                oid,
            },
        );
        write_status(project.path(), "main");
    }
    let factory = Arc::new(FakeFactory::default());
    let registry = registry(runner, factory);

    let first = registry
        .associate_session("one", first_project.path().to_path_buf())
        .await
        .unwrap();
    let second = registry
        .associate_session("two", second_project.path().to_path_buf())
        .await
        .unwrap();
    assert_ne!(first.service.unwrap().id(), second.service.unwrap().id());
    assert_eq!(registry.service_count().await, 2);
}

#[tokio::test]
async fn successful_bash_reassociates_worktree_sessions_and_keeps_old_service() {
    let project = temp_project();
    let runner = Arc::new(FakeRunner::default());
    runner.add_git_project(
        project.path(),
        FakeRevision::Named {
            branch: "main".to_owned(),
            oid: "3".repeat(40),
        },
    );
    write_status(project.path(), "main");
    let factory = Arc::new(FakeFactory::default());
    let registry = registry(runner.clone(), factory);
    let one = registry
        .associate_session("one", project.path().to_path_buf())
        .await
        .unwrap();
    registry
        .associate_session("two", project.path().to_path_buf())
        .await
        .unwrap();
    let old_id = one.service.unwrap().id();

    runner.set_revision(
        project.path(),
        FakeRevision::Named {
            branch: "feature".to_owned(),
            oid: "4".repeat(40),
        },
    );
    write_status(project.path(), "feature");
    let states = registry
        .rescan_after_successful_bash(project.path())
        .await
        .unwrap();
    assert_eq!(states.len(), 2);
    let new_ids = states
        .iter()
        .map(|(_, state)| state.service.as_ref().unwrap().id())
        .collect::<Vec<_>>();
    assert!(new_ids.iter().all(|id| *id == new_ids[0]));
    assert_ne!(old_id, new_ids[0]);
    assert_eq!(registry.service_count().await, 2);
    for session in ["one", "two"] {
        assert_eq!(
            registry
                .session_state(session)
                .await
                .unwrap()
                .project
                .revision,
            DevFlowRevisionKey::NamedBranch("feature".to_owned())
        );
    }
}
