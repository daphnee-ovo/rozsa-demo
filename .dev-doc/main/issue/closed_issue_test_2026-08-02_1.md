---
source: test
nums: 1
---

- [x] ISSUE-I004：Test TASK-T027 fail:running 14 tests
  - severity: P1
  - location：crates/rozsa-app/tests/model_registry.rs
  - description：running 14 tests
    test load_from_nonexistent_dir_returns_empty ... ok
    test loads_image_models_from_json ... ok
    test loads_generated_model_metadata ... ok
    test loads_generated_image_model_metadata ... ok
    test merges_discovered_nvidia_models ... ok
    test accepts_auth_header_provider_without_api_key ... ok
    test reports_image_provider_auth_from_env ... ok
    test models_json_allows_line_comments_and_trailing_commas ... ok
    test keeps_provider_api_keys_across_multiple_config_files ... ok
    test rejects_shell_command_api_key_configuration ... ok
    test merges_models_json_overrides_and_custom_models ... ok
    test tolerant_loading_warns_when_model_environment_reference_is_missing ... ok
    test migrates_plaintext_api_key_into_private_rozsa_env ... ok
    test tolerant_loading_reports_invalid_file_and_keeps_valid_models ... FAILED
    
    failures:
    
    ---- tolerant_loading_reports_invalid_file_and_keeps_valid_models stdout ----
    
    thread 'tolerant_loading_reports_invalid_file_and_keeps_valid_models' (4190448) panicked at crates/rozsa-app/tests/model_registry.rs:433:5:
    assertion failed: registry.find("valid-provider", "valid-model").is_some()
    note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
    
    
    failures:
        tolerant_loading_reports_invalid_file_and_keeps_valid_models
    
    test result: FAILED. 13 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
    Compiling rozsa-app v1.0.4 (/Users/xinyue/ballad/rozsa-demo/crates/rozsa-app)
        Finished `test` profile [unoptimized + debuginfo] target(s) in 8.45s
         Running tests/model_registry.rs (/Users/xinyue/ballad/rozsa-demo/target/debug/deps/model_registry-70df65d3af29d06f)
    error: test failed, to rerun pass `--test model_registry`
  - reproduce：cargo test --manifest-path '/Users/xinyue/ballad/rozsa-demo/crates/rozsa-app/Cargo.toml' --test 'model_registry'
    project_root: /Users/xinyue/ballad/rozsa-demo
  - fix：Updated the valid model test fixture to use an existing environment reference instead of triggering private .env migration; model registry and GUI regression tests now pass.
  - files_modify: [crates/rozsa-app/tests/model_registry.rs]
  - files_create: []
