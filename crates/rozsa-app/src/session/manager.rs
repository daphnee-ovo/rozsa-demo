// FrameworkTree
// manager.rs
// ├── struct SessionHeader
// ├── struct SessionEntryBase
// ├── struct SessionMessageEntry
// ├── struct ThinkingLevelChangeEntry
// ├── struct ModelChangeEntry
// ├── struct CompactionEntry
// ├── struct CustomEntry
// ├── struct LabelEntry
// ├── struct SessionInfoEntry
// ├── enum SessionEntry
// ├── impl SessionEntry
// ├── id()
// ├── parent_id()
// ├── struct SessionMeta
// ├── struct SessionManager
// ├── impl SessionManager
// ├── cwd()
// ├── set_cwd()
// ├── create()
// ├── create_lazy()
// ├── generate_id()
// ├── ensure_materialized()
// ├── append_entry()
// ├── append_message()
// ├── append_compaction()
// ├── append_model_change()
// ├── append_thinking_level_change()
// ├── append_custom()
// ├── append_label()
// ├── leaf_id()
// ├── session_id()
// ├── session_file()
// ├── entries()
// ├── context_messages()
// ├── copy_context_messages_from()
// ├── copy_context_messages_from_path()
// ├── latest_custom()
// ├── append_session_info()
// ├── open()
// ├── delete()
// ├── rename()
// ├── current_name()
// ├── list_dir()
// ├── list_dirs()
// ├── build_session_meta()
// ├── extract_message_text()
// └── systemtime_to_rfc3339()

use anyhow::{Context, Result};
use rozsa_model::types::Message;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write as _};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const SESSION_VERSION: u32 = 3;

/// Session header written as first line of the session file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionHeader {
    #[serde(rename = "type")]
    pub typ: String,
    pub version: u32,
    pub id: String,
    pub timestamp: String,
    pub cwd: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "parentSession")]
    pub parent_session: Option<String>,
}

/// Base fields shared by all session entries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEntryBase {
    pub id: String,
    #[serde(rename = "parentId")]
    pub parent_id: Option<String>,
    pub timestamp: String,
}

/// Message entry containing an agent message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMessageEntry {
    #[serde(flatten)]
    pub base: SessionEntryBase,
    pub message: Message,
}

/// Thinking level change entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkingLevelChangeEntry {
    #[serde(flatten)]
    pub base: SessionEntryBase,
    #[serde(rename = "thinkingLevel")]
    pub thinking_level: String,
}

/// Model change entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelChangeEntry {
    #[serde(flatten)]
    pub base: SessionEntryBase,
    pub provider: String,
    #[serde(rename = "modelId")]
    pub model_id: String,
}

/// Compaction entry summarizing removed messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionEntry {
    #[serde(flatten)]
    pub base: SessionEntryBase,
    pub summary: String,
    #[serde(rename = "firstKeptEntryId")]
    pub first_kept_entry_id: String,
    #[serde(rename = "tokensBefore")]
    pub tokens_before: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "fromHook")]
    pub from_hook: Option<bool>,
}

/// Custom entry for extension-specific data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomEntry {
    #[serde(flatten)]
    pub base: SessionEntryBase,
    #[serde(rename = "customType")]
    pub custom_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// Label entry for bookmarking entries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabelEntry {
    #[serde(flatten)]
    pub base: SessionEntryBase,
    #[serde(rename = "targetId")]
    pub target_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// Session info entry — carries the user-facing display name. The latest
/// session_info entry in a file wins.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfoEntry {
    #[serde(flatten)]
    pub base: SessionEntryBase,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Union of all session entry types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SessionEntry {
    #[serde(rename = "message")]
    Message(SessionMessageEntry),
    #[serde(rename = "thinking_level_change")]
    ThinkingLevelChange(ThinkingLevelChangeEntry),
    #[serde(rename = "model_change")]
    ModelChange(ModelChangeEntry),
    #[serde(rename = "compaction")]
    Compaction(CompactionEntry),
    #[serde(rename = "custom")]
    Custom(CustomEntry),
    #[serde(rename = "label")]
    Label(LabelEntry),
    #[serde(rename = "session_info")]
    SessionInfo(SessionInfoEntry),
}

impl SessionEntry {
    pub fn id(&self) -> &str {
        match self {
            SessionEntry::Message(e) => &e.base.id,
            SessionEntry::ThinkingLevelChange(e) => &e.base.id,
            SessionEntry::ModelChange(e) => &e.base.id,
            SessionEntry::Compaction(e) => &e.base.id,
            SessionEntry::Custom(e) => &e.base.id,
            SessionEntry::Label(e) => &e.base.id,
            SessionEntry::SessionInfo(e) => &e.base.id,
        }
    }

    fn parent_id(&self) -> Option<&str> {
        match self {
            SessionEntry::Message(e) => e.base.parent_id.as_deref(),
            SessionEntry::ThinkingLevelChange(e) => e.base.parent_id.as_deref(),
            SessionEntry::ModelChange(e) => e.base.parent_id.as_deref(),
            SessionEntry::Compaction(e) => e.base.parent_id.as_deref(),
            SessionEntry::Custom(e) => e.base.parent_id.as_deref(),
            SessionEntry::Label(e) => e.base.parent_id.as_deref(),
            SessionEntry::SessionInfo(e) => e.base.parent_id.as_deref(),
        }
    }
}

/// Lightweight session metadata for list views (Sessions selector UI).
///
/// Built by scanning a session file once: header + counting messages +
/// extracting the first user message and the latest session_info name.
#[derive(Debug, Clone)]
pub struct SessionMeta {
    pub path: PathBuf,
    pub id: String,
    pub cwd: String,
    pub name: Option<String>,
    pub parent_session_path: Option<String>,
    /// RFC3339 timestamp from the session header.
    pub created: String,
    /// RFC3339 timestamp — latest activity (last entry timestamp) or fs mtime fallback.
    pub modified: String,
    pub message_count: u32,
    pub first_message: String,
    /// All assistant + user message text concatenated, used for fuzzy search in the UI.
    pub all_messages_text: String,
}

/// Manages conversation sessions as append-only trees stored in JSONL files.
pub struct SessionManager {
    session_id: String,
    session_file: PathBuf,
    /// In-memory index of entries by ID.
    by_id: HashMap<String, SessionEntry>,
    /// Current leaf pointer (last entry in current branch).
    leaf_id: Option<String>,
    /// CWD stored for lazy header write.
    cwd: String,
    /// Parent session path for lazy header write.
    parent_session: Option<String>,
    /// Whether the session file has been materialized (header written).
    materialized: bool,
}

impl SessionManager {
    /// Return the persisted working directory for this session.
    pub fn cwd(&self) -> &str {
        &self.cwd
    }

    /// Update the session working directory and rewrite the session header.
    pub fn set_cwd(&mut self, cwd: String) -> Result<()> {
        if self.cwd == cwd {
            return Ok(());
        }
        self.ensure_materialized()?;
        let content = std::fs::read_to_string(&self.session_file).with_context(|| {
            format!(
                "Failed to read session file: {}",
                self.session_file.display()
            )
        })?;
        let (header_line, rest) = content
            .split_once('\n')
            .ok_or_else(|| anyhow::anyhow!("Session file has no header line"))?;
        let mut header: SessionHeader = serde_json::from_str(header_line)
            .context("Failed to parse session header while updating cwd")?;
        header.cwd = cwd.clone();
        let updated = format!(
            "{}\n{}",
            serde_json::to_string(&header).context("Failed to serialize session header")?,
            rest
        );
        std::fs::write(&self.session_file, updated).with_context(|| {
            format!(
                "Failed to write session file: {}",
                self.session_file.display()
            )
        })?;
        self.cwd = cwd;
        Ok(())
    }

    /// Create a new session file with header.
    ///
    /// # Arguments
    /// * `path` - Path to the session file to create
    /// * `session_id` - UUID for the session
    /// * `cwd` - Current working directory
    /// * `parent_session` - Optional parent session path (for forked sessions)
    ///
    /// # Returns
    /// Empty SessionManager with no entries, ready for appending.
    pub fn create(
        path: impl AsRef<Path>,
        session_id: String,
        cwd: String,
        parent_session: Option<String>,
    ) -> Result<Self> {
        let path = path.as_ref();

        // Create parent directory if needed
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create session directory: {}", parent.display())
            })?;
        }

        // Create the file and write header
        let file = File::create(path)
            .with_context(|| format!("Failed to create session file: {}", path.display()))?;
        let mut writer = BufWriter::new(file);

        let header = SessionHeader {
            typ: "session".to_string(),
            version: SESSION_VERSION,
            id: session_id.clone(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            cwd: cwd.clone(),
            parent_session: parent_session.clone(),
        };

        let header_json =
            serde_json::to_string(&header).context("Failed to serialize session header")?;
        writeln!(writer, "{}", header_json).context("Failed to write session header")?;
        writer.flush().context("Failed to flush session file")?;

        Ok(SessionManager {
            session_id,
            session_file: path.to_path_buf(),
            by_id: HashMap::new(),
            leaf_id: None,
            cwd,
            parent_session,
            materialized: true,
        })
    }

    /// Create a lazy session manager — file is only created when the first entry is written.
    pub fn create_lazy(
        path: impl AsRef<Path>,
        session_id: String,
        cwd: String,
        parent_session: Option<String>,
    ) -> Self {
        SessionManager {
            session_id,
            session_file: path.as_ref().to_path_buf(),
            by_id: HashMap::new(),
            leaf_id: None,
            cwd,
            parent_session,
            materialized: false,
        }
    }

    /// Generate a short UUID (8 hex chars) that doesn't collide with existing entries.
    fn generate_id(&self) -> String {
        for _ in 0..100 {
            let id = uuid::Uuid::new_v4().to_string();
            let short_id = &id[..8];
            if !self.by_id.contains_key(short_id) {
                return short_id.to_string();
            }
        }
        // Fallback to full UUID if we somehow have collisions
        uuid::Uuid::new_v4().to_string()
    }

    /// Materialize the session file (write header) if not yet created.
    fn ensure_materialized(&mut self) -> Result<()> {
        if self.materialized {
            return Ok(());
        }

        if let Some(parent) = self.session_file.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create session directory: {}", parent.display())
            })?;
        }

        let file = File::create(&self.session_file).with_context(|| {
            format!(
                "Failed to create session file: {}",
                self.session_file.display()
            )
        })?;
        let mut writer = BufWriter::new(file);

        let header = SessionHeader {
            typ: "session".to_string(),
            version: SESSION_VERSION,
            id: self.session_id.clone(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            cwd: self.cwd.clone(),
            parent_session: self.parent_session.clone(),
        };

        let header_json =
            serde_json::to_string(&header).context("Failed to serialize session header")?;
        writeln!(writer, "{}", header_json).context("Failed to write session header")?;
        writer.flush().context("Failed to flush session file")?;

        self.materialized = true;
        Ok(())
    }

    /// Append an entry to the session file and update internal state.
    fn append_entry(&mut self, entry: SessionEntry) -> Result<String> {
        self.ensure_materialized()?;

        let id = entry.id().to_string();

        // Serialize and append to file
        let entry_json =
            serde_json::to_string(&entry).context("Failed to serialize session entry")?;

        let mut file = OpenOptions::new()
            .append(true)
            .open(&self.session_file)
            .with_context(|| {
                format!(
                    "Failed to open session file for append: {}",
                    self.session_file.display()
                )
            })?;

        writeln!(file, "{}", entry_json).context("Failed to append entry to session file")?;

        // Update internal state
        self.by_id.insert(id.clone(), entry);
        self.leaf_id = Some(id.clone());

        Ok(id)
    }

    /// Append a message as child of current leaf.
    ///
    /// # Returns
    /// The ID of the newly created entry.
    pub fn append_message(&mut self, message: Message) -> Result<String> {
        let id = self.generate_id();
        let entry = SessionEntry::Message(SessionMessageEntry {
            base: SessionEntryBase {
                id,
                parent_id: self.leaf_id.clone(),
                timestamp: chrono::Utc::now().to_rfc3339(),
            },
            message,
        });
        self.append_entry(entry)
    }

    /// Append a compaction summary as child of current leaf.
    ///
    /// # Arguments
    /// * `summary` - Human-readable summary of removed messages
    /// * `first_kept_entry_id` - ID of the first entry kept after compaction
    /// * `tokens_before` - Token count before compaction
    /// * `details` - Optional extension-specific metadata
    /// * `from_hook` - True if generated by an extension
    ///
    /// # Returns
    /// The ID of the newly created entry.
    pub fn append_compaction(
        &mut self,
        summary: String,
        first_kept_entry_id: String,
        tokens_before: u64,
        details: Option<serde_json::Value>,
        from_hook: Option<bool>,
    ) -> Result<String> {
        let id = self.generate_id();
        let entry = SessionEntry::Compaction(CompactionEntry {
            base: SessionEntryBase {
                id,
                parent_id: self.leaf_id.clone(),
                timestamp: chrono::Utc::now().to_rfc3339(),
            },
            summary,
            first_kept_entry_id,
            tokens_before,
            details,
            from_hook,
        });
        self.append_entry(entry)
    }

    /// Append a model change as child of current leaf.
    ///
    /// # Returns
    /// The ID of the newly created entry.
    pub fn append_model_change(&mut self, provider: String, model_id: String) -> Result<String> {
        let id = self.generate_id();
        let entry = SessionEntry::ModelChange(ModelChangeEntry {
            base: SessionEntryBase {
                id,
                parent_id: self.leaf_id.clone(),
                timestamp: chrono::Utc::now().to_rfc3339(),
            },
            provider,
            model_id,
        });
        self.append_entry(entry)
    }

    /// Append a thinking level change as child of current leaf.
    ///
    /// # Returns
    /// The ID of the newly created entry.
    pub fn append_thinking_level_change(&mut self, level: String) -> Result<String> {
        let id = self.generate_id();
        let entry = SessionEntry::ThinkingLevelChange(ThinkingLevelChangeEntry {
            base: SessionEntryBase {
                id,
                parent_id: self.leaf_id.clone(),
                timestamp: chrono::Utc::now().to_rfc3339(),
            },
            thinking_level: level,
        });
        self.append_entry(entry)
    }

    /// Append a custom entry (for extension-specific data) as child of current leaf.
    ///
    /// # Returns
    /// The ID of the newly created entry.
    pub fn append_custom(
        &mut self,
        custom_type: String,
        payload: Option<serde_json::Value>,
    ) -> Result<String> {
        let id = self.generate_id();
        let entry = SessionEntry::Custom(CustomEntry {
            base: SessionEntryBase {
                id,
                parent_id: self.leaf_id.clone(),
                timestamp: chrono::Utc::now().to_rfc3339(),
            },
            custom_type,
            data: payload,
        });
        self.append_entry(entry)
    }

    /// Append a label entry as child of current leaf.
    ///
    /// # Arguments
    /// * `target_id` - ID of the entry to label
    /// * `label` - Label text, or None to clear the label
    ///
    /// # Returns
    /// The ID of the newly created entry.
    pub fn append_label(&mut self, target_id: String, label: Option<String>) -> Result<String> {
        // Verify target exists
        if !self.by_id.contains_key(&target_id) {
            anyhow::bail!("Entry {} not found", target_id);
        }

        let id = self.generate_id();
        let entry = SessionEntry::Label(LabelEntry {
            base: SessionEntryBase {
                id,
                parent_id: self.leaf_id.clone(),
                timestamp: chrono::Utc::now().to_rfc3339(),
            },
            target_id,
            label,
        });
        self.append_entry(entry)
    }

    /// Get the current leaf ID.
    pub fn leaf_id(&self) -> Option<&str> {
        self.leaf_id.as_deref()
    }

    /// Get the session ID.
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Get the session file path.
    pub fn session_file(&self) -> &Path {
        &self.session_file
    }

    /// Get all entries in the session (for compaction planning).
    pub fn entries(&self) -> Vec<SessionEntry> {
        let mut entries = Vec::new();
        let mut current = self.leaf_id.as_deref();
        while let Some(id) = current {
            let Some(entry) = self.by_id.get(id) else {
                break;
            };
            entries.push(entry.clone());
            current = entry.parent_id();
        }
        entries.reverse();
        entries
    }

    /// Return the persisted standard messages on the active session branch.
    ///
    /// Session metadata entries are intentionally excluded. The returned
    /// order is the order the agent should see when continuing the branch.
    pub fn context_messages(&self) -> Vec<Message> {
        self.entries()
            .into_iter()
            .filter_map(|entry| match entry {
                SessionEntry::Message(message) => Some(message.message),
                _ => None,
            })
            .collect()
    }

    /// Copy the source branch's persisted messages into this session.
    pub fn copy_context_messages_from(&mut self, source: &SessionManager) -> Result<()> {
        for message in source.context_messages() {
            self.append_message(message)?;
        }
        Ok(())
    }

    /// Open another session file and copy its persisted messages into this
    /// session. This is used when creating a continued child session.
    pub fn copy_context_messages_from_path(&mut self, path: impl AsRef<Path>) -> Result<()> {
        let source = SessionManager::open(path)?;
        self.copy_context_messages_from(&source)
    }

    /// Return the latest custom entry of `custom_type` by persisted timestamp.
    pub fn latest_custom(&self, custom_type: &str) -> Option<CustomEntry> {
        self.by_id
            .values()
            .filter_map(|entry| match entry {
                SessionEntry::Custom(custom) if custom.custom_type == custom_type => Some(custom),
                _ => None,
            })
            .max_by(|left, right| left.base.timestamp.cmp(&right.base.timestamp))
            .cloned()
    }

    /// Append a session_info entry recording the user-facing display name.
    /// Pass `None` to clear the name. The latest entry wins on read.
    pub fn append_session_info(&mut self, name: Option<String>) -> Result<String> {
        let id = self.generate_id();
        let entry = SessionEntry::SessionInfo(SessionInfoEntry {
            base: SessionEntryBase {
                id,
                parent_id: self.leaf_id.clone(),
                timestamp: chrono::Utc::now().to_rfc3339(),
            },
            name,
        });
        self.append_entry(entry)
    }

    /// Open an existing session file and rebuild internal state.
    ///
    /// Reads the header for `session_id` and replays all entries to rebuild
    /// the `by_id` index and locate the most recent leaf.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let file = File::open(path)
            .with_context(|| format!("Failed to open session file: {}", path.display()))?;
        let reader = BufReader::new(file);

        let mut header: Option<SessionHeader> = None;
        let mut by_id: HashMap<String, SessionEntry> = HashMap::new();
        let mut last_id: Option<String> = None;

        for (idx, line) in reader.lines().enumerate() {
            let line = line.with_context(|| {
                format!("Failed to read line {} of {}", idx + 1, path.display())
            })?;
            if line.trim().is_empty() {
                continue;
            }
            if header.is_none() {
                let h: SessionHeader = serde_json::from_str(&line)
                    .with_context(|| format!("Invalid session header in {}", path.display()))?;
                if h.typ != "session" {
                    anyhow::bail!("Missing 'session' header in {}", path.display());
                }
                header = Some(h);
                continue;
            }
            // Skip malformed lines but keep going (best effort).
            let Ok(entry) = serde_json::from_str::<SessionEntry>(&line) else {
                continue;
            };
            let id = entry.id().to_string();
            by_id.insert(id.clone(), entry);
            last_id = Some(id);
        }

        let header = header.ok_or_else(|| {
            anyhow::anyhow!("Session file {} is empty or has no header", path.display())
        })?;

        Ok(SessionManager {
            session_id: header.id,
            session_file: path.to_path_buf(),
            by_id,
            leaf_id: last_id,
            cwd: header.cwd,
            parent_session: header.parent_session,
            materialized: true,
        })
    }

    /// Delete the session file at `path`.
    pub fn delete(path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        std::fs::remove_file(path)
            .with_context(|| format!("Failed to delete session file: {}", path.display()))
    }

    /// Rename a session by appending a `session_info` entry.
    ///
    /// `path` may point to the active session or a different file. For non-active
    /// sessions we open them, append, and drop the manager. For an in-process
    /// active rename, callers should use `append_session_info` directly.
    pub fn rename(path: impl AsRef<Path>, new_name: Option<String>) -> Result<()> {
        let mut mgr = SessionManager::open(path)?;
        mgr.append_session_info(new_name)?;
        Ok(())
    }

    /// Resolve the latest `session_info` name, walking entries by appended order.
    pub fn current_name(&self) -> Option<String> {
        // Iterate in reverse insertion order is not preserved by HashMap; we
        // pick the entry with the largest timestamp instead.
        let mut best: Option<(&str, &SessionInfoEntry)> = None;
        for entry in self.by_id.values() {
            if let SessionEntry::SessionInfo(info) = entry {
                let ts = info.base.timestamp.as_str();
                match best {
                    None => best = Some((ts, info)),
                    Some((cur, _)) if ts > cur => best = Some((ts, info)),
                    _ => {}
                }
            }
        }
        best.and_then(|(_, info)| info.name.clone())
    }

    /// List all session files under a directory, returning lightweight metadata.
    /// Files that fail to parse are skipped silently.
    pub fn list_dir(dir: impl AsRef<Path>) -> Result<Vec<SessionMeta>> {
        let dir = dir.as_ref();
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut metas = Vec::new();
        for entry in std::fs::read_dir(dir)
            .with_context(|| format!("Failed to read session dir: {}", dir.display()))?
        {
            let Ok(entry) = entry else { continue };
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
                continue;
            }
            if let Ok(meta) = build_session_meta(&path) {
                metas.push(meta);
            }
        }
        // Most recent first.
        metas.sort_by(|a, b| b.modified.cmp(&a.modified));
        Ok(metas)
    }

    /// List layered session directories. Later directories override earlier
    /// directories when the same session id exists in both.
    pub fn list_dirs(dirs: &[PathBuf]) -> Result<Vec<SessionMeta>> {
        let mut by_id = HashMap::new();
        for dir in dirs {
            for meta in Self::list_dir(dir)? {
                by_id.insert(meta.id.clone(), meta);
            }
        }
        let mut metas = by_id.into_values().collect::<Vec<_>>();
        metas.sort_by(|left, right| right.modified.cmp(&left.modified));
        Ok(metas)
    }
}

/// Scan a single session file and produce its [`SessionMeta`] summary.
/// Returns Err if the file is missing a valid header — caller may skip.
fn build_session_meta(path: &Path) -> Result<SessionMeta> {
    let file = File::open(path).with_context(|| format!("Failed to open {}", path.display()))?;
    let reader = BufReader::new(file);

    let mut header: Option<SessionHeader> = None;
    let mut message_count: u32 = 0;
    let mut first_message = String::new();
    let mut all_messages_text = String::new();
    let mut latest_name: Option<(String, Option<String>)> = None; // (timestamp, name)
    let mut latest_activity: Option<String> = None;

    for line in reader.lines() {
        let Ok(line) = line else { continue };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if header.is_none() {
            let h: SessionHeader = serde_json::from_str(trimmed)
                .with_context(|| format!("Invalid header in {}", path.display()))?;
            if h.typ != "session" {
                anyhow::bail!("Missing 'session' header in {}", path.display());
            }
            header = Some(h);
            continue;
        }

        let Ok(entry) = serde_json::from_str::<SessionEntry>(trimmed) else {
            continue;
        };

        // Track latest activity timestamp via base timestamp.
        let ts = match &entry {
            SessionEntry::Message(e) => &e.base.timestamp,
            SessionEntry::ThinkingLevelChange(e) => &e.base.timestamp,
            SessionEntry::ModelChange(e) => &e.base.timestamp,
            SessionEntry::Compaction(e) => &e.base.timestamp,
            SessionEntry::Custom(e) => &e.base.timestamp,
            SessionEntry::Label(e) => &e.base.timestamp,
            SessionEntry::SessionInfo(e) => &e.base.timestamp,
        };
        match &latest_activity {
            None => latest_activity = Some(ts.clone()),
            Some(prev) if ts.as_str() > prev.as_str() => latest_activity = Some(ts.clone()),
            _ => {}
        }

        match entry {
            SessionEntry::SessionInfo(info) => {
                let info_ts = info.base.timestamp.clone();
                match &latest_name {
                    None => latest_name = Some((info_ts, info.name)),
                    Some((prev_ts, _)) if info_ts.as_str() > prev_ts.as_str() => {
                        latest_name = Some((info_ts, info.name));
                    }
                    _ => {}
                }
            }
            SessionEntry::Message(msg_entry) => {
                message_count += 1;
                let text = extract_message_text(&msg_entry.message);
                if !text.is_empty() {
                    if first_message.is_empty() && matches!(msg_entry.message, Message::User(_)) {
                        first_message = text.clone();
                    }
                    if !all_messages_text.is_empty() {
                        all_messages_text.push(' ');
                    }
                    all_messages_text.push_str(&text);
                }
            }
            _ => {}
        }
    }

    let header =
        header.ok_or_else(|| anyhow::anyhow!("No session header in {}", path.display()))?;

    let modified = latest_activity.unwrap_or_else(|| {
        // Fallback to fs mtime in RFC3339.
        std::fs::metadata(path)
            .ok()
            .and_then(|m| m.modified().ok())
            .map(systemtime_to_rfc3339)
            .unwrap_or_else(|| header.timestamp.clone())
    });

    Ok(SessionMeta {
        path: path.to_path_buf(),
        id: header.id.clone(),
        cwd: header.cwd.clone(),
        name: latest_name
            .and_then(|(_, n)| n)
            .filter(|s| !s.trim().is_empty()),
        parent_session_path: header.parent_session.clone(),
        created: header.timestamp,
        modified,
        message_count,
        first_message: if first_message.is_empty() {
            "(no messages)".to_string()
        } else {
            first_message
        },
        all_messages_text,
    })
}

/// Extract plain text from a Message for indexing/preview.
fn extract_message_text(msg: &Message) -> String {
    use rozsa_model::types::{AssistantMessage, ContentBlock, UserContent};
    match msg {
        Message::User(u) => match &u.content {
            UserContent::Text(t) => t.clone(),
            UserContent::Blocks(blocks) => blocks
                .iter()
                .filter_map(|b| {
                    if let ContentBlock::Text { text, .. } = b {
                        Some(text.clone())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join(" "),
        },
        Message::Assistant(AssistantMessage { content, .. }) => content
            .iter()
            .filter_map(|b| {
                if let ContentBlock::Text { text, .. } = b {
                    Some(text.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join(" "),
        _ => String::new(),
    }
}

fn systemtime_to_rfc3339(t: SystemTime) -> String {
    let dt: chrono::DateTime<chrono::Utc> = t.into();
    dt.to_rfc3339()
}
