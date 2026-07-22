---
source: other
nums: 1
---

- [x] ISSUE-I006：规范模型 provider 展示名称
  - severity: P1
  - location：crates/rozsa-gui/src/commands.rs:1497
  - description：GUI 使用 Provider Debug 格式暴露 Custom 包装；应独立显示 CodexOauth、普通自定义名称，以及冲突时的 Custom:名称，同时保持请求路由标识不变。
  - reproduce：登录 codex-oauth 或配置 custom-name provider，模型列表显示带 Custom 包装的 Debug 文本。
  - fix：新增独立 Provider::display_name 展示语义：codex-oauth 显示 CodexOauth，普通自定义 provider 显示原名，与内置标识或展示名冲突时显示 Custom:原名；GUI 不再使用 Debug 格式，路由标识保持不变，并新增回归测试。
  - files_modify: [crates/rozsa-model/src/types.rs, crates/rozsa-gui/src/commands.rs]
  - files_create: [crates/rozsa-model/tests/provider_display_name.rs, crates/rozsa-gui/tests/model_list_presentation_test.rs]
