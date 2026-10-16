#[test]
fn gui_startup_refreshes_codex_models_only_after_oauth_login() {
    let lib = include_str!("../src/lib.rs");
    let commands = include_str!("../src/commands.rs");
    let state = include_str!("../src/state.rs");

    assert!(lib.contains(
        "commands::spawn_codex_oauth_model_refresh(handle.clone(), gui.inner().clone())"
    ));
    assert!(commands.contains("refresh_codex_oauth_models(&app, &state, false).await"));
    assert!(state.contains("pub model_registry: Option<Arc<RwLock<ModelRegistry>>>"));

    let refresh_start = commands
        .find("async fn refresh_codex_oauth_models")
        .expect("missing codex-oauth startup refresh");
    let refresh_end = commands[refresh_start..]
        .find("#[tauri::command]")
        .map(|offset| refresh_start + offset)
        .expect("missing command after codex-oauth startup refresh");
    let refresh = &commands[refresh_start..refresh_end];

    let account_check = refresh
        .find("credentials::read_account_id")
        .expect("refresh must require a codex-oauth account ID");
    let token_check = refresh
        .find("credentials::resolve_auth_json_api_key_pub")
        .expect("refresh must require a resolvable OAuth access token");
    let request = refresh
        .find("models_endpoint::refresh_models_if_needed")
        .expect("refresh must call the shared model endpoint client");
    assert!(account_check < token_check && token_check < request);
    assert!(refresh.contains("&account_id,\n        force,"));
}

#[test]
fn changed_catalog_atomically_replaces_registry_and_notifies_frontend() {
    let commands = include_str!("../src/commands.rs");
    let frontend = include_str!("../frontend/app.js");

    assert!(commands.contains("current_registry.all_json() != refreshed_registry.all_json()"));
    assert!(commands.contains(".write()\n        .map_err"));
    assert!(commands.contains("emit_main(app, \"models-updated\", ())"));
    assert!(frontend.contains("listen('models-updated', async () =>"));
    assert!(frontend.contains("models = await invoke('list_models')"));
    assert!(frontend.contains("renderModelSelector()"));
}

#[test]
fn successful_oauth_login_forces_refresh_before_returning() {
    let commands = include_str!("../src/commands.rs");

    let login_start = commands
        .find("pub async fn auth_login(")
        .expect("missing auth_login command");
    let login_end = commands[login_start..]
        .find("#[tauri::command]")
        .map(|offset| login_start + offset)
        .expect("missing command after auth_login");
    let login = &commands[login_start..login_end];

    assert!(login.contains("state: State<'_, GuiState>"));
    assert!(login.contains("refresh_codex_oauth_models(&app, state.inner(), true).await?"));
    assert!(
        login
            .find("store_oauth_credentials(")
            .expect("login must store credentials")
            < login
                .find("refresh_codex_oauth_models(")
                .expect("login must refresh models")
    );
}
