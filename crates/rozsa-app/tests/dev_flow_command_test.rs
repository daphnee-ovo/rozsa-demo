// FrameworkTree
// dev_flow_command_test.rs
// ├── success()
// ├── recognizes_direct_create_claim_done_and_close()
// ├── accepts_stdin_redirection_and_arbitrary_producer_pipeline_for_create()
// ├── accepts_quoted_escaped_and_verified_absolute_executables()
// ├── rejects_failed_nonzero_or_truncated_bash_evidence()
// ├── rejects_unsupported_compound_or_ambiguous_shell_syntax()
// ├── create_requires_one_valid_canonical_id_per_nonempty_stdout_line()
// └── action_specific_ids_and_arguments_are_strict()

use std::path::Path;

use rozsa_app::dev_flow::{
    BashExecutionEvidence, DevFlowPresentationAction, DevFlowPresentationItemKind,
    recognize_dow_bash,
};

fn success(stdout: &str) -> BashExecutionEvidence {
    BashExecutionEvidence {
        success: true,
        exit_code: Some(0),
        truncated: false,
        stdout: stdout.to_owned(),
    }
}

#[test]
fn recognizes_direct_create_claim_done_and_close() {
    let created = recognize_dow_bash("dow task create", None, &success("TASK-T001\nTASK-T002\n"))
        .expect("task create");
    assert_eq!(created.action, DevFlowPresentationAction::Created);
    assert_eq!(created.items.len(), 2);
    assert_eq!(created.items[0].id, "TASK-T001");
    assert_eq!(created.items[0].short_id, "T001");
    assert_eq!(created.items[0].kind, DevFlowPresentationItemKind::Task);
    assert!(created.items[0].title.is_none());
    assert!(created.details_unavailable);

    let claimed = recognize_dow_bash(
        "dow claim T1 ISSUE-I002 --timeout 600",
        None,
        &success("claimed"),
    )
    .expect("mixed claim");
    assert_eq!(claimed.action, DevFlowPresentationAction::Claimed);
    assert_eq!(claimed.items[0].id, "TASK-T001");
    assert_eq!(claimed.items[1].id, "ISSUE-I002");

    let done = recognize_dow_bash("dow task done TASK-T001 T2", None, &success("done"))
        .expect("task done");
    assert_eq!(done.action, DevFlowPresentationAction::Completed);
    assert_eq!(done.items[1].id, "TASK-T002");

    let closed =
        recognize_dow_bash("dow issue close I3", None, &success("closed")).expect("issue close");
    assert_eq!(closed.action, DevFlowPresentationAction::Closed);
    assert_eq!(closed.items[0].id, "ISSUE-I003");
}

#[test]
fn accepts_stdin_redirection_and_arbitrary_producer_pipeline_for_create() {
    for command in [
        "dow task create < request.json",
        "< request.json dow task create",
        "cat request.json | dow task create",
        "generate --format json | validate --strict | dow issue create",
    ] {
        let stdout = if command.contains("issue") {
            "ISSUE-I007"
        } else {
            "TASK-T007"
        };
        assert!(
            recognize_dow_bash(command, None, &success(stdout)).is_some(),
            "{command}"
        );
    }
}

#[test]
fn accepts_quoted_escaped_and_verified_absolute_executables() {
    for command in [
        "'dow' task done 'T1'",
        "\"dow\" issue close \"I2\"",
        "d\\ow claim T3",
    ] {
        assert!(
            recognize_dow_bash(command, None, &success("ok")).is_some(),
            "{command}"
        );
    }
    let path = Path::new("/opt/homebrew/bin/dow");
    assert!(
        recognize_dow_bash(
            "/opt/homebrew/bin/dow task done T4",
            Some(path),
            &success("ok")
        )
        .is_some()
    );
    assert!(
        recognize_dow_bash(
            "/usr/local/bin/dow task done T4",
            Some(path),
            &success("ok")
        )
        .is_none()
    );
}

#[test]
fn rejects_failed_nonzero_or_truncated_bash_evidence() {
    for evidence in [
        BashExecutionEvidence {
            success: false,
            ..success("TASK-T001")
        },
        BashExecutionEvidence {
            exit_code: Some(1),
            ..success("TASK-T001")
        },
        BashExecutionEvidence {
            truncated: true,
            ..success("TASK-T001")
        },
        BashExecutionEvidence {
            exit_code: None,
            ..success("TASK-T001")
        },
    ] {
        assert!(recognize_dow_bash("dow task create", None, &evidence).is_none());
    }
}

#[test]
fn rejects_unsupported_compound_or_ambiguous_shell_syntax() {
    for command in [
        "dow task done T1 && echo ok",
        "dow task done T1 || true",
        "dow task done T1; echo ok",
        "dow task done T1\necho ok",
        "dow task done T1 &",
        "for id in T1; do dow task done $id; done",
        "$(which dow) task done T1",
        "`which dow` task done T1",
        "sh script-that-runs-dow.sh",
        "dow task done T1 | cat",
        "cat request.json | dow claim T1",
        "dow task create > created.txt",
        "dow task create <<EOF",
    ] {
        assert!(
            recognize_dow_bash(command, None, &success("TASK-T001")).is_none(),
            "{command}"
        );
    }
    assert!(
        recognize_dow_bash("dow task done 'T1", None, &success("ok")).is_none(),
        "unclosed quote"
    );
    assert!(
        recognize_dow_bash("dow task done T1\\", None, &success("ok")).is_none(),
        "dangling escape"
    );
}

#[test]
fn create_requires_one_valid_canonical_id_per_nonempty_stdout_line() {
    assert!(
        recognize_dow_bash(
            "dow issue create",
            None,
            &success("ISSUE-I001\n\nISSUE-I002\n")
        )
        .is_some()
    );
    for stdout in [
        "",
        "Created TASK-T001",
        "TASK-T001 title",
        "TASK-T001\nnot-an-id",
        "ISSUE-I001",
    ] {
        assert!(
            recognize_dow_bash("dow task create", None, &success(stdout)).is_none(),
            "{stdout:?}"
        );
    }
}

#[test]
fn action_specific_ids_and_arguments_are_strict() {
    for command in [
        "dow task done I1",
        "dow issue close T1",
        "dow claim",
        "dow claim T1 --timeout 0",
        "dow claim T1 --unknown value",
        "dow task done T0",
        "dow task done T1extra",
        "dow task create unexpected",
        "env dow task done T1",
    ] {
        assert!(
            recognize_dow_bash(command, None, &success("TASK-T001")).is_none(),
            "{command}"
        );
    }
}
