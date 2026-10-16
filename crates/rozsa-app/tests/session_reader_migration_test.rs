//! Rust-owned reference fixtures; the Go candidate maintains its own equivalent cases.
use std::fs;
use std::path::PathBuf;

use rozsa_app::session::manager::SessionManager;
use serde_json::Value;

#[test]
fn shared_reader_fixtures_match_current_session_manager() {
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/session-reader-cases.json")).unwrap();
    let base =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tmp/session-reader-rust-tests");
    fs::create_dir_all(&base).unwrap();
    let directory = tempfile::tempdir_in(base).unwrap().keep();
    for (index, case) in cases.iter().enumerate() {
        let name = case["name"].as_str().unwrap();
        let mut text = String::new();
        for line in case["lines"].as_array().unwrap() {
            text.push_str(
                &line
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| line.to_string()),
            );
            text.push('\n');
        }
        let path = directory.join(format!("case-{index}.jsonl"));
        fs::write(&path, &text).unwrap();
        let before = fs::metadata(&path).unwrap().modified().unwrap();
        let result = SessionManager::open(&path);
        if let Some(prefix) = case["errorPrefix"].as_str() {
            let error = result
                .err()
                .unwrap_or_else(|| panic!("{name}: expected error"));
            assert!(error.to_string().starts_with(prefix), "{name}: {error}");
        } else {
            let manager = result.unwrap_or_else(|error| panic!("{name}: {error}"));
            assert_eq!(
                serde_json::to_value(manager.leaf_id()).unwrap(),
                case["leaf"],
                "{name}"
            );
            let history: Vec<_> = manager
                .entries()
                .iter()
                .map(|entry| entry.id().to_owned())
                .collect();
            assert_eq!(
                serde_json::to_value(history).unwrap(),
                case["history"],
                "{name}"
            );
            // Normalize only numeric spelling of the declared f64 cost fields;
            // retain every other expected field, value and message position.
            let mut expected = case["messages"].clone();
            for message in expected.as_array_mut().unwrap() {
                if let Some(cost) = message
                    .pointer_mut("/usage/cost")
                    .and_then(Value::as_object_mut)
                {
                    for value in cost.values_mut() {
                        *value = Value::from(value.as_f64().unwrap());
                    }
                }
            }
            assert_eq!(
                serde_json::to_value(manager.context_messages()).unwrap(),
                expected,
                "{name}"
            );
        }
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            text,
            "{name}: file changed"
        );
        assert_eq!(
            fs::metadata(&path).unwrap().modified().unwrap(),
            before,
            "{name}: mtime changed"
        );
    }
}
