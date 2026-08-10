//! Integration tests for Skill Registry system.
//! Tests use explicit paths to avoid HOME environment pollution.

use rozsa_app::config_paths::ConfigRoots;
use rozsa_app::skills::SkillRegistry;
use rozsa_app::skills::loader::{SkillScope, load_skills_from_dirs};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

fn create_skill(dir: &std::path::Path, name: &str, content: &str) {
    let skill_dir = dir.join(name);
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(skill_dir.join("SKILL.md"), content).unwrap();
}

#[test]
fn full_load_from_project_dir() {
    let tmp = TempDir::new().unwrap();
    let project_skills = tmp.path().join("project_skills");
    fs::create_dir_all(&project_skills).unwrap();

    create_skill(
        &project_skills,
        "deploy",
        "---\nname: deploy\ndescription: Deploy the application\n---\n\n# Deploy\n\nInstructions.",
    );

    let result = load_skills_from_dirs(&[(project_skills, SkillScope::Project)]);
    let registry = SkillRegistry::new(result.skills);

    assert_eq!(registry.list().len(), 1);
    let skill = registry.find_by_name("deploy").unwrap();
    assert_eq!(skill.description, "Deploy the application");
    assert_eq!(skill.scope, SkillScope::Project);
}

#[test]
fn priority_override_across_dirs() {
    let tmp = TempDir::new().unwrap();
    let project_dir = tmp.path().join("project");
    let user_dir = tmp.path().join("user");
    fs::create_dir_all(&project_dir).unwrap();
    fs::create_dir_all(&user_dir).unwrap();

    create_skill(
        &user_dir,
        "deploy",
        "---\nname: deploy\ndescription: User deploy\n---\nUser.",
    );
    create_skill(
        &project_dir,
        "deploy",
        "---\nname: deploy\ndescription: Project deploy\n---\nProject.",
    );

    let result = load_skills_from_dirs(&[
        (user_dir, SkillScope::User),
        (project_dir, SkillScope::Project),
    ]);
    let registry = SkillRegistry::new(result.skills);

    assert_eq!(registry.list().len(), 1);
    assert_eq!(
        registry.find_by_name("deploy").unwrap().description,
        "Project deploy"
    );
    assert_eq!(
        registry.find_by_name("deploy").unwrap().scope,
        SkillScope::Project
    );
}

#[test]
fn agents_skills_are_loaded_between_user_and_project_scopes() {
    let tmp = TempDir::new().unwrap();
    let global = tmp.path().join("global");
    let project = tmp.path().join("project");
    let home = tmp.path().join("home");

    create_skill(
        &global.join("skills"),
        "shared",
        "---\nname: shared\ndescription: Global shared\n---\nGlobal.",
    );
    create_skill(
        &home.join(".agents").join("skills"),
        "shared",
        "---\nname: shared\ndescription: Agents shared\n---\nAgents.",
    );
    create_skill(
        &home.join(".agents").join("skills"),
        "agents-only",
        "---\nname: agents-only\ndescription: Agents-only skill\n---\nAgents only.",
    );
    create_skill(
        &project.join("skills"),
        "shared",
        "---\nname: shared\ndescription: Project shared\n---\nProject.",
    );

    let roots = ConfigRoots::from_overrides(
        Path::new("/workspace"),
        Some(global),
        Some(project),
        Some(home),
    )
    .unwrap();
    let dirs = SkillRegistry::layered_dirs(&roots);
    assert_eq!(
        dirs.iter().map(|(_, scope)| *scope).collect::<Vec<_>>(),
        vec![SkillScope::User, SkillScope::Agents, SkillScope::Project]
    );

    let registry = SkillRegistry::load_from_roots(&roots);
    assert_eq!(
        registry.find_by_name("shared").unwrap().description,
        "Project shared"
    );
    assert_eq!(
        registry.find_by_name("agents-only").unwrap().scope,
        SkillScope::Agents
    );
    assert!(
        registry
            .format_for_prompt()
            .contains("$AGENTS_SKILLS/agents-only/SKILL.md")
    );
}

#[test]
fn default_config_roots_load_global_then_project_skills() {
    let tmp = TempDir::new().unwrap();
    let global = tmp.path().join("global");
    let project = tmp.path().join("project");
    create_skill(
        &global.join("skills"),
        "deploy",
        "---\nname: deploy\ndescription: Global deploy\n---\nGlobal.",
    );
    create_skill(
        &project.join("skills"),
        "deploy",
        "---\nname: deploy\ndescription: Project deploy\n---\nProject.",
    );
    let roots = ConfigRoots::from_roots(global, project);

    let registry = SkillRegistry::load_from_roots(&roots);
    let deploy = registry.find_by_name("deploy").unwrap();
    assert_eq!(deploy.description, "Project deploy");
    assert_eq!(deploy.scope, SkillScope::Project);
}

#[test]
fn format_for_prompt_with_multiple_scopes() {
    let tmp = TempDir::new().unwrap();
    let project_dir = tmp.path().join("project");
    let user_dir = tmp.path().join("user");
    fs::create_dir_all(&project_dir).unwrap();
    fs::create_dir_all(&user_dir).unwrap();

    create_skill(
        &project_dir,
        "build",
        "---\nname: build\ndescription: Build project\n---\nBuild.",
    );
    create_skill(
        &user_dir,
        "helper",
        "---\nname: helper\ndescription: My helper\n---\nHelper.",
    );

    let result = load_skills_from_dirs(&[
        (user_dir, SkillScope::User),
        (project_dir, SkillScope::Project),
    ]);
    let registry = SkillRegistry::new(result.skills);
    let prompt = registry.format_for_prompt();

    assert!(prompt.contains("## Skills"));
    assert!(prompt.contains("$PROJECT_SKILLS/build/SKILL.md"));
    assert!(prompt.contains("$USER_SKILLS/helper/SKILL.md"));
}

#[test]
fn empty_dirs_produce_no_skills() {
    let tmp = TempDir::new().unwrap();
    let empty_dir = tmp.path().join("empty");
    fs::create_dir_all(&empty_dir).unwrap();

    let result = load_skills_from_dirs(&[(empty_dir, SkillScope::Project)]);
    let registry = SkillRegistry::new(result.skills);

    assert!(registry.is_empty());
    assert_eq!(registry.format_for_prompt(), "");
}

#[test]
fn diagnostics_collected_across_dirs() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("skills");
    fs::create_dir_all(&dir).unwrap();

    create_skill(
        &dir,
        "good",
        "---\nname: good\ndescription: Works fine\n---\nGood.",
    );
    create_skill(&dir, "bad1", "---\nname: bad1\n---\nMissing desc.");
    create_skill(&dir, "bad2", "No frontmatter at all.");

    let result = load_skills_from_dirs(&[(dir, SkillScope::Project)]);

    assert_eq!(result.skills.len(), 1);
    assert_eq!(result.skills[0].name, "good");
    assert_eq!(result.diagnostics.len(), 2);
}

#[test]
fn slash_command_names_lists_all() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("skills");
    fs::create_dir_all(&dir).unwrap();

    create_skill(&dir, "alpha", "---\ndescription: Alpha skill\n---\nA.");
    create_skill(&dir, "beta", "---\ndescription: Beta skill\n---\nB.");

    let result = load_skills_from_dirs(&[(dir, SkillScope::Project)]);
    let registry = SkillRegistry::new(result.skills);
    let mut names = registry.slash_command_names();
    names.sort();

    assert_eq!(names, vec!["alpha", "beta"]);
}
