---
source: audit
nums: 1
---

- [x] ISSUE-I017：隔离 Electron 消息事件与运行状态的会话归属
  - severity: P1
  - location：desktop/renderer/runtime/adapters/electron.ts:23
  - description：代码审查确认适配器使用全局 activeOperationId/streamText 保存运行状态，切换会话会替换 active；原操作事件仍使用当前状态接收路径，且全局 activeOperationId 阻止其他会话发送。需通过确定性回归确认并修复跨会话呈现、独立发送和取消路由，保持原 Rust 的 session/operation 身份语义。
  - reproduce：受控 provider 持续输出会话 A；切换到 B 后继续交付 A 的 delta/终态，并在 B 发送或取消。断言 A 的消息和终态只更新 A，B 可独立运行/取消，切回 A 恢复它自己的内容和进度；先证明回归在修复前失败。
  - fix：按 operationId 独立维护事件序列、流文本和终态，并在启动时绑定 SessionSnapshot path/catalogId；切换与启动经选择队列串行，发送/取消按当前 session 选择对应 operation。新增两会话后台 delta/terminal 恢复及定向取消回归。修复前新回归分别因 A delta 泄漏到 B、全局运行标志拒绝 B 发送而失败；修复后新回归和 adapter 邻近测试 4/4 通过，desktop typecheck 通过。
  - files_modify: [desktop/renderer/runtime/adapters/electron.ts]
  - files_create: [desktop/tests/session-message-routing.test.ts]
