# SPEC: 移除 Bridge 协议层

## Goal

删除三个 TS→Rust JSONL bridge binary 及其 protocol 模块。TS 遗留代码不再运行，
Rust native 模式（rozsa-cli → NativeBackend）是唯一入口。移除 bridge 后消除
三套独立协议的维护成本，简化 crate 间调用为直接函数调用。

## Design

### 删除清单

| 文件 | 说明 |
|------|------|
| `crates/rozsa-core/src/bin/bridge.rs` | core bridge binary |
| `crates/rozsa-core/src/protocol.rs` | core bridge 协议类型 |
| `crates/rozsa-core/src/protocol_tests.rs` | 协议序列化测试 |
| `crates/rozsa-model/src/main.rs` | model bridge binary |
| `crates/rozsa-model/src/protocol.rs` | model bridge 协议类型 |
| `crates/rozsa-app/src/main.rs` | app bridge binary |

### Cargo.toml 修改

**rozsa-core/Cargo.toml:**
- 删除 `[[bin]] name = "rozsa-core" path = "src/bin/bridge.rs"`
- 如果有仅 bridge 使用的依赖（如 `tokio` stdin/stdout 相关 features），检查是否仍需保留

**rozsa-model/Cargo.toml:**
- 删除默认 binary（`src/main.rs` 作为 default bin）
- 确保 crate 仍作为 `[lib]` 正常工作（已有 `lib.rs`）

**rozsa-app/Cargo.toml:**
- 删除 `src/main.rs` 作为 default bin
- 保留 `[lib]` 入口

### lib.rs 修改

**rozsa-core/src/lib.rs:**
- 删除 `pub mod protocol;`
- 删除 `mod protocol_tests;`

**rozsa-model/src/lib.rs:**
- 删除 `pub mod protocol;`

### 验证无内部依赖

已确认：
- `rozsa-core::protocol` 的类型（BridgeInput/BridgeOutput/RunMode/BridgeConfig）仅被 `bin/bridge.rs` 使用
- `rozsa-model::protocol` 仅被 `main.rs` 使用
- `rozsa-app::main.rs` 内联定义类型，无外部消费者
- `AgentEvent`（在 `events.rs` 中定义）不受影响 — 它独立于 protocol 模块

### 可能的依赖清理

bridge binary 可能引入了仅自身使用的 crate 依赖（如 `tokio::io` stdin features）。
删除后如果 `cargo check` 报 unused dependency，一并清理。

### RUST_DIFF_DECISIONS.md 更新

记录此设计决策：TS bridge 层已移除，crate 间通信为直接 Rust 函数调用。

## Acceptance

- SPEC-AC-001: `cargo check` 全 workspace 编译通过，无 bridge binary 残留
- SPEC-AC-002: `cargo build` 不再产生 `rozsa-core`、`rozsa-model` binary（只有 `rozsa-cli` 和 `rozsa-tui`）
- SPEC-AC-003: `rozsa-core/src/lib.rs` 不再导出 `protocol` 模块
- SPEC-AC-004: `rozsa-model/src/lib.rs` 不再导出 `protocol` 模块
- SPEC-AC-005: 现有 `cargo test` 通过（排除已删除的 protocol_tests）

## Test Plan

- `cargo check` 全 workspace
- `cargo build` 确认只产出 rozsa-cli / rozsa-tui binary
- `cargo test -p rozsa-core -p rozsa-model -p rozsa-app` 通过

## Self Check
- [x] Goal is clear
- [x] Acceptance criteria are testable
- [x] Matches current mode (fast)
