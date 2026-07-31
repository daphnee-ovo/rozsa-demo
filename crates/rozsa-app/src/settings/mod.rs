// FrameworkTree
// mod.rs
// ├── mod merge
// ├── mod schema
// └── mod storage

pub mod merge;
pub mod schema;
pub mod storage;

pub use schema::{
    AppearanceSettings, CompactionSettings, DevFlowSettings, PartialSettings, PermissionSettings,
    RetrySettings, Settings,
};
pub use storage::{CapabilityKind, PermissionRuleKind, SettingsManager, SettingsScope};
