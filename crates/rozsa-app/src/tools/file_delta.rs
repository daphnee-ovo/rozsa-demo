//! Bounded text-file deltas emitted by mutating tools.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::process::Command;

use serde::{Deserialize, Serialize};

const MAX_FILES: usize = 2_000;
const MAX_FILE_BYTES: u64 = 512 * 1024;
const MAX_DELTA_BYTES: usize = 256 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDelta {
    pub path: String,
    pub status: FileDeltaStatus,
    pub before: Option<String>,
    pub after: Option<String>,
    pub patch: String,
    pub added: u64,
    pub deleted: u64,
    pub truncated: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileDeltaStatus {
    Added,
    Modified,
    Deleted,
}

#[derive(Default)]
pub struct WorkspaceSnapshot {
    pub files: BTreeMap<String, String>,
    pub complete: bool,
    pub limitation: Option<String>,
}

pub fn read_text_if_present(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok()
}

pub fn build_file_delta(
    path: impl Into<String>,
    before: Option<String>,
    after: Option<String>,
) -> Option<FileDelta> {
    if before == after {
        return None;
    }
    let path = path.into();
    let status = match (&before, &after) {
        (None, Some(_)) => FileDeltaStatus::Added,
        (Some(_), None) => FileDeltaStatus::Deleted,
        _ => FileDeltaStatus::Modified,
    };
    let (mut patch, added, deleted) = render_patch(&path, before.as_deref(), after.as_deref());
    let truncated = patch.len() > MAX_DELTA_BYTES;
    if truncated {
        patch.truncate(floor_char_boundary(&patch, MAX_DELTA_BYTES));
    }
    Some(FileDelta {
        path,
        status,
        before: before.map(|value| truncate_text(value, MAX_DELTA_BYTES)),
        after: after.map(|value| truncate_text(value, MAX_DELTA_BYTES)),
        patch,
        added,
        deleted,
        truncated,
    })
}

pub fn snapshot_workspace(root: &Path) -> WorkspaceSnapshot {
    let mut snapshot = WorkspaceSnapshot {
        complete: true,
        ..Default::default()
    };
    let paths = git_visible_paths(root).unwrap_or_else(|| collect_paths(root));
    for path in paths.into_iter().take(MAX_FILES + 1) {
        if snapshot.files.len() >= MAX_FILES {
            snapshot.complete = false;
            snapshot.limitation = Some(format!("workspace snapshot exceeded {MAX_FILES} files"));
            break;
        }
        if secret_like(&path) {
            continue;
        }
        let absolute = root.join(&path);
        let Ok(metadata) = fs::metadata(&absolute) else {
            continue;
        };
        if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES {
            continue;
        }
        if let Ok(content) = fs::read_to_string(&absolute) {
            snapshot.files.insert(path, content);
        }
    }
    snapshot
}

pub fn diff_snapshots(before: WorkspaceSnapshot, after: WorkspaceSnapshot) -> (Vec<FileDelta>, bool, Option<String>) {
    let paths = before
        .files
        .keys()
        .chain(after.files.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    let deltas = paths
        .into_iter()
        .filter_map(|path| {
            build_file_delta(
                path.clone(),
                before.files.get(&path).cloned(),
                after.files.get(&path).cloned(),
            )
        })
        .collect();
    let complete = before.complete && after.complete;
    let limitation = before.limitation.or(after.limitation);
    (deltas, complete, limitation)
}

fn git_visible_paths(root: &Path) -> Option<Vec<String>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["ls-files", "--cached", "--others", "--exclude-standard"])
        .output()
        .ok()?;
    output.status.success().then(|| {
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect()
    })
}

fn collect_paths(root: &Path) -> Vec<String> {
    fn visit(root: &Path, current: &Path, paths: &mut Vec<String>) {
        let Ok(entries) = fs::read_dir(current) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.file_name().is_some_and(|name| name == ".git" || name == "target") {
                continue;
            }
            if path.is_dir() {
                visit(root, &path, paths);
            } else if let Ok(relative) = path.strip_prefix(root) {
                paths.push(relative.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    let mut paths = Vec::new();
    visit(root, root, &mut paths);
    paths
}

fn secret_like(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.split('/').any(|part| {
        matches!(part, ".env" | "id_rsa" | "auth.json" | "credentials" | "credentials.json")
            || part.contains("secret")
            || part.contains("token")
    })
}

fn render_patch(path: &str, before: Option<&str>, after: Option<&str>) -> (String, u64, u64) {
    let left = if before.is_some() { format!("a/{path}") } else { "/dev/null".to_string() };
    let right = if after.is_some() { format!("b/{path}") } else { "/dev/null".to_string() };
    let mut patch = format!("--- {left}\n+++ {right}\n");
    let before_lines = before.unwrap_or("").lines().collect::<Vec<_>>();
    let after_lines = after.unwrap_or("").lines().collect::<Vec<_>>();
    let mut prefix = 0;
    while prefix < before_lines.len()
        && prefix < after_lines.len()
        && before_lines[prefix] == after_lines[prefix]
    {
        prefix += 1;
    }
    let mut suffix = 0;
    while suffix < before_lines.len().saturating_sub(prefix)
        && suffix < after_lines.len().saturating_sub(prefix)
        && before_lines[before_lines.len() - 1 - suffix]
            == after_lines[after_lines.len() - 1 - suffix]
    {
        suffix += 1;
    }
    let deleted_lines = &before_lines[prefix..before_lines.len() - suffix];
    let added_lines = &after_lines[prefix..after_lines.len() - suffix];
    patch.push_str(&format!(
        "@@ -{},{} +{},{} @@\n",
        prefix + 1,
        deleted_lines.len(),
        prefix + 1,
        added_lines.len()
    ));
    for line in deleted_lines {
        patch.push('-');
        patch.push_str(line);
        patch.push('\n');
    }
    for line in added_lines {
        patch.push('+');
        patch.push_str(line);
        patch.push('\n');
    }
    (patch, added_lines.len() as u64, deleted_lines.len() as u64)
}

fn truncate_text(mut value: String, max: usize) -> String {
    if value.len() > max {
        value.truncate(floor_char_boundary(&value, max));
    }
    value
}

fn floor_char_boundary(value: &str, max: usize) -> usize {
    let mut end = value.len().min(max);
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    end
}
