use std::fs;

use rozsa_model::credentials::{
    ensure_private_env_value_at, resolve_config_value, resolve_config_value_from_env_file,
    resolve_environment_variable_from_shell_file,
};

#[test]
fn rejects_shell_command_values_without_running_them() {
    let error = resolve_config_value("!printf command-should-not-run").unwrap_err();

    assert!(error.contains("Shell command credential references are disabled"));
}

#[test]
fn only_dollar_prefixed_values_read_the_process_environment() {
    let name = "ROZSA_TEST_CONFIG_ENV_REFERENCE_7F2A";
    let value = "private-test-value";
    unsafe {
        std::env::set_var(name, value);
    }

    assert_eq!(resolve_config_value(name).unwrap(), name);
    assert_eq!(resolve_config_value(&format!("${name}")).unwrap(), value);

    unsafe {
        std::env::remove_var(name);
    }
}

#[test]
fn resolves_private_env_file_without_exporting_it() {
    let directory = tempfile::tempdir().unwrap();
    let env_path = directory.path().join(".env");
    fs::write(&env_path, "ROZSA_TEST_PRIVATE_ENV_3C91=from-private-file\n").unwrap();

    let value =
        resolve_config_value_from_env_file("$ROZSA_TEST_PRIVATE_ENV_3C91", &env_path).unwrap();

    assert_eq!(value, "from-private-file");
    assert!(std::env::var("ROZSA_TEST_PRIVATE_ENV_3C91").is_err());
}

#[test]
fn private_env_writer_is_idempotent_and_keeps_values_quoted() {
    let directory = tempfile::tempdir().unwrap();
    let env_path = directory.path().join(".env");

    ensure_private_env_value_at(
        &env_path,
        "ROZSA_TEST_WRITTEN_ENV_51D8",
        "value with spaces",
    )
    .unwrap();
    let first = fs::read_to_string(&env_path).unwrap();
    ensure_private_env_value_at(
        &env_path,
        "ROZSA_TEST_WRITTEN_ENV_51D8",
        "value with spaces",
    )
    .unwrap();
    let second = fs::read_to_string(&env_path).unwrap();

    assert_eq!(first, second);
    assert_eq!(first.matches("ROZSA_TEST_WRITTEN_ENV_51D8=").count(), 1);
    assert!(first.contains("\"value with spaces\""));
}

#[test]
fn resolves_literal_shell_assignment_without_executing_shell_code() {
    let directory = tempfile::tempdir().unwrap();
    let shell_file = directory.path().join(".zshrc");
    let name = "ROZSA_TEST_SHELL_ENV_4A71";
    fs::write(
        &shell_file,
        format!(
            "# API key\nexport {name}=\"from-shell\"\nexport IGNORED=$(printf should-not-run)\n"
        ),
    )
    .unwrap();

    let value = resolve_environment_variable_from_shell_file(name, &shell_file).unwrap();
    assert_eq!(value.as_deref(), Some("from-shell"));
    assert!(std::env::var(name).is_err());
    assert!(
        resolve_environment_variable_from_shell_file("IGNORED", &shell_file)
            .unwrap()
            .is_none()
    );
}

#[test]
fn resolves_fish_static_assignments_without_execution() {
    let directory = tempfile::tempdir().unwrap();
    let fish_directory = directory.path().join(".config/fish");
    fs::create_dir_all(&fish_directory).unwrap();
    let fish_file = fish_directory.join("config.fish");
    let fish_name = "ROZSA_TEST_FISH_ENV_4A71";
    fs::write(
        &fish_file,
        format!(
            "set -gx {fish_name} \"from-fish\"\nset -gx IGNORED_FISH (printf should-not-run)\n"
        ),
    )
    .unwrap();

    assert_eq!(
        resolve_environment_variable_from_shell_file(fish_name, &fish_file)
            .unwrap()
            .as_deref(),
        Some("from-fish")
    );
    assert!(
        resolve_environment_variable_from_shell_file("IGNORED_FISH", &fish_file)
            .unwrap()
            .is_none()
    );
}
