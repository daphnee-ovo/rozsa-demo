use std::{fs, path::Path};

#[test]
fn tauri_embeds_a_complete_crate_owned_frontend() {
    let gui = Path::new(env!("CARGO_MANIFEST_DIR"));
    let config: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(gui.join("tauri.conf.json")).unwrap()).unwrap();
    let frontend = gui.join(config["build"]["frontendDist"].as_str().unwrap());
    let frontend = frontend
        .canonicalize()
        .expect("Tauri frontend must exist before any Node build");
    assert!(
        frontend.starts_with(gui),
        "Tauri assets must belong to the GUI crate: {}",
        frontend.display()
    );
    for entry in [
        "index.html",
        "sidebar.html",
        "app.js",
        "sidebar.js",
        "gui_shared.js",
        "runtime/entries/main.js",
        "runtime/entries/sidebar.js",
        "ui/scripts/core/ports.js",
        "ui/templates/features/sidebar.html",
        "ui/templates/composites/notification.html",
    ] {
        assert!(
            frontend.join(entry).is_file(),
            "missing embedded resource: {entry}"
        );
    }
    let main = fs::read_to_string(frontend.join("index.html")).unwrap();
    let sidebar = fs::read_to_string(frontend.join("sidebar.html")).unwrap();
    assert!(main.contains("runtime/entries/main.js"));
    assert!(sidebar.contains("runtime/entries/sidebar.js"));
}
