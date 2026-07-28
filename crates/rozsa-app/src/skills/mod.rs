// FrameworkTree
// mod.rs
// ├── mod loader
// ├── struct Skill
// ├── impl Skill
// ├── from()
// ├── struct SkillRegistry
// ├── impl SkillRegistry
// ├── new()
// ├── empty()
// ├── load_from_defaults()
// ├── load_from_defaults_with_diagnostics()
// ├── load_from_roots()
// ├── load_from_roots_with_settings()
// ├── load_from_roots_with_settings_and_diagnostics()
// ├── layered_dirs()
// ├── find_by_name()
// ├── list()
// ├── is_empty()
// ├── format_for_prompt()
// ├── slash_command_names()
// ├── filter_enabled()
// ├── scope_priority()
// └── format_skill_var_path()

pub mod loader;

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use crate::config_paths::ConfigRoots;
use loader::{LoadedSkill, SkillScope, load_skills_from_dirs};

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
        let roots = ConfigRoots::discover(cwd)
            .expect("Rózsa config roots must be available before loading skills");
        let result = load_skills_from_dirs(&Self::layered_dirs(&roots));
        Self::new(result.skills)
    }

    /// 从默认目录加载，同时返回诊断信息
    pub fn load_from_defaults_with_diagnostics(cwd: &Path) -> (Self, Vec<loader::SkillDiagnostic>) {
        let roots = ConfigRoots::discover(cwd)
            .expect("Rózsa config roots must be available before loading skills");
        let result = load_skills_from_dirs(&Self::layered_dirs(&roots));
        (Self::new(result.skills), result.diagnostics)
    }

    pub fn load_from_roots(roots: &ConfigRoots) -> Self {
        let result = load_skills_from_dirs(&Self::layered_dirs(roots));
        Self::new(result.skills)
    }

    pub fn load_from_roots_with_settings(
        roots: &ConfigRoots,
        settings: &BTreeMap<String, bool>,
    ) -> Self {
        let result = load_skills_from_dirs(&Self::layered_dirs(roots));
        Self::new(filter_enabled(result.skills, settings))
    }

    pub fn load_from_roots_with_settings_and_diagnostics(
        roots: &ConfigRoots,
        settings: &BTreeMap<String, bool>,
    ) -> (Self, Vec<loader::SkillDiagnostic>) {
        let result = load_skills_from_dirs(&Self::layered_dirs(roots));
        (
            Self::new(filter_enabled(result.skills, settings)),
            result.diagnostics,
        )
    }

    pub fn layered_dirs(roots: &ConfigRoots) -> Vec<(PathBuf, SkillScope)> {
        let [global, project] = roots.skill_dirs();
        vec![(global, SkillScope::User), (project, SkillScope::Project)]
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
            lines.push(format!(
                "- {}: {} (file: {})",
                skill.name, skill.description, var_path
            ));
        }

        lines.join("\n")
    }

    /// 返回所有 skill 名称（用于 autocomplete）
    pub fn slash_command_names(&self) -> Vec<String> {
        self.skills.iter().map(|s| s.name.clone()).collect()
    }
}

fn filter_enabled(skills: Vec<LoadedSkill>, settings: &BTreeMap<String, bool>) -> Vec<LoadedSkill> {
    skills
        .into_iter()
        .filter(|skill| settings.get(&skill.name).copied().unwrap_or(true))
        .collect()
}

fn scope_priority(scope: SkillScope) -> u8 {
    match scope {
        SkillScope::Project => 2,
        SkillScope::User => 0,
    }
}

fn format_skill_var_path(skill: &Skill) -> String {
    let var_name = match skill.scope {
        SkillScope::Project => "$PROJECT_SKILLS",
        SkillScope::User => "$USER_SKILLS",
    };

    // 从 file_path 提取 skill 目录后的相对路径部分
    // file_path 形如 /path/to/skills/<name>/SKILL.md
    // 我们要输出 $VAR/<name>/SKILL.md
    let file_name = skill
        .file_path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    let skill_dir_name = skill
        .base_dir
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    format!("{var_name}/{skill_dir_name}/{file_name}")
}

/// 向后兼容：旧代码中的 SkillMatcher 引用
pub type SkillMatcher = SkillRegistry;
