use std::fs;
use std::path::{Path, PathBuf};

/// Skill 来源层级
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillScope {
    /// <cwd>/.rozsa/skills/
    Project,
    /// ~/.agents/skills/
    Agents,
    /// ~/.rozsa/agent/skills/
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
            let (value, was_quoted) =
                if let Some(inner) = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')) {
                    (inner, true)
                } else if let Some(inner) =
                    value.strip_prefix('\'').and_then(|v| v.strip_suffix('\''))
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_skill(dir: &Path, name: &str, content: &str) {
        let skill_dir = dir.join(name);
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), content).unwrap();
    }

    #[test]
    fn load_valid_skill() {
        let tmp = TempDir::new().unwrap();
        create_skill(
            tmp.path(),
            "deploy",
            "---\nname: deploy\ndescription: Deploy to production\n---\n\n# Deploy\n\nInstructions.",
        );

        let result = load_skills_from_dirs(&[(tmp.path().to_path_buf(), SkillScope::Project)]);
        assert_eq!(result.skills.len(), 1);
        assert!(result.diagnostics.is_empty());

        let skill = &result.skills[0];
        assert_eq!(skill.name, "deploy");
        assert_eq!(skill.description, "Deploy to production");
        assert_eq!(skill.scope, SkillScope::Project);
    }

    #[test]
    fn name_defaults_to_dir_name() {
        let tmp = TempDir::new().unwrap();
        create_skill(
            tmp.path(),
            "my-tool",
            "---\ndescription: A useful tool\n---\n\nContent.",
        );

        let result = load_skills_from_dirs(&[(tmp.path().to_path_buf(), SkillScope::User)]);
        assert_eq!(result.skills.len(), 1);
        assert_eq!(result.skills[0].name, "my-tool");
    }

    #[test]
    fn missing_description_produces_diagnostic() {
        let tmp = TempDir::new().unwrap();
        create_skill(tmp.path(), "bad-skill", "---\nname: bad\n---\n\nNo desc.");

        let result = load_skills_from_dirs(&[(tmp.path().to_path_buf(), SkillScope::Project)]);
        assert!(result.skills.is_empty());
        assert_eq!(result.diagnostics.len(), 1);
        assert!(result.diagnostics[0].message.contains("description"));
    }

    #[test]
    fn empty_description_produces_diagnostic() {
        let tmp = TempDir::new().unwrap();
        create_skill(
            tmp.path(),
            "empty-desc",
            "---\nname: empty\ndescription: \"\"\n---\n\nBody.",
        );

        let result = load_skills_from_dirs(&[(tmp.path().to_path_buf(), SkillScope::Project)]);
        assert!(result.skills.is_empty());
        assert_eq!(result.diagnostics.len(), 1);
        assert!(result.diagnostics[0].message.contains("empty"));
    }

    #[test]
    fn no_frontmatter_produces_diagnostic() {
        let tmp = TempDir::new().unwrap();
        create_skill(tmp.path(), "no-fm", "# Just markdown\n\nNo frontmatter.");

        let result = load_skills_from_dirs(&[(tmp.path().to_path_buf(), SkillScope::Project)]);
        assert!(result.skills.is_empty());
        assert_eq!(result.diagnostics.len(), 1);
        assert!(result.diagnostics[0].message.contains("frontmatter"));
    }

    #[test]
    fn empty_directory_returns_empty() {
        let tmp = TempDir::new().unwrap();
        let result = load_skills_from_dirs(&[(tmp.path().to_path_buf(), SkillScope::Project)]);
        assert!(result.skills.is_empty());
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn nonexistent_directory_returns_empty() {
        let result =
            load_skills_from_dirs(&[(PathBuf::from("/nonexistent/path"), SkillScope::Project)]);
        assert!(result.skills.is_empty());
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn recursive_scan_finds_nested_skills() {
        let tmp = TempDir::new().unwrap();
        let nested = tmp.path().join("category");
        fs::create_dir_all(&nested).unwrap();
        create_skill(
            &nested,
            "nested-skill",
            "---\ndescription: A nested skill\n---\n\nNested.",
        );

        let result = load_skills_from_dirs(&[(tmp.path().to_path_buf(), SkillScope::Agents)]);
        assert_eq!(result.skills.len(), 1);
        assert_eq!(result.skills[0].name, "nested-skill");
        assert_eq!(result.skills[0].scope, SkillScope::Agents);
    }

    #[test]
    fn multiline_description() {
        let tmp = TempDir::new().unwrap();
        create_skill(
            tmp.path(),
            "multi",
            "---\nname: multi\ndescription: |\n  This is a multiline\n  description here.\n---\n\nBody.",
        );

        let result = load_skills_from_dirs(&[(tmp.path().to_path_buf(), SkillScope::Project)]);
        assert_eq!(result.skills.len(), 1);
        assert_eq!(
            result.skills[0].description,
            "This is a multiline description here."
        );
    }

    #[test]
    fn strip_frontmatter_returns_body() {
        let content = "---\nname: test\ndescription: desc\n---\n\n# Body\n\nContent here.";
        let body = strip_frontmatter(content);
        assert_eq!(body, "\n# Body\n\nContent here.");
    }

    #[test]
    fn strip_frontmatter_no_frontmatter_returns_all() {
        let content = "# Just markdown\n\nNo frontmatter.";
        let body = strip_frontmatter(content);
        assert_eq!(body, content);
    }

    #[test]
    fn quoted_values_stripped() {
        let tmp = TempDir::new().unwrap();
        create_skill(
            tmp.path(),
            "quoted",
            "---\nname: \"my-skill\"\ndescription: 'A quoted desc'\n---\n\nBody.",
        );

        let result = load_skills_from_dirs(&[(tmp.path().to_path_buf(), SkillScope::Project)]);
        assert_eq!(result.skills.len(), 1);
        assert_eq!(result.skills[0].name, "my-skill");
        assert_eq!(result.skills[0].description, "A quoted desc");
    }
}
