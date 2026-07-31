// FrameworkTree
// mod.rs
// ├── mod dashboard
// └── mod discovery

//! Read-only dev-flow integration boundary.

pub mod dashboard;
pub mod discovery;

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
