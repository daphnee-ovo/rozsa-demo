use rozsa_app::skills::loader::{SkillScope, load_skills_from_dirs, strip_frontmatter};
use std::fs;
use std::path::{Path, PathBuf};
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
