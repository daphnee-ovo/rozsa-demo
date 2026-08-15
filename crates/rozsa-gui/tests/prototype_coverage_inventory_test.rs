// FrameworkTree
// prototype_coverage_inventory_test.rs
// ├── manifest_evidence_exists_in_real_source_files()
// ├── prototype_scene_registry_matches_disk_and_each_scene_is_runnable()
// ├── prototype_stylesheet_registry_is_bidirectional_and_preserves_source_order()
// ├── prototype_html_uses_component_library_without_compatibility_entry()
// ├── visible_event_registry_is_derived_from_runtime_source()
// ├── gap_links_and_chinese_documents_match_machine_manifest()
// ├── manifest()
// ├── surface_statuses()
// ├── evidence_surfaces()
// ├── string()
// ├── strings()
// ├── optional_strings()
// ├── extract_quoted_arguments()
// ├── scene_stylesheet_href()
// ├── stylesheet_imports()
// ├── repository_root()
// ├── files_with_extension()
// ├── files_with_extension_recursive()
// └── visit()

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

const MANIFEST: &str = include_str!("../../../docs/gui/NEW_VERSION_MIGRATION_COVERAGE.json");
const SOURCE_ORDER: &str = include_str!("../../../docs/gui/new-version/styles/source-order.json");
const COVERAGE: &str = include_str!("../../../docs/gui/NEW_VERSION_MIGRATION_COVERAGE.md");
const GAPS: &str = include_str!("../../../docs/gui/NEW_VERSION_PROTOTYPE_GAPS.md");

#[test]
fn manifest_evidence_exists_in_real_source_files() {
    let manifest = manifest();
    let statuses = surface_statuses(&manifest);

    for registry in ["source_evidence", "prototype_evidence", "blocking_evidence"] {
        for evidence in manifest[registry].as_array().unwrap() {
            let surface = string(evidence, "surface");
            assert!(
                statuses.contains_key(surface),
                "{registry} 引用了未知表面 {surface}"
            );

            let path = repository_root().join(string(evidence, "file"));
            let source = fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("无法读取 {}: {error}", path.display()));
            for token in strings(evidence, "all_of") {
                assert!(
                    source.contains(token),
                    "{surface} 的真实证据已失效：{} 不包含 {token:?}",
                    path.display()
                );
            }
            for token in optional_strings(evidence, "none_of") {
                assert!(
                    !source.contains(token),
                    "{surface} 的阻断证据已过期：{} 已出现 {token:?}，必须重新审查覆盖状态",
                    path.display()
                );
            }
        }
    }

    let runtime_surfaces = evidence_surfaces(&manifest, "source_evidence");
    let prototype_surfaces = evidence_surfaces(&manifest, "prototype_evidence");
    let blocking_surfaces = evidence_surfaces(&manifest, "blocking_evidence");
    for (surface, status) in statuses {
        assert!(
            runtime_surfaces.contains(&surface),
            "{surface} 没有 runtime 源码证据"
        );
        match status.as_str() {
            "covered" => assert!(
                prototype_surfaces.contains(&surface),
                "{surface} 被标为 covered，却没有可执行原型或场景 DOM 证据"
            ),
            "missing" => assert!(
                blocking_surfaces.contains(&surface),
                "{surface} 被标为 missing，却没有来自原型源码的阻断证据"
            ),
            "partial" | "non-visual" => {}
            other => panic!("{surface} 使用了未知状态 {other}"),
        }
    }
}

#[test]
fn prototype_scene_registry_matches_disk_and_each_scene_is_runnable() {
    let manifest = manifest();
    let expected: BTreeSet<_> = manifest["prototype_scenes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap().to_owned())
        .collect();
    let scene_dir = repository_root().join("docs/gui/new-version/scenes");
    let actual: BTreeSet<_> = files_with_extension(&scene_dir, "html")
        .into_iter()
        .map(|name| name.trim_end_matches(".html").to_owned())
        .collect();

    assert_eq!(
        actual, expected,
        "原型场景增删后必须重新分类，不能静默漏过 inventory"
    );
    for scene in actual {
        let html = fs::read_to_string(scene_dir.join(format!("{scene}.html"))).unwrap();
        assert!(
            html.contains(&format!("data-rozsa-scene=\"{scene}\"")),
            "{scene}.html 的 scene identity 与文件名不一致"
        );
        assert!(
            html.contains("href=\"../styles/main.css\""),
            "{scene}.html 未直接加载原型 styles 组件库"
        );
        assert!(
            html.contains("src=\"../rozsa-gui.js\""),
            "{scene}.html 未加载原版 JS"
        );
        let expected_override = manifest["prototype_styles"]["scene_overrides"]
            .get(&scene)
            .and_then(Value::as_str)
            .map(scene_stylesheet_href);
        for (registered_scene, path) in manifest["prototype_styles"]["scene_overrides"]
            .as_object()
            .unwrap()
        {
            let href = scene_stylesheet_href(path.as_str().unwrap());
            assert_eq!(
                html.contains(&format!("href=\"{href}\"")),
                expected_override.as_deref() == Some(href.as_str()),
                "{scene}.html 的场景覆盖 CSS 与清单不一致（登记场景：{registered_scene}）"
            );
        }
        for root in ["id=\"mainContentScene\"", "id=\"settingsPanel\""] {
            assert!(
                html.contains(root),
                "{scene}.html 缺少稳定 scene root {root}"
            );
        }
    }
}

#[test]
fn prototype_stylesheet_registry_is_bidirectional_and_preserves_source_order() {
    let manifest = manifest();
    let styles = &manifest["prototype_styles"];
    let source_order: Value =
        serde_json::from_str(SOURCE_ORDER).expect("CSS 来源顺序清单必须是合法 JSON");
    let root = repository_root();
    let styles_dir = root.join("docs/gui/new-version/styles");

    let mut registered = BTreeSet::new();
    for key in ["main_entry", "sidebar_entry"] {
        registered.insert(string(styles, key).to_owned());
    }
    for key in ["main_imports", "sidebar_imports"] {
        registered.extend(strings(styles, key).map(str::to_owned));
    }
    for key in ["scene_overrides", "standalone_styles"] {
        registered.extend(
            styles[key]
                .as_object()
                .unwrap()
                .values()
                .map(|path| path.as_str().unwrap().to_owned()),
        );
    }

    let actual: BTreeSet<_> = files_with_extension_recursive(&styles_dir, "css")
        .into_iter()
        .map(|path| format!("docs/gui/new-version/styles/{path}"))
        .collect();
    assert_eq!(
        actual, registered,
        "styles/ 中每个 CSS 都必须在机器清单中有明确入口或归属，清单也不能引用不存在的 CSS"
    );

    let main_imports: Vec<_> = strings(styles, "main_imports").map(str::to_owned).collect();
    let source_blocks: Vec<_> = source_order["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|block| string(block, "target").to_owned())
        .collect();
    assert_eq!(
        main_imports, source_blocks,
        "main.css 的组件顺序必须保持原单体 CSS 的连续块顺序"
    );
    assert_eq!(
        stylesheet_imports(string(styles, "main_entry")),
        main_imports,
        "main.css 的实际 import 与机器清单不一致"
    );
    assert_eq!(
        stylesheet_imports(string(styles, "sidebar_entry")),
        strings(styles, "sidebar_imports")
            .map(str::to_owned)
            .collect::<Vec<_>>(),
        "sidebar.css 的实际 import 与机器清单不一致"
    );

    let extracted: BTreeSet<_> = source_order["extracted_inline_styles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| string(item, "target"))
        .collect();
    let declared_extracted: BTreeSet<_> = ["scene_overrides", "standalone_styles"]
        .into_iter()
        .flat_map(|key| styles[key].as_object().unwrap().values())
        .map(|path| path.as_str().unwrap())
        .collect();
    assert_eq!(
        extracted, declared_extracted,
        "inline style 的拆分来源、HTML 归属与机器清单必须闭合"
    );

    assert!(root.join(string(styles, "readme")).is_file());
    assert!(
        !root.join("docs/gui/new-version/rozsa-gui.css").exists(),
        "不得恢复已删除的 rozsa-gui.css 兼容入口"
    );
}

#[test]
fn prototype_html_uses_component_library_without_compatibility_entry() {
    let manifest = manifest();
    let root = repository_root();
    let prototype_dir = root.join("docs/gui/new-version");
    let all_html = files_with_extension_recursive(&prototype_dir, "html");
    for relative_path in &all_html {
        let html = fs::read_to_string(prototype_dir.join(relative_path)).unwrap();
        assert!(
            !html.contains("href=\"rozsa-gui.css\"")
                && !html.contains("href=\"../rozsa-gui.css\""),
            "{relative_path} 仍通过 stylesheet href 引用已删除的兼容入口"
        );
    }

    let root_html = fs::read_to_string(prototype_dir.join("rozsa-gui.html")).unwrap();
    assert!(
        root_html.contains("href=\"styles/main.css\""),
        "根原型必须直接加载 styles/main.css"
    );
    for (html_path, css_path) in manifest["prototype_styles"]["standalone_styles"]
        .as_object()
        .unwrap()
    {
        let html = fs::read_to_string(root.join(html_path)).unwrap();
        let href = css_path
            .as_str()
            .unwrap()
            .strip_prefix("docs/gui/new-version/")
            .unwrap();
        assert!(
            html.contains(&format!("href=\"{href}\"")),
            "{html_path} 未直接加载登记的独立样式 {href}"
        );
    }
}

#[test]
fn visible_event_registry_is_derived_from_runtime_source() {
    let manifest = manifest();
    for (relative_path, expected) in manifest["events"].as_object().unwrap() {
        let source = fs::read_to_string(repository_root().join(relative_path)).unwrap();
        let listener = if relative_path.ends_with("sidebar.js") {
            "sidebarListen('"
        } else {
            "listen('"
        };
        let actual = extract_quoted_arguments(&source, listener);
        let expected: BTreeSet<_> = expected
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap().to_owned())
            .collect();
        assert_eq!(
            actual, expected,
            "{relative_path} 的可见事件发生变化，必须重新判断 ownership 与覆盖状态"
        );
    }
}

#[test]
fn gap_links_and_chinese_documents_match_machine_manifest() {
    let manifest = manifest();
    let statuses = surface_statuses(&manifest);
    let mut required_gaps = BTreeSet::new();

    for surface in manifest["surfaces"].as_array().unwrap() {
        let id = string(surface, "id");
        assert!(COVERAGE.contains(id), "中文覆盖文档缺少 {id}");
        if let Some(gap) = surface.get("gap").and_then(Value::as_str) {
            required_gaps.insert(gap.to_owned());
            assert!(
                GAPS.contains(&format!("## {gap}")),
                "中文缺口文档缺少 {gap}"
            );
        }
    }

    assert_eq!(statuses.len(), 35, "表面增删必须显式更新机器清单");
    assert_eq!(required_gaps.len(), 13, "缺口增删必须显式更新机器清单");
    for forbidden_heading in [
        "# New-version",
        "## Status contract",
        "## Coverage matrix",
        "# New-version prototype gaps",
        "**Runtime entry:**",
        "**Prototype required:**",
    ] {
        assert!(
            !COVERAGE.contains(forbidden_heading) && !GAPS.contains(forbidden_heading),
            "文档仍保留未翻译标题或字段：{forbidden_heading}"
        );
    }
}

fn manifest() -> Value {
    serde_json::from_str(MANIFEST).expect("迁移覆盖 JSON 必须是合法 JSON")
}

fn surface_statuses(manifest: &Value) -> BTreeMap<String, String> {
    manifest["surfaces"]
        .as_array()
        .unwrap()
        .iter()
        .map(|surface| {
            (
                string(surface, "id").to_owned(),
                string(surface, "status").to_owned(),
            )
        })
        .collect()
}

fn evidence_surfaces(manifest: &Value, registry: &str) -> BTreeSet<String> {
    manifest[registry]
        .as_array()
        .unwrap()
        .iter()
        .map(|evidence| string(evidence, "surface").to_owned())
        .collect()
}

fn string<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key]
        .as_str()
        .unwrap_or_else(|| panic!("字段 {key} 必须是字符串"))
}

fn strings<'a>(value: &'a Value, key: &str) -> impl Iterator<Item = &'a str> {
    value[key]
        .as_array()
        .unwrap_or_else(|| panic!("字段 {key} 必须是数组"))
        .iter()
        .map(|item| item.as_str().unwrap())
}

fn optional_strings<'a>(value: &'a Value, key: &str) -> impl Iterator<Item = &'a str> {
    value
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|item| item.as_str().unwrap())
}

fn extract_quoted_arguments(source: &str, prefix: &str) -> BTreeSet<String> {
    source
        .split(prefix)
        .skip(1)
        .filter_map(|tail| tail.split_once('\'').map(|(value, _)| value.to_owned()))
        .collect()
}

fn scene_stylesheet_href(path: &str) -> String {
    format!(
        "../{}",
        path.strip_prefix("docs/gui/new-version/")
            .expect("场景 CSS 必须位于新版原型目录")
    )
}

fn stylesheet_imports(entry: &str) -> Vec<String> {
    let source = fs::read_to_string(repository_root().join(entry)).unwrap();
    let entry_parent = Path::new(entry).parent().unwrap();
    source
        .lines()
        .filter_map(|line| {
            line.strip_prefix("@import url(\"")
                .and_then(|tail| tail.strip_suffix("\");"))
        })
        .map(|import| {
            let relative = import.strip_prefix("./").unwrap_or(import);
            entry_parent.join(relative).to_string_lossy().into_owned()
        })
        .collect()
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap()
        .to_path_buf()
}

fn files_with_extension(directory: &Path, extension: &str) -> BTreeSet<String> {
    fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some(extension))
        .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
        .collect()
}

fn files_with_extension_recursive(directory: &Path, extension: &str) -> BTreeSet<String> {
    fn visit(root: &Path, directory: &Path, extension: &str, files: &mut BTreeSet<String>) {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(root, &path, extension, files);
            } else if path.extension().and_then(|value| value.to_str()) == Some(extension) {
                files.insert(
                    path.strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                );
            }
        }
    }

    let mut files = BTreeSet::new();
    visit(directory, directory, extension, &mut files);
    files
}