---
source: audit
nums: 1
---

- [x] ISSUE-I009：follow_up / steer 退化为普通 submit
  - severity: P2
  - location：NativeBackend follow_up steer methods
  - description：follow_up 和 steer 方法均直接委托到 submit，丢弃 images 参数且语义与 TS 版不同。需实现 followUp/steer 队列机制。
  - reproduce：
  - fix：
