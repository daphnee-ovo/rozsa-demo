// FrameworkTree
// mod.rs
// ├── mod dashboard
// ├── mod discovery
// └── mod registry

//! Read-only dev-flow integration boundary.

pub mod dashboard;
pub mod discovery;
pub mod registry;

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
    SessionDevFlowState, SystemProjectCommandRunner, probe_project, resolve_project_with,
};
