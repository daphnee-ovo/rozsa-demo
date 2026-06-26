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
// └── PermissionPolicy         # pure logic: blacklist + pattern + session memory
//
// Related Docs:
// - [Gap Audit](../../../docs/NATIVE_TUI_GAP_AUDIT.md)
// - [Settings Schema](./settings/schema.rs)

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use dashmap::DashMap;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::oneshot;

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
    /// 未知工具或已知高风险操作。
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
// PermissionPolicy
// ---------------------------------------------------------------------------

/// 纯逻辑权限策略：黑名单 + 自动审批模式 + 会话记忆。
///
/// 不涉及 I/O，所有判断基于内存状态。Send + Sync（使用 std::sync::Mutex）。
pub struct PermissionPolicy {
    mode: PermissionMode,
    blacklist: Vec<(Regex, &'static str)>,
    auto_approve_patterns: Vec<Regex>,
    session_approvals: Mutex<HashSet<String>>,
}

impl PermissionPolicy {
    /// 创建新的权限策略实例。
    ///
    /// `auto_approve_patterns` 中的字符串会编译为正则，匹配 trust_key。
    pub fn new(mode: PermissionMode, auto_approve_patterns: Vec<String>) -> Self {
        // 硬编码黑名单：匹配 Bash 工具的 command 参数。
        let blacklist: Vec<(Regex, &'static str)> = vec![
            (
                Regex::new(r"rm\s+-rf\s+/").unwrap(),
                "rm -rf on root is always blocked",
            ),
            (
                Regex::new(r"\bsudo\b").unwrap(),
                "sudo requires manual execution",
            ),
            (
                Regex::new(r"git\s+reset\s+--hard").unwrap(),
                "destructive: git reset --hard",
            ),
            (
                Regex::new(r"git\s+push\s+--force").unwrap(),
                "destructive: force push",
            ),
            (
                Regex::new(r"\bmkfs\b").unwrap(),
                "destructive: filesystem format",
            ),
        ];

        let auto_approve_patterns = auto_approve_patterns
            .iter()
            .filter_map(|p| Regex::new(p).ok())
            .collect();

        Self {
            mode,
            blacklist,
            auto_approve_patterns,
            session_approvals: Mutex::new(HashSet::new()),
        }
    }

    /// 评估一次工具调用是否允许执行。
    pub fn evaluate(&self, tool_name: &str, args: &Value) -> PolicyVerdict {
        // FreePermission 模式：一律放行。
        if self.mode == PermissionMode::FreePermission {
            return PolicyVerdict::Allow;
        }

        // 黑名单检查（仅对 Bash 工具的 command 参数）。
        if tool_name == "Bash" {
            if let Some(cmd) = args.get("command").and_then(|v| v.as_str()) {
                for (pattern, reason) in &self.blacklist {
                    if pattern.is_match(cmd) {
                        return PolicyVerdict::Block {
                            reason: reason.to_string(),
                        };
                    }
                }
            }
        }

        // 生成 trust_key。
        let trust_key = build_trust_key(tool_name, args);

        // 检查会话审批记忆。
        {
            let approvals = self.session_approvals.lock().unwrap();
            if approvals.contains(&trust_key) {
                return PolicyVerdict::Allow;
            }
        }

        // AutoApprove 模式：匹配 auto_approve_patterns 则放行。
        if self.mode == PermissionMode::AutoApprove {
            for pattern in &self.auto_approve_patterns {
                if pattern.is_match(&trust_key) {
                    return PolicyVerdict::Allow;
                }
            }
        }

        // 其余情况需要审批。
        let risk = classify_risk(tool_name);
        let args_summary = summarize_args(tool_name, args);

        PolicyVerdict::NeedApproval {
            info: ApprovalInfo {
                tool_name: tool_name.to_string(),
                args_summary,
                risk,
                trust_key,
            },
        }
    }

    /// 记录会话级审批：后续相同 trust_key 自动放行。
    pub fn record_session_approval(&self, trust_key: String) {
        let mut approvals = self.session_approvals.lock().unwrap();
        approvals.insert(trust_key);
    }

    /// 获取当前权限模式。
    pub fn mode(&self) -> PermissionMode {
        self.mode
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// 根据工具名称判定风险等级。
#[doc(hidden)]
pub fn classify_risk(tool_name: &str) -> RiskLevel {
    match tool_name {
        "Bash" => RiskLevel::Shell,
        "Write" | "Edit" => RiskLevel::Write,
        "Read" | "Grep" | "Ls" | "Find" => RiskLevel::Read,
        _ => RiskLevel::Destructive,
    }
}

/// 生成 trust_key："{tool_name}:{first_arg_prefix}"。
///
/// 对于 Bash 工具取 command 的前 40 字符；
/// 对于 Read/Write/Edit 取 file_path；
/// 其他取第一个字符串值的前 40 字符。
#[doc(hidden)]
pub fn build_trust_key(tool_name: &str, args: &Value) -> String {
    let prefix = match tool_name {
        "Bash" => args
            .get("command")
            .and_then(|v| v.as_str())
            .map(|s| truncate_str(s, 40))
            .unwrap_or_default(),
        "Read" | "Write" | "Edit" => args
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

/// 生成参数摘要用于审批 UI 展示。
fn summarize_args(tool_name: &str, args: &Value) -> String {
    match tool_name {
        "Bash" => args
            .get("command")
            .and_then(|v| v.as_str())
            .map(|s| truncate_str(s, 80))
            .unwrap_or_else(|| "(no command)".to_string()),
        "Read" => args
            .get("file_path")
            .and_then(|v| v.as_str())
            .unwrap_or("(unknown)")
            .to_string(),
        "Write" | "Edit" => args
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

