---
source: audit
nums: 1
---

- [x] ISSUE-I015：原型关闭 interruption 后 Composer 仍被 scene CSS 隐藏
  - severity: P1
  - location：crates/rozsa-gui/frontend/ui/styles/composites/permission-question.css:28
  - description：T010实际原型 ask-user-question 点击Other、填写并Done后，questionPanel已关闭，但Composer computed display仍为none且activeElement不是msgInput。共享CSS按data-rozsa-scene永久隐藏Composer，与已有interruption关闭恢复输入及focus契约冲突。真实GUI同流程恢复正常。
  - reproduce：打开prototype/index.html?scene=ask-user-question，选择Other，填写答案并点击Done；读回Composer display与focus。permission-request也被同一无条件scene selector影响。
  - fix：移除按 prototype scene 永久隐藏 Composer 的两条规则，仅在实际可见 interruption 面板存在时隐藏。实际 CSS DOM 回归修复前失败、修复后4 tests通过；真实原型提交后 Composer display=block、questionPanel display=none、focus=msgInput。
  - files_modify: [crates/rozsa-gui/frontend/ui/styles/composites/permission-question.css, crates/rozsa-gui/js-tests/shared-interruption.test.js]
  - files_create: []
