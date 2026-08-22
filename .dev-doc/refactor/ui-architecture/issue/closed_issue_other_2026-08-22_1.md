---
source: other
nums: 1
---

- [x] ISSUE-I001：修复 prototype shell sidebar template 的稳定 id
  - severity: P1
  - location：docs/gui/new-version/templates/features/sidebar.html:1
  - description：application shell 通过 shell.js 挂载共享 sidebar 后，legacy mountTemplates 仍按 id=sidebar 判断是否需要 fallback。当前 template 只有 data-od-id，导致 root entry 抛出 Cannot read properties of null (reading content) 并部分加载。
  - reproduce：用本地 HTTP 打开 index.html?scene=empty-session，观察 console 和页面 alert。
  - fix：为共享 sidebar template 恢复 legacy mount 所需的 id=sidebar，并让 legacy bootstrap 从 body 读取 application-shell entry marker；加入 coverage regression assertions，浏览器验证多场景 shell ready 且无 scene/shell error。
  - files_modify: [docs/gui/new-version/templates/features/sidebar.html, crates/rozsa-gui/tests/prototype_coverage_inventory_test.rs, docs/gui/new-version/rozsa-gui.js]
  - files_create: []
