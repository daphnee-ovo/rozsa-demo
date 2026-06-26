// Integration tests for Skill Registry system
// Verifies SPEC acceptance criteria across multiple components

#[cfg(test)]
mod tests {
    use crate::agent_session::{AgentSession, AgentSessionConfig};
    use crate::session::manager::SessionManager;
    use crate::settings::SettingsManager;
    use crate::resources::LoadedResources;
    use rozsa_model::types::ThinkingLevel;
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// Helper: Create a minimal AgentSession for testing
    fn create_test_session(cwd: PathBuf) -> AgentSession {
        let session_dir = cwd.join("sessions");
        std::fs::create_dir_all(&session_dir).unwrap();
        let session_file = session_dir.join("test_session.jsonl");
        let session_manager = SessionManager::create(
            &session_file,
            "test-session".to_string(),
            cwd.to_string_lossy().to_string(),
            None,
        ).unwrap();

        let settings_path = cwd.join("settings.json");
        std::fs::write(&settings_path, "{}").unwrap();
        let settings_manager = SettingsManager::load(
            settings_path,
            None,
            None,
        ).unwrap();

        let resources = LoadedResources::default();

        let model = rozsa_model::types::Model {
            id: "test-model".to_string(),
            name: "Test Model".to_string(),
            api: rozsa_model::types::Api::AnthropicMessages,
            provider: rozsa_model::types::Provider::Anthropic,
            base_url: "https://api.anthropic.com".to_string(),
            reasoning: false,
            input_modalities: vec![rozsa_model::types::InputModality::Text],
            cost: rozsa_model::types::ModelCost {
                input: 0.0,
                output: 0.0,
                cache_read: 0.0,
                cache_write: 0.0,
            },
            context_window: 100000,
            max_tokens: 4096,
            thinking_level_map: None,
            headers: None,
            compat: None,
        };

        let config = AgentSessionConfig {
            model,
            thinking_level: ThinkingLevel::Off,
            system_prompt: "Test system prompt.".to_string(),
            cwd,
            session_manager,
            settings_manager,
            resources,
            pre_tool_use: None,
        };

        AgentSession::new(config)
    }

    #[test]
    fn test_skills_loaded_at_startup() {
        let tmp = TempDir::new().unwrap();
        let cwd = tmp.path().to_path_buf();

        let project_skills = cwd.join(".rozsa").join("skills").join("deploy");
        std::fs::create_dir_all(&project_skills).unwrap();
        std::fs::write(
            project_skills.join("SKILL.md"),
            "---\nname: deploy\ndescription: Deploy the application\n---\n\n# Deploy\n\nInstructions.",
        ).unwrap();

        let session = create_test_session(cwd);
        let registry = session.skill_registry();

        assert_eq!(registry.list().len(), 1);
        let skill = registry.find_by_name("deploy").unwrap();
        assert_eq!(skill.name, "deploy");
        assert_eq!(skill.description, "Deploy the application");
    }

    #[test]
    fn test_system_prompt_includes_skills() {
        let tmp = TempDir::new().unwrap();
        let cwd = tmp.path().to_path_buf();

        let project_skills = cwd.join(".rozsa").join("skills").join("lint");
        std::fs::create_dir_all(&project_skills).unwrap();
        std::fs::write(
            project_skills.join("SKILL.md"),
            "---\nname: lint\ndescription: Run linter\n---\n\nLint code.",
        ).unwrap();

        let session = create_test_session(cwd);
        let registry = session.skill_registry();
        let prompt_fragment = registry.format_for_prompt();

        assert!(prompt_fragment.contains("## Skills"));
        assert!(prompt_fragment.contains("Available skills:"));
        assert!(prompt_fragment.contains("lint: Run linter"));
        assert!(prompt_fragment.contains("$PROJECT_SKILLS/lint/SKILL.md"));
    }

    #[test]
    fn test_system_prompt_empty_when_no_skills() {
        let tmp = TempDir::new().unwrap();
        let cwd = tmp.path().to_path_buf();

        let session = create_test_session(cwd);
        let registry = session.skill_registry();
        let prompt_fragment = registry.format_for_prompt();

        assert_eq!(prompt_fragment, "");
    }

    #[test]
    fn test_reload_skills() {
        let tmp = TempDir::new().unwrap();
        let cwd = tmp.path().to_path_buf();

        let skill_dir = cwd.join(".rozsa").join("skills").join("alpha");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: alpha\ndescription: First skill\n---\n\nAlpha.",
        ).unwrap();

        let session = create_test_session(cwd.clone());
        assert_eq!(session.skill_registry().list().len(), 1);

        // Add a new skill
        let beta_dir = cwd.join(".rozsa").join("skills").join("beta");
        std::fs::create_dir_all(&beta_dir).unwrap();
        std::fs::write(
            beta_dir.join("SKILL.md"),
            "---\nname: beta\ndescription: Second skill\n---\n\nBeta.",
        ).unwrap();

        let diagnostics = session.reload_skills();
        assert!(diagnostics.is_empty());

        assert_eq!(session.skill_registry().list().len(), 2);
        assert!(session.skill_registry().find_by_name("alpha").is_some());
        assert!(session.skill_registry().find_by_name("beta").is_some());
    }

    #[test]
    fn test_diagnostics_for_invalid_skills() {
        let tmp = TempDir::new().unwrap();
        let cwd = tmp.path().to_path_buf();

        let bad_skill = cwd.join(".rozsa").join("skills").join("bad");
        std::fs::create_dir_all(&bad_skill).unwrap();
        std::fs::write(
            bad_skill.join("SKILL.md"),
            "---\nname: bad\n---\n\nNo description.",
        ).unwrap();

        let session = create_test_session(cwd);
        let diagnostics = session.reload_skills();

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("description"));
    }
}
