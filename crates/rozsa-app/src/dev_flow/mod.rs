// FrameworkTree
// mod.rs
// ├── mod command
// ├── mod dashboard
// ├── mod discovery
// └── mod registry

//! Read-only dev-flow integration boundary.

pub mod command;
pub mod dashboard;
pub mod discovery;
pub mod registry;

pub use command::{
    BashExecutionEvidence, DEV_FLOW_PRESENTATION_CUSTOM_TYPE, DevFlowPresentationAction,
    DevFlowPresentationItem, DevFlowPresentationItemKind, DevFlowPresentationRecord,
    DevFlowRecordedRevision, DevFlowToolPresentation, rebuild_dev_flow_presentations,
    recognize_dow_bash,
};
pub use dashboard::{
    DashboardClient, DashboardProcess, DashboardTiming, DevFlowError, DevFlowEventStream,
    DevFlowIssue, DevFlowIssueStatus, DevFlowProjectStatus, DevFlowSnapshot, DevFlowTask,
    DevFlowTaskStatus, ReconnectBackoff, start_dashboard,
};
pub use discovery::{
    CommandExecutionError, CommandOutput, DiscoveredDow, DiscoveryCommandRunner,
    DiscoveryEnvironment, DowDiscoveryError, DowInstallSource, SystemCommandRunner, discover_dow,
    discover_dow_with,
};
pub use registry::{
    DashboardServiceFactory, DevFlowAvailability, DevFlowProjectKey, DevFlowRegistry,
    DevFlowRevisionKey, DevFlowServiceHandle, ProjectCommandRunner, ProjectResolutionError,
    SessionDevFlowState, ShutdownAllReport, SystemProjectCommandRunner, probe_project,
    resolve_project_with,
};
