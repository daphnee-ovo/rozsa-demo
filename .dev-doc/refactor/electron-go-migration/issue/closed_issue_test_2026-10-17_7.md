---
source: test
nums: 1
---

- [x] ISSUE-I007：T018 完成检查在 sandbox 内未取得 packaged-app 启动诊断
  - severity: P1
  - location：backend/internal/model/anthropic/provider_test.go
  - description：项目 test.ci 运行既有 desktop/tests/package-resources.test.ts，在 sandbox 内报告 Packaged app exited before startup diagnostic (null)。同一未修改的检查在 macOS host 权限下通过，输出 Forge packaged resources PASS。Go Anthropic 专用测试、全模块 race 与 Rust 类型/OAuth 测试通过；该 packaged-app 检查未引用新 provider，且 I004/I005 已记录同样失败。
  - reproduce：npm --prefix desktop/tests exec -- tsx desktop/tests/package-resources.test.ts
    project_root: /Users/xinyue/ballad/rozsa-demo
  - fix：保持 packaged-app 检查原样，以 host 权限重跑完成检查；未修改产品或测试以绕过失败。
  - files_modify: [backend/internal/model/anthropic/provider_test.go]
  - files_create: []
