// File: permissions/mod.rs
//
// Internal Framework:
// permissions/mod.rs
// ├── PermissionMode           # on-request / auto-approve / free-permission
// ├── RiskLevel                # read / write / shell / destructive
// ├── PolicyVerdict            # allow / block / need-approval
// ├── ApprovalInfo             # data for UI prompt
// ├── PermissionResponse       # user's decision (allow / allow-session / deny)
// ├── PendingApprovals         # Arc<DashMap<id, oneshot::Sender>>
// ├── PermissionPolicy         # pure logic: blacklist + pattern + session memory
// └── split_shell_segments()   # pipe/&&/|| command splitting
//
// Related Docs:
// - [Gap Audit](../../../docs/NATIVE_TUI_GAP_AUDIT.md)
// - [Settings Schema](./settings/schema.rs)

pub mod audit;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use dashmap::DashMap;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::oneshot;

use crate::settings::SettingsManager;

/// Runtime-owned permission state shared by GUI-created sessions.
/// Configuration is replaceable; session trust never leaks across session ids.
pub struct PermissionController {
    config: std::sync::RwLock<PermissionConfig>,
    session_approvals: DashMap<String, HashSet<String>>,
    workspace_root: PathBuf,
    settings_manager: Option<SettingsManager>,
}

#[derive(Clone)]
struct PermissionConfig {
    mode: PermissionMode,
    auto_approve_patterns: Vec<String>,
    allowed_tools: Vec<String>,
    blocked_commands: Vec<String>,
    deny: Vec<String>,
    ask: Vec<String>,
    allow: Vec<String>,
}

impl PermissionController {
    pub fn new(
        mode: PermissionMode,
        auto_approve_patterns: Vec<String>,
        allowed_tools: Vec<String>,
        blocked_commands: Vec<String>,
    ) -> Self {
        Self {
            config: std::sync::RwLock::new(PermissionConfig {
                mode,
                auto_approve_patterns,
                allowed_tools,
                blocked_commands,
                deny: Vec::new(),
                ask: Vec::new(),
                allow: Vec::new(),
            }),
            session_approvals: DashMap::new(),
            workspace_root: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            settings_manager: None,
        }
    }

    pub fn with_project_rules(
        mode: PermissionMode,
        auto_approve_patterns: Vec<String>,
        allowed_tools: Vec<String>,
        blocked_commands: Vec<String>,
        deny: Vec<String>,
        ask: Vec<String>,
        allow: Vec<String>,
        workspace_root: PathBuf,
        settings_manager: SettingsManager,
    ) -> Self {
        Self {
            config: std::sync::RwLock::new(PermissionConfig {
                mode,
                auto_approve_patterns,
                allowed_tools,
                blocked_commands,
                deny,
                ask,
                allow,
            }),
            session_approvals: DashMap::new(),
            workspace_root,
            settings_manager: Some(settings_manager),
        }
    }

    pub fn update(
        &self,
        mode: PermissionMode,
        auto_approve_patterns: Vec<String>,
        allowed_tools: Vec<String>,
        blocked_commands: Vec<String>,
    ) {
        let mut config = self.config.write().unwrap();
        config.mode = mode;
        config.auto_approve_patterns = auto_approve_patterns;
        config.allowed_tools = allowed_tools;
        config.blocked_commands = blocked_commands;
    }

    pub fn evaluate(&self, session_id: &str, tool_name: &str, args: &Value) -> PolicyVerdict {
        let config = self.config.read().unwrap().clone();
        let policy = PermissionPolicy::new(
            config.mode,
            config.auto_approve_patterns,
            config.allowed_tools,
            config.blocked_commands,
        );
        let verdict = policy.evaluate(tool_name, args);
        if matches!(verdict, PolicyVerdict::Block { .. }) {
            return verdict;
        }

        if rules_match_any(&config.deny, tool_name, args, &self.workspace_root) {
            return PolicyVerdict::Block {
                reason: "blocked by permission.deny rule".to_string(),
            };
        }
        if rules_match_any(&config.ask, tool_name, args, &self.workspace_root) {
            return PolicyVerdict::NeedApproval {
                info: approval_info(tool_name, args, Vec::new()),
            };
        }
        if rules_cover_request(&config.allow, tool_name, args, &self.workspace_root) {
            return PolicyVerdict::Allow;
        }

        let Some(keys) = self.session_approvals.get(session_id) else {
            return verdict;
        };
        if request_matches_session_approval(&keys, tool_name, args) {
            return PolicyVerdict::Allow;
        }

        match verdict {
            PolicyVerdict::NeedApproval { mut info } => {
                info.trust_levels = untrusted_trust_levels(&keys, tool_name, args);
                info.trust_key = info
                    .trust_levels
                    .first()
                    .map(|level| level.key.clone())
                    .unwrap_or_else(|| request_trust_key(tool_name, args));
                PolicyVerdict::NeedApproval { info }
            }
            other => other,
        }
    }

    pub fn record_session_approval(&self, session_id: &str, trust_key: String) {
        self.session_approvals
            .entry(session_id.to_string())
            .or_default()
            .insert(trust_key);
    }

    pub fn record_project_approval(&self, trust_key: &str) -> Result<(), String> {
        let rule = trust_key_to_project_rule(trust_key)?;
        {
            let mut config = self.config.write().unwrap();
            if !config.allow.contains(&rule) {
                config.allow.push(rule.clone());
            }
        }
        if let Some(settings_manager) = &self.settings_manager {
            settings_manager
                .add_project_permission_allow(&rule)
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// PermissionMode
// ---------------------------------------------------------------------------

/// 权限模式：控制工具调用时的审批策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PermissionMode {
    /// 每次工具调用都需要用户审批。
    OnRequest,
    /// 匹配 auto-approve 模式的工具调用自动通过，其余需要审批。
    AutoApprove,
    /// 所有调用直接通过，不做任何检查（仅限受信环境）。
    FreePermission,
}

impl PermissionMode {
    /// 从字符串解析权限模式。
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "on-request" => Some(Self::OnRequest),
            "auto-permission" => Some(Self::AutoApprove),
            "free-permission" => Some(Self::FreePermission),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// RiskLevel
// ---------------------------------------------------------------------------

/// 工具调用的风险等级。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    /// 只读操作（Read, Grep, Ls, Find）。
    Read,
    /// 写操作（Write, Edit）。
    Write,
    /// Shell 命令执行。
    Shell,
    /// 网络操作（curl, wget, npm install）。
    Network,
    /// Git 操作。
    Git,
    /// 未知工具。
    Unknown,
    /// 已知高风险操作（rm -rf, secret files, outside workspace）。
    Destructive,
}

// ---------------------------------------------------------------------------
// PolicyVerdict
// ---------------------------------------------------------------------------

/// 权限策略对一次工具调用的裁定结果。
#[derive(Debug, Clone)]
pub enum PolicyVerdict {
    /// 直接放行。
    Allow,
    /// 拒绝执行。
    Block { reason: String },
    /// 需要用户审批。
    NeedApproval { info: ApprovalInfo },
}

// ---------------------------------------------------------------------------
// ApprovalInfo
// ---------------------------------------------------------------------------

/// 提交给 UI 的审批请求数据。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalInfo {
    pub tool_name: String,
    pub args_summary: String,
    pub risk: RiskLevel,
    pub trust_key: String,
    /// User-selectable session scopes. Compound shell commands contain only
    /// scopes for segments that are not already trusted.
    pub trust_levels: Vec<TrustLevel>,
}

/// A trust scope that can be selected in the approval UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustLevel {
    pub label: String,
    pub key: String,
}

// ---------------------------------------------------------------------------
// PermissionResponse
// ---------------------------------------------------------------------------

/// 用户对审批请求的回复。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PermissionResponse {
    /// 仅本次允许。
    Allow,
    /// 本会话内对相同 trust_key 自动允许。
    AllowSession { trust_key: String },
    /// 拒绝执行。
    Deny,
}

// ---------------------------------------------------------------------------
// PendingApprovals
// ---------------------------------------------------------------------------

/// 正在等待用户审批的请求映射：request_id -> oneshot sender。
pub type PendingApprovals = Arc<DashMap<String, oneshot::Sender<PermissionResponse>>>;

// ---------------------------------------------------------------------------
// Read-only tool whitelist (auto-allow without prompting)
// ---------------------------------------------------------------------------

const WORKSPACE_READ_TOOLS: &[&str] = &["Read", "Grep", "Ls", "Find", "read", "grep", "ls", "find"];

// ---------------------------------------------------------------------------
// PermissionPolicy
// ---------------------------------------------------------------------------

/// Callback invoked when a session approval is recorded — used to persist to settings.
pub type OnApprovalCallback = Box<dyn Fn(&str) + Send + Sync>;

/// 纯逻辑权限策略：黑名单 + 自动审批模式 + 会话记忆。
///
/// 不涉及 I/O，所有判断基于内存状态。Send + Sync（使用 std::sync::Mutex）。
pub struct PermissionPolicy {
    mode: PermissionMode,
    blacklist: Vec<(Regex, &'static str)>,
    auto_approve_patterns: Vec<Regex>,
    allowed_tools: Vec<String>,
    blocked_commands: Vec<String>,
    session_approvals: Mutex<HashSet<String>>,
    on_approval: Option<OnApprovalCallback>,
}

impl PermissionPolicy {
    /// 创建新的权限策略实例。
    ///
    /// `auto_approve_patterns` 中的字符串会编译为正则，匹配 trust_key。
    /// `allowed_tools` 是工具名称列表，匹配的工具直接放行（例如 ["read", "grep"]）。
    /// `blocked_commands` 是命令前缀列表，匹配的命令直接拒绝（例如 ["rm -rf", "git push --force"]）。
    pub fn new(
        mode: PermissionMode,
        auto_approve_patterns: Vec<String>,
        allowed_tools: Vec<String>,
        blocked_commands: Vec<String>,
    ) -> Self {
        let blacklist = build_hardcoded_blacklist();

        let auto_approve_patterns = auto_approve_patterns
            .iter()
            .filter_map(|p| Regex::new(p).ok())
            .collect();

        Self {
            mode,
            blacklist,
            auto_approve_patterns,
            allowed_tools,
            blocked_commands,
            session_approvals: Mutex::new(HashSet::new()),
            on_approval: None,
        }
    }

    /// Set a callback that will be invoked with each trust_key when session approval is recorded.
    /// Used to persist approvals to settings files.
    pub fn set_on_approval(&mut self, callback: OnApprovalCallback) {
        self.on_approval = Some(callback);
    }

    /// 评估一次工具调用是否允许执行。
    pub fn evaluate(&self, tool_name: &str, args: &Value) -> PolicyVerdict {
        // blocked_commands 检查：对 Bash/bash 工具检查命令前缀。
        if tool_name == "Bash" || tool_name == "bash" {
            if let Some(cmd) = args.get("command").and_then(|v| v.as_str()) {
                for blocked_prefix in &self.blocked_commands {
                    if cmd.trim_start().starts_with(blocked_prefix.as_str()) {
                        return PolicyVerdict::Block {
                            reason: format!("blocked by blocked_commands: {}", blocked_prefix),
                        };
                    }
                }
            }
        }

        // 黑名单检查（仅对 Bash/bash 工具的 command 参数）。
        if (tool_name == "Bash" || tool_name == "bash")
            && let Some(cmd) = args.get("command").and_then(|v| v.as_str())
            && let Some(reason) = check_blacklist_with_segments(cmd, &self.blacklist)
        {
            return PolicyVerdict::Block {
                reason: reason.to_string(),
            };
        }

        // FreePermission 与 allowed_tools 不得绕过硬编码或用户配置的拦截。
        if self.mode == PermissionMode::FreePermission
            || self.allowed_tools.iter().any(|name| name == tool_name)
        {
            return PolicyVerdict::Allow;
        }

        // 工作区只读工具自动放行（不需要用户审批）。
        if WORKSPACE_READ_TOOLS.contains(&tool_name) {
            return PolicyVerdict::Allow;
        }

        // 复合 shell 命令必须逐段命中已授予的 trust，不能由第一段放行整条命令。
        {
            let approvals = self.session_approvals.lock().unwrap();
            if request_matches_session_approval(&approvals, tool_name, args) {
                return PolicyVerdict::Allow;
            }
        }

        // AutoApprove 的规则也必须覆盖每个 shell 段。
        if self.mode == PermissionMode::AutoApprove
            && patterns_cover_request(&self.auto_approve_patterns, tool_name, args)
        {
            return PolicyVerdict::Allow;
        }

        // 其余情况需要审批。
        let risk = infer_risk_level(tool_name, args);
        let args_summary = summarize_args(tool_name, args);

        let approvals = self.session_approvals.lock().unwrap();
        let trust_levels = untrusted_trust_levels(&approvals, tool_name, args);
        let trust_key = trust_levels
            .first()
            .map(|level| level.key.clone())
            .unwrap_or_else(|| request_trust_key(tool_name, args));

        PolicyVerdict::NeedApproval {
            info: ApprovalInfo {
                tool_name: tool_name.to_string(),
                args_summary,
                risk,
                trust_key,
                trust_levels,
            },
        }
    }

    /// 记录会话级审批：后续相同 trust_key 自动放行。
    /// 如果设置了 on_approval 回调，同时触发持久化。
    pub fn record_session_approval(&self, trust_key: String) {
        {
            let mut approvals = self.session_approvals.lock().unwrap();
            approvals.insert(trust_key.clone());
        }
        if let Some(ref callback) = self.on_approval {
            callback(&trust_key);
        }
    }

    /// 获取当前权限模式。
    pub fn mode(&self) -> PermissionMode {
        self.mode
    }
}

// ---------------------------------------------------------------------------
// Hardcoded blacklist (aligned with TS HARDCODED_BLACKLIST)
// ---------------------------------------------------------------------------

fn build_hardcoded_blacklist() -> Vec<(Regex, &'static str)> {
    vec![
        // rm -rf with dangerous targets (/, ~, $HOME, ., *)
        (
            Regex::new(r"\brm\s+-[^\n;]*r[^\n;]*f[^\n;]*(/|~|\$HOME|\.|\*)(\b|$)").unwrap(),
            "destructive: rm -rf on dangerous path",
        ),
        // rm with wildcards
        (
            Regex::new(r"\brm\s+[^\n;]*\*").unwrap(),
            "destructive: rm with wildcards",
        ),
        // rm targeting current directory
        (
            Regex::new(r"\brm\s+-[^\n;]*\s+\.(?:\b|$|/)").unwrap(),
            "destructive: rm on current directory",
        ),
        // sudo
        (
            Regex::new(r"^\s*sudo\b").unwrap(),
            "sudo requires manual execution",
        ),
        // git reset --hard
        (
            Regex::new(r"\bgit\s+reset\s+--hard\b").unwrap(),
            "destructive: git reset --hard",
        ),
        // git clean -fd
        (
            Regex::new(r"\bgit\s+clean\s+-fd\b").unwrap(),
            "destructive: git clean -fd",
        ),
        // git push --force / -f
        (
            Regex::new(r"\bgit\s+push\b[^\n;]*(--force|-f)\b").unwrap(),
            "destructive: force push",
        ),
        // dd (disk dump)
        (
            Regex::new(r"\bdd\b").unwrap(),
            "destructive: dd disk utility",
        ),
        // mkfs (filesystem format)
        (
            Regex::new(r"\bmkfs\b").unwrap(),
            "destructive: filesystem format",
        ),
        // diskutil erase (macOS)
        (
            Regex::new(r"\bdiskutil\s+erase\b").unwrap(),
            "destructive: diskutil erase",
        ),
    ]
}

// ---------------------------------------------------------------------------
// Shell segment splitting (I038)
// ---------------------------------------------------------------------------

/// 将 shell 命令按 pipe (|) 和逻辑运算符 (&&, ||, ;) 拆分为独立段。
///
/// 每段独立检查黑名单，防止通过 `innocent | dangerous` 绕过。
pub fn split_shell_segments(command: &str) -> Vec<&str> {
    let mut segments = Vec::new();
    let mut start = 0;
    let bytes = command.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        match bytes[i] {
            // Skip quoted strings
            b'\'' => {
                i += 1;
                while i < len && bytes[i] != b'\'' {
                    i += 1;
                }
                i += 1;
            }
            b'"' => {
                i += 1;
                while i < len {
                    if bytes[i] == b'\\' {
                        i += 2;
                    } else if bytes[i] == b'"' {
                        i += 1;
                        break;
                    } else {
                        i += 1;
                    }
                }
            }
            // Pipe
            b'|' => {
                if i + 1 < len && bytes[i + 1] == b'|' {
                    // ||
                    segments.push(command[start..i].trim());
                    i += 2;
                    start = i;
                } else {
                    // |
                    segments.push(command[start..i].trim());
                    i += 1;
                    start = i;
                }
            }
            // &&
            b'&' => {
                if i + 1 < len && bytes[i + 1] == b'&' {
                    segments.push(command[start..i].trim());
                    i += 2;
                    start = i;
                } else {
                    i += 1;
                }
            }
            // ;
            b';' => {
                segments.push(command[start..i].trim());
                i += 1;
                start = i;
            }
            _ => {
                i += 1;
            }
        }
    }

    let last = command[start..].trim();
    if !last.is_empty() {
        segments.push(last);
    }

    segments
}

/// 对完整命令及其各段逐一检查黑名单，返回第一个命中的 reason。
fn check_blacklist_with_segments<'a>(
    command: &str,
    blacklist: &'a [(Regex, &'static str)],
) -> Option<&'a str> {
    // Check full command first
    for (pattern, reason) in blacklist {
        if pattern.is_match(command) {
            return Some(reason);
        }
    }

    // Split and check each segment independently
    let segments = split_shell_segments(command);
    if segments.len() > 1 {
        for segment in &segments {
            for (pattern, reason) in blacklist {
                if pattern.is_match(segment) {
                    return Some(reason);
                }
            }
        }
    }

    // Deep analysis: check subcommands and sensitive patterns
    if let Some(reason) = check_command_deep(command, blacklist) {
        return Some(reason);
    }

    None
}

// ---------------------------------------------------------------------------
// Deep command analysis (I042)
// ---------------------------------------------------------------------------

/// Sensitive environment variable patterns — leaking these is blocked.
static SENSITIVE_ENV_VARS: &[&str] = &[
    "AWS_SECRET_ACCESS_KEY",
    "AWS_SESSION_TOKEN",
    "ANTHROPIC_API_KEY",
    "OPENAI_API_KEY",
    "GITHUB_TOKEN",
    "GH_TOKEN",
    "GOOGLE_API_KEY",
    "API_KEY",
    "API_SECRET",
    "SECRET_KEY",
    "PRIVATE_KEY",
    "DATABASE_URL",
    "DB_PASSWORD",
    "NPM_TOKEN",
    "PYPI_TOKEN",
];

/// Deep command analysis: detects subcommand injection and env var leaks.
fn check_command_deep<'a>(
    command: &str,
    blacklist: &'a [(Regex, &'static str)],
) -> Option<&'a str> {
    // 1. Extract $(...) subcommands and check each against blacklist.
    for subcmd in extract_subcommands(command) {
        for (pattern, reason) in blacklist {
            if pattern.is_match(&subcmd) {
                return Some(reason);
            }
        }
    }

    // 2. Detect sensitive env var leaks (echo $SECRET_KEY, curl ... $API_KEY).
    if has_sensitive_env_leak(command) {
        return Some("blocked: potential sensitive environment variable leak");
    }

    None
}

/// Extract subcommands from $(...) and `...` constructs.
fn extract_subcommands(command: &str) -> Vec<String> {
    let mut subs = Vec::new();
    let bytes = command.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        // $(...) — nested parentheses
        if i + 1 < len && bytes[i] == b'$' && bytes[i + 1] == b'(' {
            i += 2;
            let start = i;
            let mut depth = 1;
            while i < len && depth > 0 {
                if bytes[i] == b'(' {
                    depth += 1;
                } else if bytes[i] == b')' {
                    depth -= 1;
                }
                if depth > 0 {
                    i += 1;
                }
            }
            if start < i {
                subs.push(command[start..i].to_string());
            }
            i += 1; // skip closing )
        }
        // `...` backtick substitution
        else if bytes[i] == b'`' {
            i += 1;
            let start = i;
            while i < len && bytes[i] != b'`' {
                i += 1;
            }
            if start < i {
                subs.push(command[start..i].to_string());
            }
            i += 1; // skip closing `
        } else {
            i += 1;
        }
    }

    subs
}

/// Check if command leaks sensitive environment variables.
fn has_sensitive_env_leak(command: &str) -> bool {
    for var in SENSITIVE_ENV_VARS {
        // Check $VAR and ${VAR} patterns in context of echo/printf/curl/wget
        let dollar_var = format!("${}", var);
        let brace_var = format!("${{{}}}", var);
        if command.contains(&dollar_var) || command.contains(&brace_var) {
            // Only flag if in a context that would expose it (echo, curl, wget, etc.)
            let leak_prefixes = ["echo", "printf", "curl", "wget", "cat", "tee"];
            for prefix in leak_prefixes {
                if command.contains(prefix) {
                    return true;
                }
            }
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Public helpers
// ---------------------------------------------------------------------------

/// 根据工具名称和参数判定风险等级。
#[doc(hidden)]
pub fn classify_risk(tool_name: &str) -> RiskLevel {
    match tool_name {
        "Bash" | "bash" => RiskLevel::Shell,
        "Write" | "Edit" | "write" | "edit" => RiskLevel::Write,
        "Read" | "Grep" | "Ls" | "Find" | "read" | "grep" | "ls" | "find" => RiskLevel::Read,
        "subagent" => RiskLevel::Unknown,
        _ => RiskLevel::Unknown,
    }
}

/// 精细风险推断：基于工具名称和参数内容。
/// 用于审批 UI 展示更精确的风险标签。
#[doc(hidden)]
pub fn infer_risk_level(tool_name: &str, args: &Value) -> RiskLevel {
    let base = classify_risk(tool_name);

    if (tool_name == "Bash" || tool_name == "bash")
        && let Some(cmd) = args.get("command").and_then(|v| v.as_str())
    {
        // Git commands
        if cmd.trim_start().starts_with("git ") {
            return RiskLevel::Git;
        }
        // Network commands
        let network_prefixes = [
            "curl ",
            "wget ",
            "npm install",
            "npm publish",
            "pnpm ",
            "yarn ",
            "bun install",
            "pip install",
            "cargo install",
        ];
        for prefix in network_prefixes {
            if cmd.contains(prefix) {
                return RiskLevel::Network;
            }
        }
    }

    // File tools: check if path is a secret file
    if (tool_name == "Read"
        || tool_name == "Write"
        || tool_name == "Edit"
        || tool_name == "read"
        || tool_name == "write"
        || tool_name == "edit")
        && let Some(path) = args.get("file_path").and_then(|v| v.as_str())
        && is_secret_path(path)
    {
        return RiskLevel::Destructive;
    }

    base
}

/// 检测路径是否指向敏感文件。
fn is_secret_path(path: &str) -> bool {
    let secret_patterns = [
        ".env",
        "id_rsa",
        "id_ed25519",
        ".npmrc",
        ".pypirc",
        "credentials",
        "secrets",
        ".aws/credentials",
        "token",
        ".netrc",
    ];
    let lower = path.to_lowercase();
    secret_patterns.iter().any(|pat| lower.contains(pat))
}

/// 生成 trust_key："{tool_name}:{first_arg_prefix}"。
///
/// 对于 Bash 工具取 command 的前 40 字符；
/// 对于 Read/Write/Edit 取 file_path；
/// 其他取第一个字符串值的前 40 字符。
#[doc(hidden)]
pub fn build_trust_key(tool_name: &str, args: &Value) -> String {
    let prefix = match tool_name {
        "Bash" | "bash" => args
            .get("command")
            .and_then(|v| v.as_str())
            .map(|s| truncate_str(s, 40))
            .unwrap_or_default(),
        "Read" | "Write" | "Edit" | "read" | "write" | "edit" => args
            .get("file_path")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        _ => args
            .as_object()
            .and_then(|obj| {
                obj.values()
                    .find_map(|v| v.as_str())
                    .map(|s| truncate_str(s, 40))
            })
            .unwrap_or_default(),
    };

    format!("{tool_name}:{prefix}")
}

/// 生成多级 trust keys — 从精确到宽泛。
///
/// 对于 Bash 命令，生成各级前缀（按空格截断）以及复合命令各段。
/// 对于文件工具，生成路径和目录前缀。
/// 审批时保存最精确的 key，匹配时检查所有 session approvals 是否是当前 key 的前缀。
#[doc(hidden)]
pub fn generate_trust_levels(tool_name: &str, args: &Value) -> Vec<String> {
    generate_trust_level_options(tool_name, args)
        .into_iter()
        .map(|level| level.key)
        .collect()
}

/// Generate the scopes shown by the approval UI. Compound commands expose the
/// independent scopes for their segments so trust can accumulate safely.
pub fn generate_trust_level_options(tool_name: &str, args: &Value) -> Vec<TrustLevel> {
    match tool_name {
        "Bash" | "bash" => {
            let Some(command) = args.get("command").and_then(|value| value.as_str()) else {
                return Vec::new();
            };
            let command = first_effective_line(command);
            let segments = split_shell_segments(command);
            let targets = if segments.len() > 1 {
                segments
            } else {
                vec![command]
            };
            targets
                .into_iter()
                .flat_map(|segment| command_trust_levels(tool_name, segment))
                .fold(Vec::new(), |mut levels, level| {
                    if !levels
                        .iter()
                        .any(|existing: &TrustLevel| existing.key == level.key)
                    {
                        levels.push(level);
                    }
                    levels
                })
        }
        "Read" | "Write" | "Edit" | "read" | "write" | "edit" => {
            let Some(path) = args.get("file_path").and_then(|value| value.as_str()) else {
                return Vec::new();
            };
            let mut levels = vec![TrustLevel {
                label: path.to_string(),
                key: format!("{tool_name}:{path}"),
            }];
            if let Some(dir) = std::path::Path::new(path).parent() {
                let dir = format!("{}/", dir.display());
                levels.push(TrustLevel {
                    label: format!("{dir}*"),
                    key: format!("{tool_name}:{dir}"),
                });
            }
            levels
        }
        _ => vec![TrustLevel {
            label: request_trust_key(tool_name, args),
            key: request_trust_key(tool_name, args),
        }],
    }
}

fn command_trust_levels(tool_name: &str, command: &str) -> Vec<TrustLevel> {
    let command = command.trim();
    if command.is_empty() {
        return Vec::new();
    }
    let mut levels = vec![TrustLevel {
        label: command.to_string(),
        key: format!("{tool_name}:{command}"),
    }];
    let words = command.split_whitespace().collect::<Vec<_>>();
    for count in (1..words.len()).rev() {
        let prefix = words[..count].join(" ");
        levels.push(TrustLevel {
            label: format!("{prefix} *"),
            key: format!("{tool_name}:{prefix}"),
        });
    }
    levels
}

fn first_effective_line(command: &str) -> &str {
    command
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
        .trim()
}

fn request_trust_key(tool_name: &str, args: &Value) -> String {
    if matches!(tool_name, "Bash" | "bash") {
        let command = args
            .get("command")
            .and_then(|value| value.as_str())
            .map(first_effective_line)
            .unwrap_or_default();
        return format!("{tool_name}:{command}");
    }
    build_trust_key(tool_name, args)
}

fn request_matches_session_approval(
    approvals: &HashSet<String>,
    tool_name: &str,
    args: &Value,
) -> bool {
    if matches!(tool_name, "Bash" | "bash") {
        let Some(command) = args.get("command").and_then(|value| value.as_str()) else {
            return false;
        };
        let command = first_effective_line(command);
        let segments = split_shell_segments(command);
        let targets = if segments.len() > 1 {
            segments
        } else {
            vec![command]
        };
        return targets
            .into_iter()
            .all(|segment| command_matches_session_approval(approvals, tool_name, segment));
    }

    let trust_key = request_trust_key(tool_name, args);
    approvals.iter().any(|approved| {
        approved == &trust_key
            || (approved.starts_with(&format!("{tool_name}:"))
                && approved.ends_with('/')
                && trust_key.starts_with(approved))
    })
}

fn command_matches_session_approval(
    approvals: &HashSet<String>,
    tool_name: &str,
    command: &str,
) -> bool {
    let prefix = format!("{tool_name}:");
    approvals.iter().any(|approved| {
        let Some(approved_command) = approved.strip_prefix(&prefix) else {
            return false;
        };
        command == approved_command
            || command
                .strip_prefix(approved_command)
                .and_then(|suffix| suffix.chars().next())
                .is_some_and(char::is_whitespace)
    })
}

fn patterns_cover_request(patterns: &[Regex], tool_name: &str, args: &Value) -> bool {
    if !matches!(tool_name, "Bash" | "bash") {
        return patterns
            .iter()
            .any(|pattern| pattern.is_match(&request_trust_key(tool_name, args)));
    }
    let Some(command) = args.get("command").and_then(|value| value.as_str()) else {
        return false;
    };
    let command = first_effective_line(command);
    let segments = split_shell_segments(command);
    let targets = if segments.len() > 1 {
        segments
    } else {
        vec![command]
    };
    targets.into_iter().all(|segment| {
        let key = format!("{tool_name}:{}", segment.trim());
        patterns.iter().any(|pattern| pattern.is_match(&key))
    })
}

fn untrusted_trust_levels(
    approvals: &HashSet<String>,
    tool_name: &str,
    args: &Value,
) -> Vec<TrustLevel> {
    if !matches!(tool_name, "Bash" | "bash") {
        return generate_trust_level_options(tool_name, args);
    }
    let Some(command) = args.get("command").and_then(|value| value.as_str()) else {
        return Vec::new();
    };
    let command = first_effective_line(command);
    let segments = split_shell_segments(command);
    let targets = if segments.len() > 1 {
        segments
    } else {
        vec![command]
    };
    targets
        .into_iter()
        .filter(|segment| !command_matches_session_approval(approvals, tool_name, segment))
        .flat_map(|segment| command_trust_levels(tool_name, segment))
        .fold(Vec::new(), |mut levels, level| {
            if !levels
                .iter()
                .any(|existing: &TrustLevel| existing.key == level.key)
            {
                levels.push(level);
            }
            levels
        })
}

fn approval_info(tool_name: &str, args: &Value, trust_levels: Vec<TrustLevel>) -> ApprovalInfo {
    let trust_levels = if trust_levels.is_empty() {
        generate_trust_level_options(tool_name, args)
    } else {
        trust_levels
    };
    let trust_key = trust_levels
        .first()
        .map(|level| level.key.clone())
        .unwrap_or_else(|| request_trust_key(tool_name, args));
    ApprovalInfo {
        tool_name: tool_name.to_string(),
        args_summary: summarize_args(tool_name, args),
        risk: infer_risk_level(tool_name, args),
        trust_key,
        trust_levels,
    }
}

fn rules_match_any(rules: &[String], tool_name: &str, args: &Value, workspace_root: &Path) -> bool {
    request_targets(tool_name, args).iter().any(|target| {
        rules
            .iter()
            .any(|rule| rule_matches(rule, tool_name, target, workspace_root))
    })
}

fn rules_cover_request(
    rules: &[String],
    tool_name: &str,
    args: &Value,
    workspace_root: &Path,
) -> bool {
    let targets = request_targets(tool_name, args);
    !targets.is_empty()
        && targets.iter().all(|target| {
            rules
                .iter()
                .any(|rule| rule_matches(rule, tool_name, target, workspace_root))
        })
}

fn request_targets(tool_name: &str, args: &Value) -> Vec<String> {
    if matches!(tool_name, "Bash" | "bash") {
        let Some(command) = args.get("command").and_then(|value| value.as_str()) else {
            return Vec::new();
        };
        return split_shell_segments(first_effective_line(command))
            .into_iter()
            .map(str::to_string)
            .collect();
    }
    args.get("file_path")
        .and_then(|value| value.as_str())
        .map(|path| vec![path.to_string()])
        .unwrap_or_default()
}

fn rule_matches(rule: &str, tool_name: &str, target: &str, workspace_root: &Path) -> bool {
    let Some((tool, pattern)) = rule.split_once('(') else {
        return false;
    };
    let Some(pattern) = pattern.strip_suffix(')') else {
        return false;
    };
    if tool != "*" && !tool.eq_ignore_ascii_case(tool_name) {
        return false;
    }
    if matches!(tool_name, "Bash" | "bash") {
        let prefix = pattern.trim_end().strip_suffix('*').map(str::trim_end);
        return prefix.map_or_else(
            || target.trim() == pattern.trim(),
            |prefix| {
                target.trim() == prefix
                    || target
                        .trim()
                        .strip_prefix(prefix)
                        .and_then(|suffix| suffix.chars().next())
                        .is_some_and(char::is_whitespace)
            },
        );
    }

    let target = normalize_rule_path(target, workspace_root);
    let wildcard = pattern.trim_end().ends_with('*');
    let pattern = pattern.trim_end().trim_end_matches('*').trim_end();
    let pattern = normalize_rule_path(pattern, workspace_root);
    target == pattern || ((wildcard || pattern.ends_with('/')) && target.starts_with(&pattern))
}

fn normalize_rule_path(path: &str, workspace_root: &Path) -> String {
    let path = Path::new(path);
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        workspace_root.join(path)
    };
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            component => normalized.push(component.as_os_str()),
        }
    }
    let mut text = normalized.to_string_lossy().replace('\\', "/");
    if normalized.is_dir() || text.ends_with('/') {
        text = format!("{}/", text.trim_end_matches('/'));
    }
    text
}

fn trust_key_to_project_rule(trust_key: &str) -> Result<String, String> {
    let (tool, target) = trust_key
        .split_once(':')
        .ok_or_else(|| format!("Invalid trust key: {trust_key}"))?;
    if target.is_empty() {
        return Err("Cannot persist an empty trust scope".to_string());
    }
    let suffix = if matches!(tool, "Bash" | "bash") {
        " *"
    } else {
        "*"
    };
    Ok(format!("{tool}({target}{suffix})"))
}

/// 生成参数摘要用于审批 UI 展示。
fn summarize_args(tool_name: &str, args: &Value) -> String {
    match tool_name {
        "Bash" | "bash" => args
            .get("command")
            .and_then(|v| v.as_str())
            .map(|s| truncate_str(s, 80))
            .unwrap_or_else(|| "(no command)".to_string()),
        "Read" | "read" => args
            .get("file_path")
            .and_then(|v| v.as_str())
            .unwrap_or("(unknown)")
            .to_string(),
        "Write" | "Edit" | "write" | "edit" => args
            .get("file_path")
            .and_then(|v| v.as_str())
            .unwrap_or("(unknown)")
            .to_string(),
        _ => {
            let s = serde_json::to_string(args).unwrap_or_default();
            truncate_str(&s, 120)
        }
    }
}

/// 截断字符串到指定字节长度（在 char 边界截断）。
fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        let mut end = max_len;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}...", &s[..end])
    }
}
