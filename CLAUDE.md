# Rózsa

AI coding agent — Rust 重写中，TypeScript 遗留代码仍在运行。

## 开发

- 检查：`npm run check`（biome + ts + pinned-deps + shrinkwrap）
- 测试：`./devtools/before/test.sh`（不要直接跑全量 vitest）
- Rust 构建：`cargo build`
- TS 构建：`npm run build`

## 技术栈

- Rust (Cargo workspace, 5 crates)
- TypeScript (Node, ESM) — 迁移中
- Biome (lint/format)
- tsgo (type check)
- npm workspaces

## 项目结构

- `crates/` — Rust workspace crates (rozsa-model, rozsa-core, rozsa-app, rozsa-tui, rozsa-cli)
- `packages/` — TypeScript 遗留包（迁移源）
- `docs/` — 文档
- `devtools/` — 构建/检查脚本
- `tests/` — 集成测试

## 代码风格

- Rust: rustfmt + clippy
- TypeScript: Biome 管理 lint 和 format，erasable syntax only
- 无 inline imports，仅 top-level
- 提交前必须 `npm run check` 全通过

## 核心原则

本项目遵循 [Core Rule](https://github.com/daphnee-ovo/code-core-rule) 中定义的软件工程原则，完整内容已集成到 `AGENTS.md` 的 "Core Principles" 章节。核心要点：

- 安全系统优先级最高，不得绕过
- 需求不明确时先确认，范围扩大时暂停请求确认
- 代码与文档双向可追踪
- 优先简单实现，组合优于集成
- 高内聚低耦合，显式约束优于运行时约定
- 错误透明报告，快速失败，不隐藏问题
- 信任已有测试

## 参考

详细的开发规则、Git 规范、核心原则见 `AGENTS.md`。
