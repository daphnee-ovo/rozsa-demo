---
source: audit
nums: 1
---

- [x] ISSUE-I002：修复 interruption panel 的事件委托根节点
  - severity: P1
  - location：docs/gui/new-version/scripts/components/interruption-panel.js:5
  - description：application shell 的 composer template 使用 data-od-id=chat-input 而不是 id=chat-input，interruption-panel 通过 api.byId("chat-input") 得到 null，导致权限面板的 allow/trust/deny/hint 等点击事件没有绑定。
  - reproduce：用本地 HTTP 打开 index.html?scene=permission-request，点击权限面板的 Deny and hints，面板不切换到提示页且没有动作。
  - fix：将 interruption-panel 的事件委托根节点改为模板实际使用的 data-od-id=chat-input，并加入 coverage regression assertion；浏览器验证权限提示页切换、取消与 Allow 关闭/焦点恢复均通过。
  - files_modify: [docs/gui/new-version/scripts/components/interruption-panel.js, crates/rozsa-gui/tests/prototype_coverage_inventory_test.rs]
  - files_create: []
