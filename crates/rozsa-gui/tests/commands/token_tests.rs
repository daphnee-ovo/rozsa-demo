use super::*;

#[test]
fn attachment_pick_modes_accept_file_directory_and_any() {
    assert!(matches!(
        AttachmentPickMode::parse("any"),
        Ok(AttachmentPickMode::Any)
    ));
    assert!(matches!(
        AttachmentPickMode::parse("file"),
        Ok(AttachmentPickMode::File)
    ));
    assert!(matches!(
        AttachmentPickMode::parse("directory"),
        Ok(AttachmentPickMode::Directory)
    ));
    assert!(AttachmentPickMode::parse("unsupported").is_err());
}

#[test]
fn slash_tokens_highlight_anywhere_in_input() {
    let text = "prefix /tree suffix /model";
    let skill_commands = Vec::new();

    assert_eq!(
        slash_command_ranges(text, &skill_commands),
        vec![(7, 12), (20, 26)]
    );
    assert_eq!(
        first_builtin_slash_command(text),
        Some(("tree".to_string(), "suffix /model".to_string()))
    );
}

#[test]
fn skill_tokens_highlight_without_active_agent() {
    let text = "prefix /brainstorm suffix";
    let skill_commands = vec![SkillSlashCommand {
        name: "brainstorm".to_string(),
        description: "Collaborative exploration".to_string(),
    }];

    assert_eq!(slash_command_ranges(text, &skill_commands), vec![(7, 18)]);
    assert_eq!(
        first_skill_slash_command(text, &skill_commands),
        Some(("brainstorm".to_string(), "suffix".to_string()))
    );
}

#[test]
fn skill_tokens_normalize_all_matches() {
    let text = "prefix /brainstorm and /ask suffix";
    let skill_commands = vec![
        SkillSlashCommand {
            name: "brainstorm".to_string(),
            description: "Collaborative exploration".to_string(),
        },
        SkillSlashCommand {
            name: "ask".to_string(),
            description: "Ask".to_string(),
        },
    ];

    assert_eq!(
        normalize_skill_commands_in_text(text, &skill_commands),
        Some("prefix /skill:brainstorm and /skill:ask suffix".to_string())
    );
}

#[test]
fn file_reference_tokens_highlight_anywhere_in_input() {
    let cwd = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let text = "prefix @src/commands.rs suffix";

    assert_eq!(file_reference_ranges(text, cwd), vec![(7, 23)]);
}

#[test]
fn quoted_file_reference_tokens_with_spaces_highlight() {
    let temp_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tmp");
    std::fs::create_dir_all(&temp_root).expect("create project fixture directory");
    let temp_dir = tempfile::Builder::new()
        .prefix("file-drag-highlight-")
        .tempdir_in(temp_root)
        .unwrap();
    let file = temp_dir.path().join("source file.rs");
    std::fs::write(&file, "fn main() {}\n").unwrap();
    let text = format!(r#"@"{}""#, file.display());

    assert_eq!(
        file_reference_ranges(&text, temp_dir.path()),
        vec![(0, text.len())]
    );
}

#[test]
fn utf16_cursor_offsets_never_split_multibyte_characters() {
    let text = r#"@"/Users/xinyue/Downloads/希音 ai数据分析一面.txt" "#;
    let cursor = text.encode_utf16().count();

    assert_eq!(utf16_offset_to_byte_index(text, cursor), text.len());
    assert_eq!(utf16_offset_to_byte_index("😀/path", 1), 0);
    assert_eq!(utf16_offset_to_byte_index("😀/path", 2), 4);
}
