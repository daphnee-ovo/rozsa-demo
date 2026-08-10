// FrameworkTree
// scope.rs
// ├── enum AllowedTools
// ├── struct SubagentScope
// ├── impl SubagentScope
// ├── inherit()
// ├── readonly()
// ├── scoped()
// ├── custom()
// ├── check_tool_allowed()
// └── resolve_path()

// File: subagent/scope.rs
//
// 子 agent 工具访问范围控制 — 控制 subagent 可使用的工具集与路径/命令/skill 限制。
//
// Internal Framework:
// scope.rs
// ├── AllowedTools
// │   ├── All
// │   └── Only(HashSet<String>)
// ├── SubagentScope
// │   ├── inherit()        # 继承全部权限
// │   ├── readonly()       # 只读工具
// │   ├── scoped(paths)    # 限定到指定路径
// │   ├── custom(...)      # 完全自定义
// │   └── check_tool_allowed(tool, args, cwd) -> Result<(), String>
//
// Related Tests:
// - tests/unit/app/subagent_scope.rs

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde_json::Value;

#[derive(Clone)]
pub enum AllowedTools {
    All,
    Only(HashSet<String>),
}

#[derive(Clone)]
pub struct SubagentScope {
    pub allowed_tools: AllowedTools,
    pub allowed_paths: Option<Vec<PathBuf>>,
    pub bash_prefixes: Option<Vec<String>>,
    pub allowed_skills: Option<Vec<String>>,
}

const FILE_TOOLS: &[&str] = &["read", "write", "edit"];
const PATH_KEYS: &[&str] = &["path", "file_path", "filePath", "dir", "directory"];

impl SubagentScope {
    pub fn inherit() -> Self {
        Self {
            allowed_tools: AllowedTools::All,
            allowed_paths: None,
            bash_prefixes: None,
            allowed_skills: None,
        }
    }

    pub fn readonly() -> Self {
        let tools: HashSet<String> = ["read", "bash"].into_iter().map(String::from).collect();
        Self {
            allowed_tools: AllowedTools::Only(tools),
            allowed_paths: None,
            bash_prefixes: Some(
                crate::permissions::SAFE_SHELL_COMMANDS
                    .iter()
                    .flat_map(|prefix| [prefix.to_string(), format!("{prefix} ")])
                    .collect(),
            ),
            allowed_skills: None,
        }
    }

    pub fn scoped(paths: Vec<PathBuf>) -> Self {
        let tools: HashSet<String> = ["read", "write", "edit"]
            .into_iter()
            .map(String::from)
            .collect();
        Self {
            allowed_tools: AllowedTools::Only(tools),
            allowed_paths: Some(paths),
            bash_prefixes: None,
            allowed_skills: None,
        }
    }

    pub fn custom(
        tools: AllowedTools,
        paths: Option<Vec<PathBuf>>,
        bash_prefixes: Option<Vec<String>>,
        skills: Option<Vec<String>>,
    ) -> Self {
        Self {
            allowed_tools: tools,
            allowed_paths: paths,
            bash_prefixes,
            allowed_skills: skills,
        }
    }

    pub fn check_tool_allowed(
        &self,
        tool_name: &str,
        args: &Value,
        cwd: &Path,
    ) -> Result<(), String> {
        // 1. 工具名白名单
        if let AllowedTools::Only(allowed) = &self.allowed_tools {
            if !allowed.contains(tool_name) {
                return Err(format!("tool '{}' is not in the allowed set", tool_name));
            }
        }

        // 2. 文件类工具 — 检查路径
        if FILE_TOOLS.contains(&tool_name) {
            if let Some(allowed_paths) = &self.allowed_paths {
                let path_str = PATH_KEYS
                    .iter()
                    .find_map(|k| args.get(*k).and_then(|v| v.as_str()));
                if let Some(p) = path_str {
                    let resolved = resolve_path(cwd, Path::new(p));
                    let ok = allowed_paths.iter().any(|allowed| {
                        let allowed_abs = resolve_path(cwd, allowed);
                        resolved.starts_with(&allowed_abs)
                    });
                    if !ok {
                        return Err(format!(
                            "path '{}' is outside the allowed scope",
                            resolved.display()
                        ));
                    }
                }
            }
        }

        // 3. bash — 检查命令前缀
        if tool_name == "bash" {
            if let Some(prefixes) = &self.bash_prefixes {
                let cmd = args.get("command").and_then(|v| v.as_str()).unwrap_or("");
                let ok = prefixes.iter().any(|p| cmd.starts_with(p));
                if !ok {
                    return Err(format!(
                        "bash command '{}' does not match any allowed prefix",
                        cmd
                    ));
                }
            }
        }

        // 4. skill — 检查 skill 名单
        if tool_name == "skill" {
            if let Some(allowed_skills) = &self.allowed_skills {
                let skill = args.get("skill").and_then(|v| v.as_str()).unwrap_or("");
                if !allowed_skills.iter().any(|s| s == skill) {
                    return Err(format!("skill '{}' is not in the allowed set", skill));
                }
            }
        }

        Ok(())
    }
}

fn resolve_path(cwd: &Path, p: &Path) -> PathBuf {
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        cwd.join(p)
    }
}
