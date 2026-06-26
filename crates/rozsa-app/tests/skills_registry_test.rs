use rozsa_app::skills::loader::{LoadedSkill, SkillScope};
use rozsa_app::skills::SkillRegistry;
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
