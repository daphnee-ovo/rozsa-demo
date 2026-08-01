use std::fs;

use rozsa_model::credentials::{
    ensure_private_env_value_at, resolve_config_value, resolve_config_value_from_env_file,
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
