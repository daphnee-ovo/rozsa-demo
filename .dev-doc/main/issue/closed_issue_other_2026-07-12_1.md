---
source: other
nums: 1
---

- [x] ISSUE-I001：GUI IME composition 仍被任意按键打断
  - severity: P0
  - location：crates/rozsa-gui/frontend/app.js
  - description：中文输入法 composition 期间，任意后续按键都会打断预编辑状态，无法连续输入 yi 等拼音；上一版仅增加 composition guard，实际 GUI 事件链仍存在中断源。
  - reproduce：在 GUI 输入框中切换中文输入法，输入 y 后再输入 i；预期 composition 预览保持 yi，实际按 i 后预览被重置或只剩 i。
  - fix：在 compositionstart 时使未完成的 autocomplete 请求失效；输入事件识别 event.isComposing 和 insertCompositionText；异步 autocomplete 返回及 highlight DOM 回写期间再次阻断 composition，compositionend 后延迟刷新补全。
  - files_modify: [crates/rozsa-gui/frontend/app.js, crates/rozsa-gui/frontend/index.html]
  - files_create: [crates/rozsa-gui/tests/ime_runtime_test.rs]
