use rozsa_app::resources::ResourceLoader;
use tempfile::TempDir;

#[tokio::test]
async fn project_config_context_overrides_global_context() {
    let temp = TempDir::new().unwrap();
    let cwd = temp.path().join("workspace");
    let global = temp.path().join("global");
    let project = cwd.join(".rozsa");
    tokio::fs::create_dir_all(&global).await.unwrap();
    tokio::fs::create_dir_all(&project).await.unwrap();
    tokio::fs::write(global.join("AGENTS.md"), "global config")
        .await
        .unwrap();
    tokio::fs::write(project.join("AGENTS.md"), "project config")
        .await
        .unwrap();

    let resources = ResourceLoader::new(cwd, vec![global, project])
        .load()
        .await
        .unwrap();

    assert!(
        resources
            .resources
            .iter()
            .any(|resource| resource.content == "project config")
    );
    assert!(
        !resources
            .resources
            .iter()
            .any(|resource| resource.content == "global config")
    );
}

#[tokio::test]
async fn global_config_context_is_used_when_project_context_is_missing() {
    let temp = TempDir::new().unwrap();
    let cwd = temp.path().join("workspace");
    let global = temp.path().join("global");
    tokio::fs::create_dir_all(&global).await.unwrap();
    tokio::fs::create_dir_all(&cwd).await.unwrap();
    tokio::fs::write(global.join("AGENTS.md"), "global config")
        .await
        .unwrap();

    let resources = ResourceLoader::new(cwd.clone(), vec![global, cwd.join(".rozsa")])
        .load()
        .await
        .unwrap();

    assert!(
        resources
            .resources
            .iter()
            .any(|resource| resource.content == "global config")
    );
}
