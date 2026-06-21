---
source: other
nums: 1
---

- [x] ISSUE-I005：移除多余的 ROZSA_MODEL_RUST_APIS 环境变量门控
  - severity: P1
  - location：packages/ai/src/providers/rozsa-model-bridge.ts:241
  - description：shouldUseRustModelProvider 有两道门控：isRustModelSupportedApi（代码白名单）和 rustApiSet（ROZSA_MODEL_RUST_APIS 环境变量）。后者是多余的灰度控制，每加一个新 provider 都要同步改 run.sh 默认值，且 ROZSA_MODEL_BACKEND=rust 的语义本身已经足够明确。应删除 rustApiSet 检查，只保留 isRustModelSupportedApi。
  - fix：
