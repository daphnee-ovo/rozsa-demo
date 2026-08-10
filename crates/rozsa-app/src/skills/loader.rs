// FrameworkTree
// loader.rs
// ├── enum SkillScope
// ├── struct LoadedSkill
// ├── struct SkillDiagnostic
// ├── struct SkillLoadResult
// ├── load_skills_from_dirs()
// ├── scan_skills_dir()
// ├── parse_skill_file()
// ├── extract_frontmatter()
// ├── parse_frontmatter_field()
// └── strip_frontmatter()

use std::fs;
use std::path::{Path, PathBuf};

/// Skill 来源层级
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillScope {
    /// <cwd>/.rozsa/skills/
    Project,
    /// ~/.agents/skills/
    Agents,
    /// ~/.rozsa/skills/
    User,
}

/// 加载后的 Skill 定义
#[derive(Debug, Clone)]
pub struct LoadedSkill {
    pub name: String,
    pub description: String,
    pub file_path: PathBuf,
    pub base_dir: PathBuf,
    pub scope: SkillScope,
}

/// Skill 加载诊断信息
#[derive(Debug, Clone)]
pub struct SkillDiagnostic {
    pub path: PathBuf,
    pub message: String,
}

/// Skill 加载结果
#[derive(Debug, Clone, Default)]
pub struct SkillLoadResult {
    pub skills: Vec<LoadedSkill>,
    pub diagnostics: Vec<SkillDiagnostic>,
}

/// 从多个目录加载 skills，按优先级顺序传入
pub fn load_skills_from_dirs(dirs: &[(PathBuf, SkillScope)]) -> SkillLoadResult {
    let mut result = SkillLoadResult::default();

    for (dir, scope) in dirs {
        if !dir.is_dir() {
            continue;
        }
        scan_skills_dir(dir, *scope, &mut result);
    }

    result
}

/// 递归扫描 skills 目录，识别 <skill-name>/SKILL.md 模式
fn scan_skills_dir(dir: &Path, scope: SkillScope, result: &mut SkillLoadResult) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let skill_file = path.join("SKILL.md");
        if skill_file.is_file() {
            match parse_skill_file(&skill_file, &path, scope) {
                Ok(skill) => result.skills.push(skill),
                Err(diagnostic) => result.diagnostics.push(diagnostic),
            }
        } else {
            scan_skills_dir(&path, scope, result);
        }
    }
}

/// 解析单个 SKILL.md 文件
fn parse_skill_file(
    file_path: &Path,
    skill_dir: &Path,
    scope: SkillScope,
) -> Result<LoadedSkill, SkillDiagnostic> {
    let content = fs::read_to_string(file_path).map_err(|e| SkillDiagnostic {
        path: file_path.to_path_buf(),
        message: format!("failed to read: {e}"),
    })?;

    let frontmatter = extract_frontmatter(&content).ok_or_else(|| SkillDiagnostic {
        path: file_path.to_path_buf(),
        message: "missing YAML frontmatter (no --- delimiters)".to_string(),
    })?;

    let description =
        parse_frontmatter_field(&frontmatter, "description").ok_or_else(|| SkillDiagnostic {
            path: file_path.to_path_buf(),
            message: "missing required field: description".to_string(),
        })?;

    if description.trim().is_empty() {
        return Err(SkillDiagnostic {
            path: file_path.to_path_buf(),
            message: "description is empty".to_string(),
        });
    }

    let name = parse_frontmatter_field(&frontmatter, "name").unwrap_or_else(|| {
        skill_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string()
    });

    Ok(LoadedSkill {
        name,
        description,
        file_path: file_path.to_path_buf(),
        base_dir: skill_dir.to_path_buf(),
        scope,
    })
}

/// 提取 --- 之间的 frontmatter 文本
fn extract_frontmatter(content: &str) -> Option<String> {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return None;
    }

    let after_first = &trimmed[3..];
    let after_first = after_first.strip_prefix('\n').unwrap_or(after_first);

    let end_pos = after_first.find("\n---")?;
    Some(after_first[..end_pos].to_string())
}

/// 从 frontmatter 文本中提取字段值
fn parse_frontmatter_field(frontmatter: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}:");

    for (i, line) in frontmatter.lines().enumerate() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(&prefix) {
            let value = rest.trim();
            // 带引号的值：显式空引号 "" 或 '' 视为存在但为空
            let (value, was_quoted) = if let Some(inner) =
                value.strip_prefix('"').and_then(|v| v.strip_suffix('"'))
            {
                (inner, true)
            } else if let Some(inner) = value.strip_prefix('\'').and_then(|v| v.strip_suffix('\''))
            {
                (inner, true)
            } else {
                (value, false)
            };

            if value == "|" || value == ">" {
                // 多行值：收集后续缩进行
                let lines: Vec<&str> = frontmatter.lines().collect();
                let mut multiline = String::new();
                for ml_line in &lines[i + 1..] {
                    if ml_line.starts_with(' ') || ml_line.starts_with('\t') {
                        if !multiline.is_empty() {
                            multiline.push(' ');
                        }
                        multiline.push_str(ml_line.trim());
                    } else {
                        break;
                    }
                }
                if !multiline.is_empty() {
                    return Some(multiline);
                }
            }

            if !value.is_empty() || was_quoted {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// 从文件内容中 strip frontmatter，返回 body 部分
pub fn strip_frontmatter(content: &str) -> &str {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return content;
    }

    let after_first = &trimmed[3..];
    let after_first = after_first.strip_prefix('\n').unwrap_or(after_first);

    match after_first.find("\n---") {
        Some(pos) => {
            let after_end = &after_first[pos + 4..];
            after_end.strip_prefix('\n').unwrap_or(after_end)
        }
        None => content,
    }
}
