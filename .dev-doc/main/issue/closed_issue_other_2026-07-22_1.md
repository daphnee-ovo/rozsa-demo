---
source: other
nums: 1
---

- [x] ISSUE-I002：Stop button 与 double Esc 无法终止运行中的 agent
  - severity: P0
  - location：crates/rozsa-gui/frontend/app.js
  - description：GUI agent running 时，Stop button 和连续两次 Escape 未能终止实际 model/tool loop。
  - reproduce：启动 GUI，发送产生持续运行的请求，点击 Stop 或连续按两次 Escape；agent 继续运行。
  - fix：Stop 与 double Esc 统一取消当前 interaction；core 在 model、permission hook 和 tool await 外层强制响应 cancellation；Bash 使用独立 process group 并在 future 丢弃时终止整个子进程树；同时清空 queue、steer 和 follow-up。
  - files_modify: [crates/rozsa-gui/frontend/app.js, crates/rozsa-gui/src/commands.rs, crates/rozsa-app/src/agent_session.rs, crates/rozsa-core/src/agent_loop.rs, crates/rozsa-app/tests/session_title_test.rs, crates/rozsa-gui/tests/queue_steering_ui_test.rs, crates/rozsa-gui/tests/transient_popup_dismissal_test.rs, docs/gui/TERMINOLOGY.md, crates/rozsa-core/tests/agent_loop_test.rs, crates/rozsa-cli/src/run.rs, crates/rozsa-app/src/tools/bash.rs, crates/rozsa-app/tests/permissions_test.rs]
  - files_create: [crates/rozsa-app/tests/bash_abort_test.rs]
