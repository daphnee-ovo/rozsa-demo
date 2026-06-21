---
source: other
nums: 1
---

- [x] ISSUE-I002：smoke test — anthropic-messages custom provider JSON 端到端验证
  - severity: P2
  - location：tests/unit/model/anthropic-messages-parity.test.ts
  - description：需要增加一个 custom provider JSON 配置（models.json 格式）的集成测试，验证通过 Rust backend 使用 anthropic-messages 协议的全链路：model metadata 加载 → bridge 分发 → payload 构建 → fake server 响应 → stream event 正确返回。确保非 Anthropic 直连场景（如 Fireworks、自定义 endpoint）的 custom provider 配置能正确工作。
  - reproduce：目前 parity test 仅测试 provider=anthropic 的情况，未覆盖 custom provider JSON 配置场景
  - fix：
