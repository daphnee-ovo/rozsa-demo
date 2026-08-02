---
source: test
nums: 1
---

- [x] ISSUE-I002：修复 Dev-flow dashboard 启动超时竞态与 ReconnectBackoff Default 告警
  - severity: P1
  - location：crates/rozsa-app/src/dev_flow/dashboard.rs:730
  - description：start_dashboard 在 spawn 前计算 startup deadline：高负载下 spawn 可能耗尽 1s 超时窗口，子进程未及执行即被杀，导致 failed_startup_kills_and_reaps_the_owned_child 偶发读取不到 pid 文件；另有 ReconnectBackoff 手写 Default 触发 clippy derivable_impls。
  - reproduce：cargo test -p rozsa-app --test dev_flow_dashboard_test -- failed_startup_kills_and_reaps_the_owned_child 多次运行可见偶发失败（本会话 3 次中 1 次失败）；cargo clippy -p rozsa-app --all-targets 报 derivable_impls。
  - fix：start_dashboard 将 startup deadline 移到每次 spawn 之后（每个端口尝试独立计时），子进程始终获得完整启动窗口，消除高负载下 spawn 超时导致子进程未执行即被杀、pid 文件缺失的竞态；ReconnectBackoff 改为派生 Default 消除 clippy derivable_impls 告警。回归验证：failed_startup_kills_and_reaps_the_owned_child 修复前本会话 3 次中 1 次失败，修复后 6/6 通过，dev_flow_dashboard_test 全套 12/12 通过，clippy 无 dashboard.rs 告警。
  - files_modify: [crates/rozsa-app/src/dev_flow/dashboard.rs, crates/rozsa-app/tests/dev_flow_dashboard_test.rs]
  - files_create: []
