/**
 * LSP (Language Server Protocol) 内置模块
 *
 * lsp/
 * ├── index.ts          — 公共导出
 * ├── lsp-core.ts       — 核心引擎：服务器管理、连接、协议通信
 * ├── lsp-tool.ts       — AI 可调用的 LSP 查询工具定义
 * └── lsp-hook.ts       — 自动诊断 hook：文件追踪、诊断收集、消息注入
 */

export { LSPManager } from "./lsp-core.ts";
export type { LSPDiagnosticsResult, LSPHookOptions } from "./lsp-hook.ts";
export { hasActionableErrors, LSPHook } from "./lsp-hook.ts";
export { lspTool } from "./lsp-tool.ts";
