#[allow(dead_code)]
#[path = "../src/args.rs"]
mod args;

#[test]
fn existing_directory_positional_input_becomes_the_workspace() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("project");
    std::fs::create_dir(&workspace).unwrap();
    let (cwd, prompt) =
        args::resolve_positional_input(Some("project"), temp.path(), false).unwrap();

    assert_eq!(cwd, workspace.canonicalize().unwrap());
    assert_eq!(prompt, None);
}

#[test]
fn non_directory_positional_input_requires_explicit_print_mode() {
    let temp = tempfile::tempdir().unwrap();
    let error =
        args::resolve_positional_input(Some("fix the bug"), temp.path(), false).unwrap_err();

    assert!(error.to_string().contains("rozsa -p \"fix the bug\""));
}

#[test]
fn print_mode_keeps_positional_text_as_the_prompt() {
    let temp = tempfile::tempdir().unwrap();
    let (cwd, prompt) =
        args::resolve_positional_input(Some("fix the bug"), temp.path(), true).unwrap();

    assert_eq!(cwd, temp.path());
    assert_eq!(prompt.as_deref(), Some("fix the bug"));
}

#[test]
fn missing_positional_input_uses_the_startup_working_directory() {
    let temp = tempfile::tempdir().unwrap();
    let (cwd, prompt) = args::resolve_positional_input(None, temp.path(), false).unwrap();

    assert_eq!(cwd, temp.path());
    assert_eq!(prompt, None);
}
