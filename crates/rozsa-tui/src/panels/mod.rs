// panels/ — UI 面板集合
//
// Internal Framework:
// panels/
// ├── sidebar.rs ............ 侧边栏 (git/model/tokens/agents/files)
// ├── model_selector.rs ..... 模型选择器
// ├── session_selector.rs ... 会话选择器
// ├── permission.rs ......... 权限审批面板
// ├── autocomplete.rs ....... 自动补全面板
// └── graph.rs .............. 会话历史图

pub mod sidebar;
pub mod model_selector;
pub mod session_selector;
pub mod permission;
pub mod autocomplete;
pub mod graph;
