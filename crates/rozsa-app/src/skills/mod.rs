pub mod loader;
#[cfg(test)]
mod integration_tests;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use loader::{load_skills_from_dirs, LoadedSkill, SkillScope};

/// 注册表中的 Skill
#[derive(Debug, Clone)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub file_path: PathBuf,
    pub base_dir: PathBuf,
    pub scope: SkillScope,
}

impl From<LoadedSkill> for Skill {
    fn from(loaded: LoadedSkill) -> Self {
        Self {
            name: loaded.name,
            description: loaded.description,
            file_path: loaded.file_path,
            base_dir: loaded.base_dir,
            scope: loaded.scope,
        }
    }
}

/// Skill 注册表
pub struct SkillRegistry {
    skills: Vec<Skill>,
}

impl SkillRegistry {
    /// 从加载结果构建注册表，按优先级去重（先加入的优先）
    pub fn new(loaded_skills: Vec<LoadedSkill>) -> Self {
        let mut seen: HashMap<String, usize> = HashMap::new();
        let mut skills: Vec<Skill> = Vec::new();

        for loaded in loaded_skills {
            let name = loaded.name.clone();
            if let Some(&existing_idx) = seen.get(&name) {
                // 高优先级覆盖低优先级
                if scope_priority(loaded.scope) > scope_priority(skills[existing_idx].scope) {
                    skills[existing_idx] = loaded.into();
                }
            } else {
                seen.insert(name, skills.len());
                skills.push(loaded.into());
            }
        }

        Self { skills }
    }

    /// 空注册表
    pub fn empty() -> Self {
        Self { skills: Vec::new() }
    }

    /// 从默认目录加载 skills 并构建注册表
    /// 返回 (registry, diagnostics)
    pub fn load_from_defaults(cwd: &Path) -> Self {
        let home = dirs_next::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let dirs = vec![
            (cwd.join(".rozsa").join("skills"), SkillScope::Project),
            (home.join(".agents").join("skills"), SkillScope::Agents),
            (home.join(".rozsa").join("agent").join("skills"), SkillScope::User),
        ];
        let result = load_skills_from_dirs(&dirs);
        Self::new(result.skills)
    }

    /// 从默认目录加载，同时返回诊断信息
    pub fn load_from_defaults_with_diagnostics(cwd: &Path) -> (Self, Vec<loader::SkillDiagnostic>) {
        let home = dirs_next::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let dirs = vec![
            (cwd.join(".rozsa").join("skills"), SkillScope::Project),
            (home.join(".agents").join("skills"), SkillScope::Agents),
            (home.join(".rozsa").join("agent").join("skills"), SkillScope::User),
        ];
        let result = load_skills_from_dirs(&dirs);
        (Self::new(result.skills), result.diagnostics)
    }

    pub fn find_by_name(&self, name: &str) -> Option<&Skill> {
        self.skills.iter().find(|s| s.name == name)
    }

    pub fn list(&self) -> &[Skill] {
        &self.skills
    }

    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }

    /// 生成 system prompt 中的 skill 列表片段
    pub fn format_for_prompt(&self) -> String {
        if self.skills.is_empty() {
            return String::new();
        }

        let mut lines = vec!["## Skills".to_string(), "Available skills:".to_string()];

        for skill in &self.skills {
            let var_path = format_skill_var_path(skill);
            lines.push(format!("- {}: {} (file: {})", skill.name, skill.description, var_path));
        }

        lines.join("\n")
    }

    /// 返回所有 skill 名称（用于 autocomplete）
    pub fn slash_command_names(&self) -> Vec<String> {
        self.skills.iter().map(|s| s.name.clone()).collect()
    }
}

fn scope_priority(scope: SkillScope) -> u8 {
    match scope {
        SkillScope::Project => 2,
        SkillScope::Agents => 1,
        SkillScope::User => 0,
    }
}

fn format_skill_var_path(skill: &Skill) -> String {
    let var_name = match skill.scope {
        SkillScope::Project => "$PROJECT_SKILLS",
        SkillScope::Agents => "$AGENTS_SKILLS",
        SkillScope::User => "$USER_SKILLS",
    };

    // 从 file_path 提取 skill 目录后的相对路径部分
    // file_path 形如 /path/to/skills/<name>/SKILL.md
    // 我们要输出 $VAR/<name>/SKILL.md
    let file_name = skill.file_path.file_name().unwrap_or_default().to_string_lossy();
    let skill_dir_name = skill.base_dir.file_name().unwrap_or_default().to_string_lossy();
    format!("{var_name}/{skill_dir_name}/{file_name}")
}

/// 向后兼容：旧代码中的 SkillMatcher 引用
pub type SkillMatcher = SkillRegistry;

#[cfg(test)]
mod tests {
    use super::*;
    use loader::{LoadedSkill, SkillScope};
    use std::path::PathBuf;

    fn make_skill(name: &str, scope: SkillScope) -> LoadedSkill {
        LoadedSkill {
            name: name.to_string(),
            description: format!("{name} description"),
            file_path: PathBuf::from(format!("/skills/{name}/SKILL.md")),
            base_dir: PathBuf::from(format!("/skills/{name}")),
            scope,
        }
    }

    #[test]
    fn dedup_by_priority_project_wins() {
        let skills = vec![
            make_skill("deploy", SkillScope::User),
            make_skill("deploy", SkillScope::Project),
        ];
        let registry = SkillRegistry::new(skills);
        assert_eq!(registry.list().len(), 1);
        assert_eq!(registry.list()[0].scope, SkillScope::Project);
    }

    #[test]
    fn dedup_by_priority_agents_over_user() {
        let skills = vec![
            make_skill("tool", SkillScope::User),
            make_skill("tool", SkillScope::Agents),
        ];
        let registry = SkillRegistry::new(skills);
        assert_eq!(registry.list().len(), 1);
        assert_eq!(registry.list()[0].scope, SkillScope::Agents);
    }

    #[test]
    fn no_dedup_for_different_names() {
        let skills = vec![
            make_skill("alpha", SkillScope::User),
            make_skill("beta", SkillScope::Project),
        ];
        let registry = SkillRegistry::new(skills);
        assert_eq!(registry.list().len(), 2);
    }

    #[test]
    fn find_by_name() {
        let skills = vec![make_skill("deploy", SkillScope::Project)];
        let registry = SkillRegistry::new(skills);
        assert!(registry.find_by_name("deploy").is_some());
        assert!(registry.find_by_name("nonexistent").is_none());
    }

    #[test]
    fn format_for_prompt_empty() {
        let registry = SkillRegistry::empty();
        assert_eq!(registry.format_for_prompt(), "");
    }

    #[test]
    fn format_for_prompt_includes_var_paths() {
        let skills = vec![
            make_skill("deploy", SkillScope::Project),
            make_skill("lint", SkillScope::Agents),
            make_skill("helper", SkillScope::User),
        ];
        let registry = SkillRegistry::new(skills);
        let prompt = registry.format_for_prompt();
        assert!(prompt.contains("$PROJECT_SKILLS/deploy/SKILL.md"));
        assert!(prompt.contains("$AGENTS_SKILLS/lint/SKILL.md"));
        assert!(prompt.contains("$USER_SKILLS/helper/SKILL.md"));
    }

    #[test]
    fn slash_command_names() {
        let skills = vec![
            make_skill("deploy", SkillScope::Project),
            make_skill("lint", SkillScope::User),
        ];
        let registry = SkillRegistry::new(skills);
        let names = registry.slash_command_names();
        assert_eq!(names, vec!["deploy", "lint"]);
    }
}
