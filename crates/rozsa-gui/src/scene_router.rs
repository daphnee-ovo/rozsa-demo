// FrameworkTree
// scene_router.rs
// ├── enum GuiScene
// ├── enum SettingsPane
// ├── enum GuiWebview
// ├── impl GuiWebview
// ├── label()
// ├── from_label()
// ├── struct GuiSceneSnapshot
// ├── struct SceneUpdate
// ├── struct ReadyUpdate
// ├── struct SceneRouter
// ├── impl SceneRouter
// ├── default()
// ├── impl SceneRouter
// ├── snapshot()
// ├── set_scene()
// └── webview_ready()

// File: scene_router.rs
//
// Window scene state machine shared by the persistent main and sidebar WebViews.
//
// Structure:
// - GuiSceneSnapshot: complete, revisioned state sent to either WebView.
// - SceneRouter::set_scene: validates expected_revision and serially commits intents.
// - SceneRouter::webview_ready: records readiness and returns the latest full snapshot.
//
// Revision rules:
// - revision starts at 1 so a newly loaded WebView with last_revision 0 receives state.
// - only a committed scene or settings-pane change increments revision.
// - a stale expected_revision never mutates state and returns the current snapshot.
// - WebViews must apply only snapshots newer than their local revision.
//
// Design: ../../../.dev-doc/main/SPEC.md#3-scene-与状态边界
// IPC contract: ../../../.dev-doc/main/SPEC.md#4-ipc-与事件路由

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

pub const GUI_SCENE_SNAPSHOT_EVENT: &str = "gui-scene-snapshot";

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum GuiScene {
    Main,
    Settings,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SettingsPane {
    Skills,
    Tools,
    Extensions,
    General,
    Appearance,
    #[serde(rename = "keyboard-shortcuts")]
    KeyboardShortcuts,
    Permissions,
    #[serde(rename = "dev-flow")]
    DevFlow,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum GuiWebview {
    Main,
    Sidebar,
}

impl GuiWebview {
    pub fn label(self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::Sidebar => "sidebar",
        }
    }

    pub fn from_label(label: &str) -> Result<Self, String> {
        match label {
            "main" => Ok(Self::Main),
            "sidebar" => Ok(Self::Sidebar),
            _ => Err(format!("Unknown GUI WebView: {label}")),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiSceneSnapshot {
    pub revision: u64,
    pub scene: GuiScene,
    pub selected_pane: Option<SettingsPane>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SceneUpdate {
    pub snapshot: GuiSceneSnapshot,
    pub changed: bool,
    pub stale: bool,
    pub ready_webviews: Vec<GuiWebview>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReadyUpdate {
    pub snapshot: GuiSceneSnapshot,
    pub should_emit: bool,
    pub all_webviews_ready: bool,
}

pub struct SceneRouter {
    snapshot: GuiSceneSnapshot,
    ready_webviews: BTreeSet<GuiWebview>,
}

impl Default for SceneRouter {
    fn default() -> Self {
        Self {
            snapshot: GuiSceneSnapshot {
                revision: 1,
                scene: GuiScene::Main,
                selected_pane: None,
            },
            ready_webviews: BTreeSet::new(),
        }
    }
}

impl SceneRouter {
    pub fn snapshot(&self) -> GuiSceneSnapshot {
        self.snapshot
    }

    pub fn set_scene(
        &mut self,
        scene: GuiScene,
        selected_pane: Option<SettingsPane>,
        expected_revision: u64,
    ) -> Result<SceneUpdate, String> {
        if expected_revision != self.snapshot.revision {
            return Ok(SceneUpdate {
                snapshot: self.snapshot,
                changed: false,
                stale: true,
                ready_webviews: Vec::new(),
            });
        }

        let selected_pane = match scene {
            GuiScene::Main => None,
            GuiScene::Settings => Some(
                selected_pane.ok_or_else(|| "Settings scene requires selectedPane".to_owned())?,
            ),
        };
        if self.snapshot.scene == scene && self.snapshot.selected_pane == selected_pane {
            return Ok(SceneUpdate {
                snapshot: self.snapshot,
                changed: false,
                stale: false,
                ready_webviews: Vec::new(),
            });
        }

        let revision = self
            .snapshot
            .revision
            .checked_add(1)
            .ok_or_else(|| "GUI scene revision overflow".to_owned())?;
        self.snapshot = GuiSceneSnapshot {
            revision,
            scene,
            selected_pane,
        };
        Ok(SceneUpdate {
            snapshot: self.snapshot,
            changed: true,
            stale: false,
            ready_webviews: self.ready_webviews.iter().copied().collect(),
        })
    }

    pub fn webview_ready(&mut self, webview: GuiWebview, last_revision: u64) -> ReadyUpdate {
        self.ready_webviews.insert(webview);
        ReadyUpdate {
            snapshot: self.snapshot,
            should_emit: last_revision < self.snapshot.revision,
            all_webviews_ready: self.ready_webviews.len() == 2,
        }
    }
}