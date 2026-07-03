// File: permissions/audit.rs
//
// Permission audit logger — append-only JSONL with secret redaction.
//
// Internal Framework:
// audit.rs
// ├── PermissionAuditEntry   # serializable log entry
// ├── AuditLogger            # append-only JSONL writer
// └── redact_secrets()       # mask sensitive values
//
// Related Code:
// - [PermissionPolicy](./mod.rs)

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::Serialize;

use super::RiskLevel;

/// A single permission audit log entry.
#[derive(Debug, Clone, Serialize)]
pub struct PermissionAuditEntry {
    pub timestamp: String,
    pub tool_name: String,
    pub args_summary: String,
    pub risk_level: RiskLevel,
    pub decision: String,
    pub decision_source: String,
    pub trust_key: Option<String>,
}

/// Append-only JSONL permission audit logger.
pub struct AuditLogger {
    path: PathBuf,
    file: Mutex<Option<fs::File>>,
}

impl AuditLogger {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            file: Mutex::new(None),
        }
    }

    pub fn log(&self, entry: &PermissionAuditEntry) {
        let mut guard = self.file.lock().unwrap();
        let file = guard.get_or_insert_with(|| {
            if let Some(parent) = self.path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            OpenOptions::new()
                .create(true)
                .truncate(false)
                .append(true)
                .open(&self.path)
                .unwrap_or_else(|_| {
                    OpenOptions::new()
                        .create(true)
                        .truncate(false)
                        .write(true)
                        .open("/dev/null")
                        .unwrap()
                })
        });

        let mut entry = entry.clone();
        entry.args_summary = redact_secrets(&entry.args_summary);

        if let Ok(json) = serde_json::to_string(&entry) {
            let _ = writeln!(file, "{}", json);
        }
    }
}

/// Redact patterns that look like API keys, tokens, or passwords.
fn redact_secrets(s: &str) -> String {
    use regex::Regex;
    static PATTERNS: std::sync::OnceLock<Vec<Regex>> = std::sync::OnceLock::new();
    let patterns = PATTERNS.get_or_init(|| {
        vec![
            // API keys (sk-..., key-..., etc.)
            Regex::new(r"(?i)(sk|key|token|secret|password|api[_-]?key)[=:\s]+\S+").unwrap(),
            // Bearer tokens
            Regex::new(r"(?i)bearer\s+\S+").unwrap(),
            // Long hex/base64 strings (>20 chars, likely secrets)
            Regex::new(r"\b[A-Za-z0-9+/]{32,}={0,2}\b").unwrap(),
        ]
    });

    let mut result = s.to_string();
    for pattern in patterns {
        result = pattern.replace_all(&result, "[REDACTED]").to_string();
    }
    result
}
