# Development tools

每个工具必须在文件开头简短说明用途；调用细节只在此处补充，避免在脚本与文档间重复堆积。

## Electron + Go desktop

- `start-desktop.sh`：编译当前代码并通过 Forge/Vite 启动调试应用；默认加载 `tmp/desktop-debug/sessions/` 的 Markdown、Mermaid、工具调用和长历史合成示例。已有示例文件不会覆盖。应用参数原样跟在脚本后；显式传 `--session-dir /absolute/sessions` 或 `--session-dir=/absolute/sessions` 时仅使用指定目录。不生成 `.app` 包。
- `show-interruption-states.mjs`：启动隔离的 Electron + Go 展示实例；本地合成 provider 会在两个 session 中分别挂起 Permission Request 和 AskUserQuestion，切换侧栏查看同一个 renderer mount 的不同状态。按 Ctrl+C 关闭。
- `prepare-debug-sessions.mjs`：生成上述示例会话；可传目录作为第一个参数，已有同名文件保留。
- `build-desktop.sh`：全新安装桌面依赖并生成当前主机的应用包；首次设置或依赖变更后使用。
- `package-desktop.sh`：依赖已安装时生成应用包；传入 `make` 生成当前主机安装包，其余参数传给 Forge。
- `test-desktop.sh`：运行 Electron + Go 跨语言验收，不启动完整 TEST phase。
- `check-error-display.sh`：列出指定源码中可能直接展示异常的位置供人工复核；候选结果不是违规结论，通知与状态文案需结合上下文判断。调用示例：`bash devtools/check-error-display.sh desktop/renderer/scripts/features/settings.tsx desktop/renderer/runtime/app/capability-permissions.ts`。
- `update-builtin-skills-config.mjs`：在桌面启动或打包前，把内置技能目录清单写入 backend 配置。

## Retained Rust/Tauri checks and utilities

- `deploy-test-app.sh`：构建、签名并启动一次性 macOS Rust/Tauri 测试应用。
- `test-gui.sh`：运行 Rust/Tauri GUI 的 JavaScript 与 Rust 测试，可传 `--js` 或 `--rust` 缩小范围。
- `test-shared-gui-acceptance.sh`：运行 Rust/Tauri 共享 GUI 验收目标。
- `test-shared-ui-cleanup.sh`：验证共享 UI 源码清理后的 Rust/Tauri 行为。
- `test-sidebar-dev-flow.sh`：运行侧栏与 Dev Flow 的聚焦行为检查。
- `test-prompt-path.sh`：使用一次性本地 provider smoke-test Rust prompt path。
- `fake-anthropic-server.py`：提供可控的 Anthropic Messages API 测试服务；通过 `uv run devtools/fake-anthropic-server.py` 启动。
- `sync-codex-model-client-version.sh`：从官方 `openai/codex` tag 同步 models endpoint client version；传 `--check` 仅验证。
- `sync-cargo-version.sh`：把 workspace Cargo 版本同步到指定版本；由 `/iterate` 调用，也可传入版本手动执行。
- `guard-no-inline-test.sh`：Claude Code 写入前阻止在 Rust 生产源码内嵌测试。
- `pre-commit-no-inline-test.sh`：提交前检查暂存 Rust 生产源码是否含内嵌测试。
