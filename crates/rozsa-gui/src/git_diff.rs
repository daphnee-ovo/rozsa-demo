//! Safe workspace diff helpers. Tracked changes use `git diff`; untracked
//! files are rendered against the null device, matching Codex `/diff`.

use std::path::Path;
use std::process::{Command, Output};

use serde::Serialize;

const MAX_PATCH_BYTES: usize = 120_000;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDiff {
    pub path: String,
    pub patch: String,
    pub truncated: bool,
}

pub fn read_workspace_diff(cwd: &Path, path: &str) -> Result<FileDiff, String> {
    validate_relative_path(path)?;
    if !inside_git_repo(cwd)? {
        return Err("Workspace is not a Git repository".to_string());
    }

    let tracked = run_git(cwd, &["ls-files", "--error-unmatch", "--", path])?
        .status
        .success();
    let bytes = if tracked {
        checked_diff(cwd, &["diff", "--no-textconv", "--no-ext-diff", "--", path])?
            .stdout
    } else {
        let untracked = run_git(cwd, &["ls-files", "--others", "--exclude-standard", "--", path])?;
        if !untracked.status.success() || untracked.stdout.is_empty() {
            Vec::new()
        } else {
            checked_diff(
                cwd,
                &[
                    "diff",
                    "--no-textconv",
                    "--no-ext-diff",
                    "--no-index",
                    "--",
                    null_device(),
                    path,
                ],
            )?
            .stdout
        }
    };

    let truncated = bytes.len() > MAX_PATCH_BYTES;
    let patch = String::from_utf8_lossy(&bytes[..bytes.len().min(MAX_PATCH_BYTES)]).to_string();
    Ok(FileDiff {
        path: path.to_string(),
        patch,
        truncated,
    })
}

pub fn workspace_diff_stat(cwd: &Path) -> (u64, u64) {
    if !inside_git_repo(cwd).unwrap_or(false) {
        return (0, 0);
    }
    let mut totals = (0, 0);
    if let Ok(output) = checked_diff(cwd, &["diff", "--numstat", "--no-ext-diff", "--"]) {
        add_numstat(&mut totals, &output.stdout);
    }
    let Ok(untracked) = run_git(cwd, &["ls-files", "--others", "--exclude-standard"]) else {
        return totals;
    };
    for path in String::from_utf8_lossy(&untracked.stdout)
        .lines()
        .map(str::trim)
        .filter(|path| !path.is_empty())
    {
        if let Ok(output) = checked_diff(
            cwd,
            &[
                "diff",
                "--no-textconv",
                "--no-ext-diff",
                "--no-index",
                "--numstat",
                "--",
                null_device(),
                path,
            ],
        ) {
            add_numstat(&mut totals, &output.stdout);
        }
    }
    totals
}

fn validate_relative_path(path: &str) -> Result<(), String> {
    let path = Path::new(path);
    if path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err("Diff path must stay within the workspace".to_string());
    }
    Ok(())
}

fn inside_git_repo(cwd: &Path) -> Result<bool, String> {
    Ok(run_git(cwd, &["rev-parse", "--is-inside-work-tree"])?
        .status
        .success())
}

fn checked_diff(cwd: &Path, args: &[&str]) -> Result<Output, String> {
    let output = run_git(cwd, args)?;
    if output.status.success() || output.status.code() == Some(1) {
        return Ok(output);
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(if stderr.is_empty() {
        format!("git {args:?} failed")
    } else {
        format!("git {args:?} failed: {stderr}")
    })
}

fn run_git(cwd: &Path, args: &[&str]) -> Result<Output, String> {
    Command::new("git")
        .arg("-c")
        .arg(if cfg!(windows) {
            "core.hooksPath=NUL"
        } else {
            "core.hooksPath=/dev/null"
        })
        .arg("-C")
        .arg(cwd)
        .args(args)
        .output()
        .map_err(|error| format!("Failed to run git: {error}"))
}

fn null_device() -> &'static str {
    if cfg!(windows) { "NUL" } else { "/dev/null" }
}

fn add_numstat(totals: &mut (u64, u64), bytes: &[u8]) {
    for line in String::from_utf8_lossy(bytes).lines() {
        let mut parts = line.split_whitespace();
        totals.0 += parts
            .next()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(0);
        totals.1 += parts
            .next()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(0);
    }
}
