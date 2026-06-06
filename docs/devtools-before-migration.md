# devtools/before 工具迁移方案

> 生成时间：2026-06-06
> 背景：项目正在从 TypeScript 逐步迁移到 Rust，devtools/before/ 中的 23 个脚本需要规划迁移/保留/删除策略。
>
> **核心约束**：TypeScript 依赖将逐步移除。最终状态下，项目只保留 Rust 和解释型脚本（shell / Python）。

## 总览

| 维度 | 统计 |
|------|------|
| 脚本总数 | 23 |
| Node.js (.mjs) | 12 |
| TypeScript (.ts) | 5 |
| Shell (.sh) | 4 |
| Batch (.bat) | 1 |
| PowerShell (.ps1) | 1 |
| 建议迁移到 Python | 3 |
| 建议保持现状 | 11 |
| 建议 TS 退役后删除 | 9 |

### 按模块分布

| 模块 | 数量 | 包含脚本 |
|------|------|----------|
| Stats（统计分析） | 6 | cost.ts, stats.ts, edit-tool-stats.mjs, read-tool-stats.mjs, session-context-stats.mjs, tool-stats.ts |
| CI（持续集成检查） | 4 | check-pinned-deps.mjs, check-lockfile-commit.mjs, check-ts-relative-imports.mjs, check-browser-smoke.mjs |
| Tooling（开发工具） | 4 | sync-versions.js, update-source-imports-to-ts.sh, generate-coding-agent-shrinkwrap.mjs, session-transcripts.ts |
| Test（测试入口） | 4 | test.sh, pi-test.sh, pi-test.ps1, pi-test.bat, browser-smoke-entry.ts |
| Build（构建） | 2 | build-binaries.sh, profile-coding-agent-node.mjs |
| Release（发布） | 2 | release.mjs, local-release.mjs |

---

## 迁移原则

### 语言选择决策树

```
脚本逻辑简单，以调用外部命令为主？
├── 是 → shell（已是最佳形态）
└── 否 → 逻辑复杂度如何？
    ├── 纯计算/数据处理，频繁修改？
    │   └── Python（解释执行，编辑即跑，stdlib 覆盖充分）
    ├── 性能敏感（海量数据/实时）？
    │   └── Rust（类型安全 + 编译优化）
    └── 强依赖 npm/Node 生态？
        └── 保持 Node（等依赖消除后再评估）
```

### 关键认知

1. **`.mjs` 已经是解释执行** — `node foo.mjs` 就能跑。用 Python 重写只是换解释器，不带来"编辑即跑"的额外收益。
2. **`.ts` 是真正的痛点** — 需要编译步骤（`tsc` / `tsx` / `ts-node`）。迁移到 Python 能消除编译等待。
3. **Shell 脚本不需要动** — 对于编排外部命令的场景，shell 已经是最简洁最合适的语言。
4. **Rust 不适合频繁迭代的 devtools** — 每次改完等 `cargo build` 的摩擦成本高于类型安全带来的收益。devtools 的瓶颈是开发者迭代速度，不是运行时性能。
5. **Python 无需 pip 依赖** — 这些脚本只用到 `json`、`pathlib`、`argparse`、`datetime`、`statistics`、`webbrowser`、`html` 等 stdlib 模块。

---

## 分类方案

### 第一类：迁移到 Python（3 个 .ts 文件）

这三个文件当前需要编译才能运行，迁移到 Python 后消除编译步骤，且逻辑完全可用纯 stdlib 覆盖。

#### cost.ts → cost_stats.py

- **当前**：~184 行 TypeScript，需 `tsx` 或编译后运行
- **功能**：解析 JSONL 会话日志，按日/提供商聚合 token 费用，格式化表格输出
- **Python 映射**：`json` + `pathlib` + `argparse` + `datetime` + `collections.defaultdict`
- **难度**：低
- **建议**：与 stats.ts 合并为单一脚本 `cost_stats.py`，两者共享会话解析和聚合逻辑

#### stats.ts → 合并到 cost_stats.py

- **当前**：~234 行 TypeScript，需编译
- **功能**：与 cost.ts 高度重叠 — JSONL 解析、本地时间日期分桶、多级提供商聚合
- **建议**：作为 `cost_stats.py` 的子命令（`python cost_stats.py cost` / `python cost_stats.py stats`）
- **收益**：消除约 200 行重复的会话解析和日期聚合逻辑

#### tool-stats.ts → tool_stats.py

- **当前**：~233 行 TypeScript + 嵌入的 HTML/JS 模板
- **功能**：JSONL 处理 → token 估算 → 直方图分桶 → HTML Dashboard（Tailwind CSS + Chart.js CDN）→ 自动打开浏览器
- **Python 映射**：`json` + `pathlib` + `statistics` + `webbrowser` + HTML 用 f-string 模板
- **难度**：低
- **收益**：Dashboard 样式迭代不再需要编译，改完 HTML/CSS 立刻跑

---

### 第二类：保持现状 — 解释型（8 个 .mjs）

这些文件已经是解释执行，不需要编译，也没有阻塞性的依赖问题。

| 脚本 | 行数 | 功能 | 后续计划 |
|------|------|------|----------|
| **check-pinned-deps.mjs** | ~64 | 递归检查 package.json 中依赖版本是否 pin 死 | CI 稳定后无需修改，保持 Node |
| **check-lockfile-commit.mjs** | ~100 | 确保 package-lock.json 有对应的 package.json 变更 | npm 体系内，随 npm 退役后删除 |
| **edit-tool-stats.mjs** | ~835 | 编辑工具差异分析（LCP/LCS 算法、分位数统计、多维度分组） | 等 Node 退役时评估是否迁 Python |
| **read-tool-stats.mjs** | ~506 | 工具使用统计（时区感知分桶、Unicode 柱状图、三格式输出） | 同上 |
| **session-context-stats.mjs** | ~406 | 会话上下文窗口统计、压缩追踪 | 数据源（models.generated.ts）需先迁出 TS |
| **local-release.mjs** | ~270 | npm pack → tarball → 本地安装验证 | npm 体系内，随 npm 退役后删除 |
| **release.mjs** | ~130 | npm 发布流程编排（git tag + npm publish） | 同上 |
| **profile-coding-agent-node.mjs** | ~530 | Node 进程性能采样（spawn + 解析 stderr 时序） | rozsa 有二进制后可改造为 Rust benchmark |

这些文件无需立即处理。策略是：**等 TypeScript 包和 npm 体系彻底移除后，再逐一评估哪些 .mjs 仍然需要，需要的是保留 Node 还是迁移到 Python。**

---

### 第三类：保持现状 — Shell（4 个）

Shell 脚本处于"最佳形态"——它们编排外部命令、操作文件系统、设置环境变量，没有数据处理的复杂逻辑。

| 脚本 | 功能 | 理由不动 |
|------|------|----------|
| **test.sh** | 环境清理 + `npm test` + trap 恢复 auth.json | shell 的 `trap` 机制是理想方案 |
| **build-binaries.sh** | 调用 npm/bun/tar/zip 构建多平台二进制包 | shell 编排外部 CLI 最简洁 |
| **pi-test.sh** | 解析参数 + 取消环境变量 + `tsx` 启动 coding-agent | 临时启动器，coding-agent 退役后删除 |
| **pi-test.ps1** | Windows PowerShell 版启动器 | 同上 |
| **pi-test.bat** | cmd.exe → PowerShell 桥接（12 行） | 同上 |

---

### 第四类：TS 退役后删除（9 个）

这些脚本的存在意义完全依赖 TypeScript 遗留包。TS 移除后它们自然成为死代码。

| 脚本 | 删除前提 |
|------|----------|
| **browser-smoke-entry.ts** | 浏览器包导出的 TS 入口点，无 TS 包则无用 |
| **check-browser-smoke.mjs** | 配套的 esbuild 冒烟测试运行器 |
| **check-ts-relative-imports.mjs** | TS 都没有了，不需要检查 ".js 在 TS 导入中非法" |
| **sync-versions.js** | npm workspace 内 package.json 版本同步，npm 包退役后无意义 |
| **generate-coding-agent-shrinkwrap.mjs** | 为 coding-agent npm 包生成 shrinkwrap |
| **update-source-imports-to-ts.sh** | TS 迁移期工具，改写 `.js` → `.ts` 导入 |
| **pi-test.sh** | coding-agent CLI 已迁移到 Rust rozsa binary |
| **pi-test.ps1** | 同上 |
| **pi-test.bat** | 同上 |
| **session-transcripts.ts** | 深度依赖 TS 包 `parseSessionEntries` + `pi` CLI |

---

## 分阶段执行计划

### Phase 1：速赢 — 3 个 .ts → Python（可立即执行）

**目标**：消除编译步骤，合并重复逻辑

```
任务 1.1: 创建 docs/before/devtools-migration/cost_stats.py
  - 合并 cost.ts + stats.ts
  - 子命令: cost / stats
  - Python stdlib only

任务 1.2: 创建 docs/before/devtools-migration/tool_stats.py
  - 从 tool-stats.ts 翻译
  - 保持 HTML Dashboard 输出不变

任务 1.3: 验证
  - 用历史 JSONL 数据对比输出一致性
  - 确认后删除 cost.ts, stats.ts, tool-stats.ts

任务 1.4: 更新 CI 引用
  - 检查 npm run check 中是否引用了这些脚本
  - 更新为 python 调用
```

**预计工作量**：0.5–1 天

### Phase 2：跟随 TS 退役 — 删除 9 个脚本（逐步）

**触发条件**：对应的 TypeScript 包不再存在

按依赖关系分组删除：

| 删除批次 | 脚本 | 触发条件 |
|----------|------|----------|
| 批次 A | update-source-imports-to-ts.sh | 最后一批 TS 源码文件被移除 |
| 批次 B | check-ts-relative-imports.mjs | 同上 |
| 批次 C | browser-smoke-entry.ts, check-browser-smoke.mjs | 浏览器给向 TS 包退役 |
| 批次 D | sync-versions.js, generate-coding-agent-shrinkwrap.mjs | npm workspace 退役 |
| 批次 E | pi-test.sh, pi-test.ps1, pi-test.bat, session-transcripts.ts | coding-agent 完全迁移到 Rust |

**预计工作量**：只需删除文件和更新 CI/文档引用，每批次几分钟。

### Phase 3：等 Node 退役 — 评估 .mjs 去向（远期）

**触发条件**：决定从项目中彻底移除 Node.js 运行时

此时剩余的 `.mjs` 脚本需要逐一评估：

| 脚本 | 迁移目标 | 理由 |
|------|----------|------|
| check-pinned-deps.mjs | Python | 简单 JSON + fs，Python 胜任 |
| check-lockfile-commit.mjs | Python 或删除 | 如果 npm 仍存在则保 Node，否则迁 Python |
| edit-tool-stats.mjs | Python | 逻辑复杂但非性能敏感，Python statistics 模块够用 |
| read-tool-stats.mjs | Python | 注意时区处理从 Intl.DateTimeFormat 换 zoneinfo |
| session-context-stats.mjs | Python 或 Rust | 取决于数据源是否已入 `rozsa-model` crate |
| local-release.mjs | Python 或删除 | 取决于是否还做 npm 发布 |
| release.mjs | Python 或删除 | 同上 |
| profile-coding-agent-node.mjs | Rust | 届时 rozsa 已有二进制，集成到 criterion benchmark |

**预计工作量**：2–3 天（取决于保留多少脚本）

---

## 未来目标目录结构

TS 和 npm 全部移除后的理想状态：

```
devtools/
├── cost_stats.py              # 合并 cost + stats（Phase 1）
├── tool_stats.py              # HTML Dashboard（Phase 1）
├── test.sh                    # 不动
├── build-binaries.sh          # 不动
└── bench/
    └── rozsa_bench.rs         # profile-coding-agent-node.mjs 的 Rust 替代（Phase 3）
```

如果决定保留 Node 运行时，则中间状态：

```
devtools/
├── before/                    # 保留的 .mjs 脚本
│   ├── check-pinned-deps.mjs
│   ├── edit-tool-stats.mjs
│   ├── read-tool-stats.mjs
│   └── session-context-stats.mjs
├── cost_stats.py
├── tool_stats.py
├── test.sh
└── build-binaries.sh
```

---

## 风险备忘

1. **会话日志格式兼容**：Python 重写的统计脚本必须与现有 JSONL 格式兼容。Phase 1 执行时需对比输出。
2. **session-context-stats 的数据源**：该脚本从 `packages/ai/src/models.generated.ts` 用 regex 提取模型上下文窗口信息。这个数据需要在 Python（或 Rust）中以某种形式可用。
3. **删除时机**：9 个删除候选脚本中，有些可能比预期存续更久。每次删除前确认对应 TS 包已确认退役。
4. **CI 管道依赖**：`npm run check` 可能引用了这些脚本。任何迁移/删除都需同步更新 CI 配置。
5. **Python 版本**：macOS 自带 Python 3，建议设定最低版本要求（如 Python 3.10+，确保 `zoneinfo` 等模块可用）。

---

## 变更记录

| 日期 | 变更 |
|------|------|
| 2026-06-06 | 初始版本，全量分析 23 个脚本 |
