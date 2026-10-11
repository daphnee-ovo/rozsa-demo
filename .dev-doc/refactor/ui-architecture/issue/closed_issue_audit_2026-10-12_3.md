---
source: audit
nums: 1
---

- [x] ISSUE-I016：恢复固定 interruption 演示场景的 baseline 输入显隐
  - severity: P1
  - location：crates/rozsa-gui/frontend/ui/styles/composites/permission-question.css:28
  - description：I015移除scene selector误改了T034明确验证的固定原型演示行为：提交后普通Composer继续隐藏且焦点回BODY。恢复演示scene约束；真实GUI没有data-rozsa-scene，仍仅按面板visible隐藏并恢复输入。
  - reproduce：ef21cc5下原型ask-user-question提交后Composer显示并获得焦点，与T034记录的baseline行为不一致。
  - fix：恢复两项固定演示scene选择器；4项Node测试通过，回归断言在恢复前失败、恢复后通过。
  - files_modify: [crates/rozsa-gui/frontend/ui/styles/composites/permission-question.css, crates/rozsa-gui/js-tests/shared-interruption.test.js]
  - files_create: []
