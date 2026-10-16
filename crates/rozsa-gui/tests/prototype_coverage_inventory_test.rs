// FrameworkTree
// prototype_coverage_inventory_test.rs
// ├── coverage_schema_matches_specification()
// ├── application_shell_templates_preserve_mount_contract()
// ├── prototype_inventory_is_bidirectional()
// ├── runtime_inventory_and_visible_events_are_bidirectional()
// ├── path_references_resolve_to_current_sources()
// ├── chinese_summary_and_design_gaps_match_inventory()
// ├── manifest()
// ├── items()
// ├── strings()
// ├── item_strings()
// ├── extract_listener_events()
// ├── files_with_extension()
// └── repository_root()

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

const MANIFEST: &str = include_str!("../../../prototype/docs/UI_COVERAGE.json");
const COVERAGE: &str = include_str!("../../../prototype/docs/UI_COVERAGE.md");
const GAPS: &str = include_str!("../../../prototype/docs/DESIGN_GAPS.md");

#[test]
fn coverage_schema_matches_specification() {
    let manifest = manifest();
    let expected_areas: BTreeSet<_> = [
        "application-shell-sidebar",
        "conversation-composer-queue-steering",
        "tool-call-evidence-diff-markdown",
        "permission-ask-user-question",
        "settings-appearance-permissions",
        "notification-feedback",
        "dev-flow",
        "gallery",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    assert_eq!(strings(&manifest, "areas"), expected_areas);

    let mut ids = BTreeSet::new();
    let mut actual_areas = BTreeSet::new();
    for item in items(&manifest) {
        let id = item["id"].as_str().expect("id 必须是字符串");
        assert!(ids.insert(id), "界面对照 ID 重复：{id}");
        let area = item["area"].as_str().expect("area 必须是字符串");
        assert!(expected_areas.contains(area), "{id} 使用未知区域 {area}");
        actual_areas.insert(area.to_owned());

        for field in [
            "runtime_entry",
            "prototype_entry",
            "css",
            "markup",
            "behavior",
            "used_by",
            "verification",
        ] {
            let values = item[field]
                .as_array()
                .unwrap_or_else(|| panic!("{id}.{field} 必须是数组"));
            let non_visual_surface =
                item["status"] == "non-visual" && matches!(field, "css" | "markup");
            assert!(
                non_visual_surface || !values.is_empty(),
                "{id}.{field} 不得为空"
            );
            assert!(
                values.iter().all(Value::is_string),
                "{id}.{field} 只能包含字符串"
            );
        }
        assert!(
            item["runtime_trigger"]
                .as_str()
                .is_some_and(|value| !value.is_empty()),
            "{id}.runtime_trigger 不得为空"
        );
        assert!(
            matches!(
                item["status"].as_str(),
                Some("covered" | "partial" | "missing" | "non-visual")
            ),
            "{id} 使用了未知 status"
        );
    }
    assert_eq!(actual_areas, expected_areas, "八个界面区域都必须有项目");
}

#[test]
fn application_shell_templates_preserve_mount_contract() {
    let sidebar_forwarder =
        fs::read_to_string(repository_root().join("prototype/templates/features/sidebar.html"))
            .expect("prototype sidebar template must be readable");
    assert!(
        sidebar_forwarder.contains(
            "data-ui-template-src=\"../../../desktop/renderer/templates/features/sidebar.html\""
        ),
        "prototype sidebar must load its shared source"
    );
    let sidebar = fs::read_to_string(
        repository_root().join("desktop/renderer/templates/features/sidebar.html"),
    )
    .expect("shared sidebar template must be readable");
    assert!(
        sidebar.contains("<aside id=\"sidebar\" data-od-id=\"sidebar\""),
        "shared sidebar template must preserve the legacy mount id"
    );
    let legacy = fs::read_to_string(repository_root().join("prototype/rozsa-gui.js"))
        .expect("prototype legacy bootstrap must be readable");
    assert!(
        legacy.contains(
            "const isApplicationShell = document.body?.dataset.rozsaEntry === \"application-shell\";"
        ),
        "legacy bootstrap must read the application-shell entry marker from body"
    );
    let interruption_forwarder = fs::read_to_string(
        repository_root().join("prototype/scripts/components/interruption-panel.js"),
    )
    .expect("prototype interruption component must be readable");
    assert!(
        interruption_forwarder
            .contains("../../../desktop/renderer/scripts/components/interruption-panel.js"),
        "prototype interruption must load its shared source"
    );
    let interruption = fs::read_to_string(
        repository_root().join("desktop/renderer/scripts/components/interruption-panel.js"),
    )
    .expect("shared interruption component must be readable");
    assert!(
        interruption.contains("document.querySelector('[data-od-id=\"chat-input\"]')"),
        "interruption component must bind to the template's data-od-id root"
    );
}

#[test]
fn prototype_inventory_is_bidirectional() {
    let manifest = manifest();
    let prototype = &manifest["prototype_inventory"];
    let scene_dir = repository_root().join("prototype/scenes");
    let actual_scenes: BTreeSet<_> = files_with_extension(&scene_dir, "html")
        .into_iter()
        .map(|name| name.trim_end_matches(".html").to_owned())
        .collect();
    let declared_scenes = strings(prototype, "scenes");
    assert_eq!(
        actual_scenes, declared_scenes,
        "scene inventory 必须与磁盘双向一致"
    );

    let prototype_references: BTreeSet<_> = items(&manifest)
        .flat_map(|item| item_strings(item, "prototype_entry").chain(item_strings(item, "used_by")))
        .collect();
    for scene in &declared_scenes {
        assert!(
            prototype_references.iter().any(|entry| entry == scene
                || entry == &format!("scene:{scene}")
                || entry.starts_with(&format!("scene:{scene}#"))),
            "scene {scene} 没有对应界面对照项目"
        );
    }

    let root = prototype["application_root"].as_str().unwrap();
    let gallery = prototype["gallery"].as_str().unwrap();
    let gallery_script = prototype["gallery_script"].as_str().unwrap();
    for path in [
        root,
        gallery,
        prototype["script"].as_str().unwrap(),
        gallery_script,
    ] {
        assert!(
            repository_root().join(path).is_file(),
            "原型清单路径不存在：{path}"
        );
    }
    assert!(
        prototype_references.contains(root),
        "主原型入口没有对应项目"
    );
    assert!(
        prototype_references
            .iter()
            .any(|entry| entry.starts_with(gallery)),
        "gallery 入口没有对应项目"
    );
    let prototype_behavior_references: BTreeSet<_> = items(&manifest)
        .flat_map(|item| item_strings(item, "behavior"))
        .collect();
    assert!(
        prototype_behavior_references
            .iter()
            .any(|entry| entry.starts_with(gallery_script)),
        "gallery behavior 入口没有对应项目"
    );

    let gallery_items: Vec<_> = items(&manifest)
        .filter(|item| item["area"] == "gallery")
        .collect();
    assert_eq!(
        gallery_items.len(),
        4,
        "gallery topbar 与三个 view 必须分别登记"
    );
    for item in gallery_items {
        let id = item["id"].as_str().unwrap();
        assert_eq!(
            item_strings(item, "used_by").collect::<Vec<_>>(),
            ["gallery:visual-direction"]
        );
        assert_eq!(
            item_strings(item, "markup").count(),
            1,
            "{id} 必须声明唯一 markup 责任位置"
        );
        assert_eq!(
            item_strings(item, "behavior").count(),
            1,
            "{id} 必须声明唯一 behavior 责任位置"
        );
    }
}

#[test]
fn runtime_inventory_and_visible_events_are_bidirectional() {
    let manifest = manifest();
    let runtime = &manifest["runtime_inventory"];
    let item_runtime_text = items(&manifest)
        .flat_map(|item| item_strings(item, "runtime_entry").chain(item_strings(item, "behavior")))
        .collect::<Vec<_>>()
        .join("\n");

    for field in ["entries", "scripts"] {
        for path in strings(runtime, field) {
            assert!(
                repository_root().join(&path).is_file(),
                "runtime inventory 路径不存在：{path}"
            );
            let basename = Path::new(&path).file_name().unwrap().to_string_lossy();
            assert!(
                item_runtime_text.contains(basename.as_ref()),
                "runtime 文件 {path} 没有对应项目"
            );
        }
    }

    let declared_events = runtime["visible_events"].as_object().unwrap();
    let item_events: BTreeSet<_> = items(&manifest)
        .flat_map(|item| {
            item.get("runtime_events")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .map(|event| event.as_str().unwrap().to_owned())
        .collect();
    for (path, events) in declared_events {
        let source = fs::read_to_string(repository_root().join(path)).unwrap();
        let prefix = if path.ends_with("runtime/adapters/notifications.js") {
            "port.subscribe('"
        } else if path.ends_with("sidebar.js") {
            "sidebarListen('"
        } else {
            "listen('"
        };
        let actual = extract_listener_events(&source, prefix);
        let expected: BTreeSet<_> = events
            .as_array()
            .unwrap()
            .iter()
            .map(|event| event.as_str().unwrap().to_owned())
            .collect();
        assert_eq!(actual, expected, "{path} 的 visible event inventory 已失效");
        for event in expected {
            assert!(
                item_events.contains(&event),
                "visible event {event} 没有归属到界面对照项目"
            );
        }
    }
}

#[test]
fn path_references_resolve_to_current_sources() {
    let manifest = manifest();
    for item in items(&manifest) {
        let id = item["id"].as_str().unwrap();
        for field in ["runtime_entry", "prototype_entry"] {
            for reference in item_strings(item, field) {
                let path = reference.split('#').next().unwrap();
                if path.starts_with("crates/")
                    || path.starts_with("docs/")
                    || path.starts_with("prototype/")
                {
                    assert!(
                        repository_root().join(path).is_file(),
                        "{id}.{field} 引用不存在：{path}"
                    );
                }
            }
        }
    }
}

#[test]
fn chinese_summary_and_design_gaps_match_inventory() {
    let manifest = manifest();
    let mut gap_ids = BTreeSet::new();
    for item in items(&manifest) {
        let id = item["id"].as_str().unwrap();
        assert!(COVERAGE.contains(id), "中文区域汇总缺少 {id}");
        if matches!(item["status"].as_str(), Some("partial" | "missing")) {
            gap_ids.insert(id);
            assert!(
                GAPS.contains(&format!("`coverage_id`：`{id}`")),
                "设计缺口表缺少 {id}"
            );
        }
    }

    let documented_gap_ids: BTreeSet<_> = GAPS
        .lines()
        .filter_map(|line| {
            line.strip_prefix("- `coverage_id`：`")
                .and_then(|tail| tail.strip_suffix('`'))
        })
        .collect();
    assert_eq!(
        documented_gap_ids, gap_ids,
        "DESIGN_GAPS 只能且必须收录 partial/missing 项"
    );
    for field in [
        "runtime_trigger",
        "current_runtime",
        "prototype_missing",
        "required_prototype",
        "blocked_scope",
    ] {
        let marker = format!("- `{field}`：");
        assert_eq!(
            GAPS.matches(&marker).count(),
            gap_ids.len(),
            "每个缺口都必须记录 {field}"
        );
    }
}

fn manifest() -> Value {
    serde_json::from_str(MANIFEST).expect("UI_COVERAGE.json 必须是合法 JSON")
}

fn items(manifest: &Value) -> impl Iterator<Item = &Value> {
    manifest["items"]
        .as_array()
        .expect("items 必须是数组")
        .iter()
}

fn strings(value: &Value, field: &str) -> BTreeSet<String> {
    value[field]
        .as_array()
        .unwrap_or_else(|| panic!("{field} 必须是数组"))
        .iter()
        .map(|item| item.as_str().unwrap().to_owned())
        .collect()
}

fn item_strings<'a>(item: &'a Value, field: &str) -> impl Iterator<Item = &'a str> {
    item[field]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap())
}

fn extract_listener_events(source: &str, prefix: &str) -> BTreeSet<String> {
    source
        .split(prefix)
        .skip(1)
        .filter_map(|tail| tail.split_once('\'').map(|(event, _)| event.to_owned()))
        .collect()
}

fn files_with_extension(directory: &Path, extension: &str) -> BTreeSet<String> {
    fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some(extension))
        .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
        .collect()
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap()
        .to_path_buf()
}
