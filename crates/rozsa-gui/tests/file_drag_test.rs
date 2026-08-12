#[test]
fn product_composer_converts_native_file_drop_paths_to_at_references() {
    let js = include_str!("../frontend/app.js");
    let css = include_str!("../frontend/styles/components/forms.css");

    for event_name in [
        "tauri://drag-enter",
        "tauri://drag-over",
        "tauri://drag-drop",
        "tauri://drag-leave",
    ] {
        assert!(
            js.contains(event_name),
            "missing native drag event: {event_name}"
        );
    }
    assert!(js.contains("payload?.paths"));
    assert!(js.contains("paths.map(formatFileReference).join('')"));
    assert!(js.contains("insertFileReferences(paths)"));
    assert!(js.contains("const separator = beforeSelection.length > 0"));
    assert!(js.contains("insertFileReferences([path])"));
    assert!(js.contains("input.addEventListener('dragover'"));
    assert!(js.contains("input.addEventListener('drop'"));
    assert!(js.contains("await configureNativeFileDrag()"));
    assert!(css.contains(".input-wrapper.file-drop-active"));
}
