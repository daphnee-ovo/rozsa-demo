---
source: other
nums: 1
---

- [x] ISSUE-I003：fake-anthropic-server 无条件返回 tool_use 导致 agent 死循环
  - severity: P1
  - location：devtools/fake-anthropic-server.py:37
  - description：fake server 在有 tools 的请求中无条件返回 tool_use block，agent 执行 tool 失败后重新请求，server 又返回相同 tool_use，形成无限循环。应改为默认只返回文本，不主动调用 tool。
  - reproduce：启动 fake server + run.sh，发送任意消息，观察到重复 "Hello from fake Anthropic server!" + tool validation error 循环刷屏
  - fix：
