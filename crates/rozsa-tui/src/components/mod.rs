// components/ — UI 组件集合
//
// Internal Framework:
// components/
// ├── editor.rs ............. 编辑器插件接口
// ├── sidebar.rs ............ 侧边栏 (git/model/tokens/agents/files)
// ├── model_selector.rs ..... 模型选择器
// ├── session_selector.rs ... 会话选择器
// ├── session_search.rs ..... 会话搜索
// ├── session_tree.rs ....... 会话树结构
// ├── permission.rs ......... 权限审批面板
// ├── autocomplete.rs ....... 自动补全面板
// ├── autocomplete_provider.rs Provider 架构
// └── graph.rs .............. 会话历史图

pub mod editor;
pub mod sidebar;
pub mod model_selector;
pub mod session_selector;
pub mod session_search;
pub mod session_tree;
pub mod permission;
pub mod autocomplete;
#[allow(dead_code)]
pub mod autocomplete_provider;
pub mod graph;
