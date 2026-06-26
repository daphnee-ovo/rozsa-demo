/**
 * LSP Core Engine — 语言服务器协议连接管理核心引擎
 *
 * 架构树:
 * lsp-core.ts
 * ├── LSP_SERVERS              — 9 种语言服务器配置定义
 * ├── LSPManager (class)       — 单例管理器，按工作目录维护连接
 * │   ├── getOrCreateManager() — 静态工厂
 * │   ├── getServerForFile()   — 惰性启动连接
 * │   ├── shutdownAll()        — 优雅关闭所有连接
 * │   ├── touchFile()          — 打开/更新文件内容
 * │   ├── closeFile()          — 关闭文件
 * │   ├── getDiagnostics()     — 获取诊断信息
 * │   ├── getDefinition()      — 跳转定义
 * │   ├── getReferences()      — 查找引用
 * │   ├── getHover()           — 悬停信息
 * │   ├── getSignatureHelp()   — 签名帮助
 * │   ├── getDocumentSymbols() — 文档符号
 * │   ├── rename()             — 重命名
 * │   └── getCodeActions()     — 代码操作
 * ├── LSPConnection (class)    — 单个语言服务器进程封装
 * │   ├── initialize()         — 初始化握手
 * │   ├── openFile()           — 打开文件通知
 * │   ├── updateFile()         — 更新文件内容
 * │   ├── closeFile()          — 关闭文件通知
 * │   └── shutdown()           — 关闭连接
 * └── 工具函数
 *     ├── which()              — 查找二进制文件路径
 *     ├── findProjectRoot()    — 向上查找项目根目录
 *     ├── fileToUri()          — 文件路径转 URI
 *     └── uriToFile()          — URI 转文件路径
 */

import type { ChildProcess } from "node:child_process";
import { EventEmitter } from "node:events";
import { existsSync, statSync } from "node:fs";
import { dirname, extname, join, resolve, sep } from "node:path";
import spawn from "cross-spawn";
import {
	type CodeAction,
	CodeActionRequest,
	createMessageConnection,
	DefinitionRequest,
	type Diagnostic,
	DidChangeTextDocumentNotification,
	DidCloseTextDocumentNotification,
	DidOpenTextDocumentNotification,
	type DocumentSymbol,
	DocumentSymbolRequest,
	ExitNotification,
	HoverRequest,
	InitializedNotification,
	InitializeRequest,
	type Location,
	type MessageConnection,
	PublishDiagnosticsNotification,
	ReferencesRequest,
	RenameRequest,
	ShutdownRequest,
	SignatureHelpRequest,
	StreamMessageReader,
	StreamMessageWriter,
	type WorkspaceEdit,
} from "vscode-languageserver-protocol/node.js";

// ============================================================
// 工具函数
// ============================================================

/** 已知的额外 PATH 搜索路径 */
const EXTRA_PATH_DIRS = [
	join(process.env.HOME ?? "~", ".cargo", "bin"),
	join(process.env.HOME ?? "~", "go", "bin"),
	join(process.env.HOME ?? "~", ".pub-cache", "bin"),
	"/opt/homebrew/bin",
	"/usr/local/bin",
];

/**
 * 在 PATH 及常见位置中查找可执行文件
 * @returns 可执行文件绝对路径，未找到返回 null
 */
export function which(binary: string): string | null {
	const pathEnv = process.env.PATH ?? "";
	const dirs = [...pathEnv.split(sep === "\\" ? ";" : ":"), ...EXTRA_PATH_DIRS];

	for (const dir of dirs) {
		if (!dir) continue;
		const candidate = join(dir, binary);
		try {
			const stat = statSync(candidate);
			if (stat.isFile()) {
				return candidate;
			}
		} catch {
			// 文件不存在，继续搜索
		}
	}
	return null;
}

/**
 * 从给定文件路径向上查找包含指定标记文件/目录的项目根
 * @param filePath - 起始文件路径
 * @param markers - 标记文件或目录名列表（如 ["package.json", "tsconfig.json"]）
 * @returns 项目根目录路径，未找到返回 null
 */
export function findProjectRoot(filePath: string, markers: string[]): string | null {
	let current = dirname(resolve(filePath));
	const root = resolve("/");

	while (current !== root) {
		for (const marker of markers) {
			const markerPath = join(current, marker);
			if (existsSync(markerPath)) {
				return current;
			}
		}
		const parent = dirname(current);
		if (parent === current) break;
		current = parent;
	}
	return null;
}

/** 文件路径转 LSP URI */
function fileToUri(filePath: string): string {
	const normalized = resolve(filePath);
	return `file://${normalized}`;
}

/** LSP URI 转文件路径 */
function uriToFile(uri: string): string {
	return uri.replace("file://", "");
}

/**
 * 根据文件扩展名猜测 languageId
 */
function getLanguageId(filePath: string): string {
	const ext = extname(filePath).toLowerCase();
	const map: Record<string, string> = {
		".ts": "typescript",
		".tsx": "typescriptreact",
		".js": "javascript",
		".jsx": "javascriptreact",
		".mts": "typescript",
		".cts": "typescript",
		".mjs": "javascript",
		".cjs": "javascript",
		".py": "python",
		".rs": "rust",
		".go": "go",
		".dart": "dart",
		".kt": "kotlin",
		".kts": "kotlin",
		".swift": "swift",
		".c": "c",
		".h": "c",
		".cpp": "cpp",
		".cxx": "cpp",
		".cc": "cpp",
		".hpp": "cpp",
		".hxx": "cpp",
		".java": "java",
	};
	return map[ext] ?? "plaintext";
}

// ============================================================
// 语言服务器配置
// ============================================================

/** 语言服务器配置定义 */
export interface LSPServerConfig {
	/** 唯一标识 */
	id: string;
	/** 显示名称 */
	name: string;
	/** 支持的文件扩展名（包含点号） */
	extensions: string[];
	/** 根据文件路径查找项目根目录 */
	findRoot(filePath: string): string | null;
	/** 在项目根目录启动语言服务器进程，缺少二进制则返回 null */
	spawn(root: string): ChildProcess | null;
}

/** 9 种语言的服务器配置 */
export const LSP_SERVERS: LSPServerConfig[] = [
	// ---- TypeScript / JavaScript ----
	{
		id: "typescript",
		name: "TypeScript Language Server",
		extensions: [".ts", ".tsx", ".js", ".jsx", ".mts", ".cts", ".mjs", ".cjs"],
		findRoot(filePath: string): string | null {
			return findProjectRoot(filePath, ["tsconfig.json", "jsconfig.json", "package.json"]);
		},
		spawn(root: string): ChildProcess | null {
			const bin = which("typescript-language-server");
			if (!bin) return null;
			return spawn(bin, ["--stdio"], { cwd: root, stdio: "pipe" });
		},
	},

	// ---- Python ----
	{
		id: "python",
		name: "Python Language Server (Pyright)",
		extensions: [".py", ".pyi"],
		findRoot(filePath: string): string | null {
			return findProjectRoot(filePath, [
				"pyproject.toml",
				"setup.py",
				"setup.cfg",
				"requirements.txt",
				"Pipfile",
				".python-version",
			]);
		},
		spawn(root: string): ChildProcess | null {
			// 优先尝试 pyright，回退到 pylsp
			const pyright = which("pyright-langserver");
			if (pyright) {
				return spawn(pyright, ["--stdio"], { cwd: root, stdio: "pipe" });
			}
			const pylsp = which("pylsp");
			if (pylsp) {
				return spawn(pylsp, [], { cwd: root, stdio: "pipe" });
			}
			return null;
		},
	},

	// ---- Rust ----
	{
		id: "rust",
		name: "rust-analyzer",
		extensions: [".rs"],
		findRoot(filePath: string): string | null {
			return findProjectRoot(filePath, ["Cargo.toml"]);
		},
		spawn(root: string): ChildProcess | null {
			const bin = which("rust-analyzer");
			if (!bin) return null;
			return spawn(bin, [], { cwd: root, stdio: "pipe" });
		},
	},

	// ---- Go ----
	{
		id: "go",
		name: "gopls",
		extensions: [".go"],
		findRoot(filePath: string): string | null {
			return findProjectRoot(filePath, ["go.mod", "go.sum"]);
		},
		spawn(root: string): ChildProcess | null {
			const bin = which("gopls");
			if (!bin) return null;
			return spawn(bin, ["serve"], { cwd: root, stdio: "pipe" });
		},
	},

	// ---- Dart ----
	{
		id: "dart",
		name: "Dart Language Server",
		extensions: [".dart"],
		findRoot(filePath: string): string | null {
			return findProjectRoot(filePath, ["pubspec.yaml", "pubspec.lock"]);
		},
		spawn(root: string): ChildProcess | null {
			const bin = which("dart");
			if (!bin) return null;
			return spawn(bin, ["language-server", "--protocol=lsp"], { cwd: root, stdio: "pipe" });
		},
	},

	// ---- Kotlin ----
	{
		id: "kotlin",
		name: "Kotlin Language Server",
		extensions: [".kt", ".kts"],
		findRoot(filePath: string): string | null {
			return findProjectRoot(filePath, [
				"build.gradle.kts",
				"build.gradle",
				"settings.gradle.kts",
				"settings.gradle",
				"pom.xml",
			]);
		},
		spawn(root: string): ChildProcess | null {
			const bin = which("kotlin-language-server");
			if (!bin) return null;
			return spawn(bin, [], { cwd: root, stdio: "pipe" });
		},
	},

	// ---- Swift ----
	{
		id: "swift",
		name: "SourceKit-LSP",
		extensions: [".swift"],
		findRoot(filePath: string): string | null {
			return findProjectRoot(filePath, ["Package.swift", ".swiftpm"]);
		},
		spawn(root: string): ChildProcess | null {
			const bin = which("sourcekit-lsp");
			if (!bin) return null;
			return spawn(bin, [], { cwd: root, stdio: "pipe" });
		},
	},

	// ---- C/C++ ----
	{
		id: "cpp",
		name: "clangd",
		extensions: [".c", ".h", ".cpp", ".cxx", ".cc", ".hpp", ".hxx"],
		findRoot(filePath: string): string | null {
			return findProjectRoot(filePath, [
				"compile_commands.json",
				"CMakeLists.txt",
				".clangd",
				"Makefile",
				"meson.build",
			]);
		},
		spawn(root: string): ChildProcess | null {
			const bin = which("clangd");
			if (!bin) return null;
			return spawn(bin, ["--background-index"], { cwd: root, stdio: "pipe" });
		},
	},

	// ---- Java ----
	{
		id: "java",
		name: "Eclipse JDT Language Server",
		extensions: [".java"],
		findRoot(filePath: string): string | null {
			return findProjectRoot(filePath, [
				"pom.xml",
				"build.gradle",
				"build.gradle.kts",
				".classpath",
				"settings.gradle",
			]);
		},
		spawn(root: string): ChildProcess | null {
			const bin = which("jdtls");
			if (!bin) return null;
			return spawn(bin, [], { cwd: root, stdio: "pipe" });
		},
	},
];

// ============================================================
// LSPConnection — 单个语言服务器连接封装
// ============================================================

/** 已打开文件的追踪信息 */
interface OpenFileEntry {
	uri: string;
	languageId: string;
	version: number;
	lastAccessTime: number;
}

/** 连接状态 */
type ConnectionState = "idle" | "initializing" | "ready" | "broken" | "shutdown";

/** LSP 连接配置 */
interface LSPConnectionOptions {
	serverId: string;
	serverName: string;
	projectRoot: string;
	process: ChildProcess;
}

/** LRU 最大打开文件数 */
const MAX_OPEN_FILES = 30;

/** 文件空闲超时 (ms) */
const FILE_IDLE_TIMEOUT = 60_000;

/** 初始化超时 (ms) */
const INIT_TIMEOUT = 30_000;

/**
 * LSPConnection — 封装单个语言服务器进程的生命周期和通信
 */
class LSPConnection extends EventEmitter {
	readonly serverId: string;
	readonly serverName: string;
	readonly projectRoot: string;

	private process: ChildProcess;
	private connection: MessageConnection | null = null;
	private state: ConnectionState = "idle";
	private initPromise: Promise<void> | null = null;

	/** 已打开的文件映射: URI -> 文件信息 */
	private openFiles = new Map<string, OpenFileEntry>();

	/** 按文件缓存的诊断信息 */
	private diagnosticsCache = new Map<string, Diagnostic[]>();

	/** 文件空闲定时器 */
	private idleTimers = new Map<string, ReturnType<typeof setTimeout>>();

	constructor(options: LSPConnectionOptions) {
		super();
		this.serverId = options.serverId;
		this.serverName = options.serverName;
		this.projectRoot = options.projectRoot;
		this.process = options.process;
	}

	/** 获取当前连接状态 */
	getState(): ConnectionState {
		return this.state;
	}

	/**
	 * 初始化连接 — 执行 LSP initialize/initialized 握手
	 * 使用并发防护确保只初始化一次
	 */
	async initialize(signal?: AbortSignal): Promise<void> {
		if (this.state === "ready") return;
		if (this.state === "broken" || this.state === "shutdown") {
			throw new Error(`LSP connection [${this.serverName}] is ${this.state}`);
		}

		// 并发防护：如果已经在初始化中，等待已有的 Promise
		if (this.initPromise) {
			return this.initPromise;
		}

		this.state = "initializing";
		this.initPromise = this._doInitialize(signal);

		try {
			await this.initPromise;
		} finally {
			this.initPromise = null;
		}
	}

	private async _doInitialize(signal?: AbortSignal): Promise<void> {
		const { stdout, stdin } = this.process;
		if (!stdout || !stdin) {
			this.state = "broken";
			throw new Error(`LSP process [${this.serverName}] missing stdio streams`);
		}

		// 创建 LSP 消息连接
		const reader = new StreamMessageReader(stdout);
		const writer = new StreamMessageWriter(stdin);
		this.connection = createMessageConnection(reader, writer);

		// 注册诊断通知处理（push 模式）
		this.connection.onNotification(PublishDiagnosticsNotification.type, (params) => {
			const filePath = uriToFile(params.uri);
			this.diagnosticsCache.set(filePath, params.diagnostics);
			this.emit("diagnostics", filePath, params.diagnostics);
		});

		this.connection.listen();

		// 初始化超时控制
		const timeoutPromise = new Promise<never>((_, reject) => {
			const timer = setTimeout(() => {
				reject(new Error(`LSP initialize timeout (${INIT_TIMEOUT}ms) for ${this.serverName}`));
			}, INIT_TIMEOUT);
			// 允许 signal 提前中止
			signal?.addEventListener("abort", () => {
				clearTimeout(timer);
				reject(new Error("LSP initialize aborted"));
			});
		});

		try {
			await Promise.race([
				this.connection.sendRequest(InitializeRequest.type, {
					processId: process.pid,
					rootUri: fileToUri(this.projectRoot),
					rootPath: this.projectRoot,
					capabilities: {
						textDocument: {
							synchronization: {
								dynamicRegistration: false,
								willSave: false,
								willSaveWaitUntil: false,
								didSave: true,
							},
							completion: {
								dynamicRegistration: false,
								completionItem: { snippetSupport: false },
							},
							hover: { dynamicRegistration: false },
							signatureHelp: { dynamicRegistration: false },
							definition: { dynamicRegistration: false },
							references: { dynamicRegistration: false },
							documentSymbol: { dynamicRegistration: false },
							codeAction: { dynamicRegistration: false },
							rename: { dynamicRegistration: false },
							publishDiagnostics: { relatedInformation: true },
						},
						workspace: {
							workspaceFolders: true,
						},
					},
					workspaceFolders: [
						{
							uri: fileToUri(this.projectRoot),
							name: this.projectRoot.split(sep).pop() ?? "workspace",
						},
					],
				}),
				timeoutPromise,
			]);

			// 发送 initialized 通知
			this.connection.sendNotification(InitializedNotification.type, {});
			this.state = "ready";
		} catch (err) {
			this.state = "broken";
			this.connection.dispose();
			this.connection = null;
			throw err;
		}
	}

	/**
	 * 打开文件 — 发送 textDocument/didOpen 通知
	 */
	async openFile(filePath: string, content: string, languageId?: string): Promise<void> {
		if (this.state !== "ready" || !this.connection) {
			throw new Error(`LSP connection [${this.serverName}] not ready`);
		}

		const uri = fileToUri(filePath);
		const langId = languageId ?? getLanguageId(filePath);

		// 如果已经打开，使用 didChange 更新
		if (this.openFiles.has(uri)) {
			await this.updateFile(filePath, content);
			return;
		}

		// LRU 淘汰：如果打开文件数达到上限，关闭最久未访问的
		await this._evictIfNeeded();

		const entry: OpenFileEntry = {
			uri,
			languageId: langId,
			version: 1,
			lastAccessTime: Date.now(),
		};
		this.openFiles.set(uri, entry);

		this.connection.sendNotification(DidOpenTextDocumentNotification.type, {
			textDocument: {
				uri,
				languageId: langId,
				version: entry.version,
				text: content,
			},
		});

		// 重置空闲计时器
		this._resetIdleTimer(uri, filePath);
	}

	/**
	 * 更新已打开文件内容 — 发送 textDocument/didChange 通知（全量替换）
	 */
	async updateFile(filePath: string, content: string): Promise<void> {
		if (this.state !== "ready" || !this.connection) {
			throw new Error(`LSP connection [${this.serverName}] not ready`);
		}

		const uri = fileToUri(filePath);
		const entry = this.openFiles.get(uri);

		if (!entry) {
			// 文件未打开，先打开它
			await this.openFile(filePath, content);
			return;
		}

		entry.version++;
		entry.lastAccessTime = Date.now();

		this.connection.sendNotification(DidChangeTextDocumentNotification.type, {
			textDocument: { uri, version: entry.version },
			contentChanges: [{ text: content }],
		});

		this._resetIdleTimer(uri, filePath);
	}

	/**
	 * 关闭文件 — 发送 textDocument/didClose 通知
	 */
	closeFile(filePath: string): void {
		if (this.state !== "ready" || !this.connection) return;

		const uri = fileToUri(filePath);
		const entry = this.openFiles.get(uri);
		if (!entry) return;

		this.connection.sendNotification(DidCloseTextDocumentNotification.type, {
			textDocument: { uri },
		});

		this.openFiles.delete(uri);
		this.diagnosticsCache.delete(filePath);

		// 清除空闲计时器
		const timer = this.idleTimers.get(uri);
		if (timer) {
			clearTimeout(timer);
			this.idleTimers.delete(uri);
		}
	}

	/**
	 * 获取文件诊断信息（从 push 缓存读取）
	 */
	getDiagnostics(filePath: string): Diagnostic[] {
		return this.diagnosticsCache.get(filePath) ?? [];
	}

	/**
	 * 获取定义位置
	 */
	async getDefinition(filePath: string, line: number, character: number, signal?: AbortSignal): Promise<Location[]> {
		this._ensureReady();
		this._touchAccess(filePath);

		const result = await this._sendRequest(
			DefinitionRequest.type,
			{
				textDocument: { uri: fileToUri(filePath) },
				position: { line, character },
			},
			signal,
		);

		if (!result) return [];
		// 结果可能是 Location | Location[] | LocationLink[]
		if (Array.isArray(result)) {
			return result.map((item: any) => {
				if ("targetUri" in item) {
					return { uri: item.targetUri, range: item.targetRange };
				}
				return item as Location;
			});
		}
		return [result as Location];
	}

	/**
	 * 获取引用位置
	 */
	async getReferences(filePath: string, line: number, character: number, signal?: AbortSignal): Promise<Location[]> {
		this._ensureReady();
		this._touchAccess(filePath);

		const result = await this._sendRequest(
			ReferencesRequest.type,
			{
				textDocument: { uri: fileToUri(filePath) },
				position: { line, character },
				context: { includeDeclaration: true },
			},
			signal,
		);

		return (result as Location[]) ?? [];
	}

	/**
	 * 获取悬停信息
	 */
	async getHover(filePath: string, line: number, character: number, signal?: AbortSignal): Promise<string | null> {
		this._ensureReady();
		this._touchAccess(filePath);

		const result = await this._sendRequest(
			HoverRequest.type,
			{
				textDocument: { uri: fileToUri(filePath) },
				position: { line, character },
			},
			signal,
		);

		if (!result || !result.contents) return null;

		// 解析 MarkupContent | MarkedString | MarkedString[]
		const contents = result.contents;
		if (typeof contents === "string") return contents;
		if ("value" in contents) return contents.value;
		if (Array.isArray(contents)) {
			return contents.map((c: any) => (typeof c === "string" ? c : c.value)).join("\n\n");
		}
		return null;
	}

	/**
	 * 获取签名帮助
	 */
	async getSignatureHelp(
		filePath: string,
		line: number,
		character: number,
		signal?: AbortSignal,
	): Promise<string | null> {
		this._ensureReady();
		this._touchAccess(filePath);

		const result = await this._sendRequest(
			SignatureHelpRequest.type,
			{
				textDocument: { uri: fileToUri(filePath) },
				position: { line, character },
			},
			signal,
		);

		if (!result || !result.signatures || result.signatures.length === 0) return null;

		const activeSignature = result.signatures[result.activeSignature ?? 0];
		if (!activeSignature) return null;

		let text = activeSignature.label;
		if (activeSignature.documentation) {
			const doc = activeSignature.documentation;
			const docText = typeof doc === "string" ? doc : doc.value;
			text += `\n\n${docText}`;
		}
		return text;
	}

	/**
	 * 获取文档符号
	 */
	async getDocumentSymbols(filePath: string, signal?: AbortSignal): Promise<DocumentSymbol[]> {
		this._ensureReady();
		this._touchAccess(filePath);

		const result = await this._sendRequest(
			DocumentSymbolRequest.type,
			{
				textDocument: { uri: fileToUri(filePath) },
			},
			signal,
		);

		if (!result) return [];

		// 结果可能是 DocumentSymbol[] 或 SymbolInformation[]
		// 尝试统一转换为 DocumentSymbol 形式
		return result as DocumentSymbol[];
	}

	/**
	 * 重命名符号
	 */
	async rename(
		filePath: string,
		line: number,
		character: number,
		newName: string,
		signal?: AbortSignal,
	): Promise<WorkspaceEdit | null> {
		this._ensureReady();
		this._touchAccess(filePath);

		const result = await this._sendRequest(
			RenameRequest.type,
			{
				textDocument: { uri: fileToUri(filePath) },
				position: { line, character },
				newName,
			},
			signal,
		);

		return result ?? null;
	}

	/**
	 * 获取代码操作
	 */
	async getCodeActions(
		filePath: string,
		line: number,
		character: number,
		signal?: AbortSignal,
	): Promise<CodeAction[]> {
		this._ensureReady();
		this._touchAccess(filePath);

		const diagnostics = this.diagnosticsCache.get(filePath) ?? [];
		// 筛选光标行相关的诊断
		const relevantDiagnostics = diagnostics.filter((d) => d.range.start.line <= line && d.range.end.line >= line);

		const result = await this._sendRequest(
			CodeActionRequest.type,
			{
				textDocument: { uri: fileToUri(filePath) },
				range: {
					start: { line, character },
					end: { line, character },
				},
				context: { diagnostics: relevantDiagnostics },
			},
			signal,
		);

		if (!result) return [];
		// 结果可能包含 Command 和 CodeAction 混合
		return result.filter((item: any) => "kind" in item || "edit" in item) as CodeAction[];
	}

	/**
	 * 优雅关闭连接
	 */
	async shutdown(): Promise<void> {
		if (this.state === "shutdown") return;

		// 清理所有空闲计时器
		for (const timer of this.idleTimers.values()) {
			clearTimeout(timer);
		}
		this.idleTimers.clear();

		if (this.connection && (this.state === "ready" || this.state === "initializing")) {
			try {
				await this.connection.sendRequest(ShutdownRequest.type);
				this.connection.sendNotification(ExitNotification.type);
			} catch {
				// 忽略关闭时的错误
			}
			this.connection.dispose();
		}

		// 确保进程终止
		if (this.process && !this.process.killed) {
			this.process.kill("SIGTERM");
			// 给进程 3 秒优雅退出，超时则强制终止
			setTimeout(() => {
				if (this.process && !this.process.killed) {
					this.process.kill("SIGKILL");
				}
			}, 3000);
		}

		this.state = "shutdown";
		this.openFiles.clear();
		this.diagnosticsCache.clear();
		this.connection = null;
	}

	// ---- 内部辅助方法 ----

	private _ensureReady(): void {
		if (this.state !== "ready" || !this.connection) {
			throw new Error(`LSP connection [${this.serverName}] not ready (state: ${this.state})`);
		}
	}

	/** 更新文件访问时间 */
	private _touchAccess(filePath: string): void {
		const uri = fileToUri(filePath);
		const entry = this.openFiles.get(uri);
		if (entry) {
			entry.lastAccessTime = Date.now();
			this._resetIdleTimer(uri, filePath);
		}
	}

	/** 发送 LSP 请求，支持 AbortSignal */
	private async _sendRequest(type: any, params: any, signal?: AbortSignal): Promise<any> {
		if (!this.connection) return null;

		if (signal?.aborted) {
			throw new Error("Request aborted");
		}

		// 用 Promise.race 实现 abort 信号支持
		const requestPromise = this.connection.sendRequest(type, params);

		if (!signal) return requestPromise;

		return Promise.race([
			requestPromise,
			new Promise<never>((_, reject) => {
				signal.addEventListener("abort", () => reject(new Error("Request aborted")), { once: true });
			}),
		]);
	}

	/** LRU 淘汰：关闭最久未访问的文件 */
	private async _evictIfNeeded(): Promise<void> {
		if (this.openFiles.size < MAX_OPEN_FILES) return;

		// 找到最旧的文件
		let oldestUri: string | null = null;
		let oldestTime = Infinity;

		for (const [uri, entry] of this.openFiles) {
			if (entry.lastAccessTime < oldestTime) {
				oldestTime = entry.lastAccessTime;
				oldestUri = uri;
			}
		}

		if (oldestUri) {
			const filePath = uriToFile(oldestUri);
			this.closeFile(filePath);
		}
	}

	/** 重置文件空闲计时器 */
	private _resetIdleTimer(uri: string, filePath: string): void {
		const existing = this.idleTimers.get(uri);
		if (existing) {
			clearTimeout(existing);
		}

		const timer = setTimeout(() => {
			this.closeFile(filePath);
			this.idleTimers.delete(uri);
		}, FILE_IDLE_TIMEOUT);

		this.idleTimers.set(uri, timer);

		// 确保定时器不阻止进程退出
		if (timer.unref) {
			timer.unref();
		}
	}
}

// ============================================================
// LSPManager — 单例管理器
// ============================================================

/** 管理器实例缓存，按工作目录 */
const managerInstances = new Map<string, LSPManager>();

/**
 * LSPManager — 管理多个语言服务器连接的核心类
 *
 * 按 `serverId:projectRoot` 维护连接池，自动根据文件类型和项目结构
 * 选择合适的语言服务器。
 */
export class LSPManager extends EventEmitter {
	/** 工作目录 */
	readonly cwd: string;

	/** 连接池: key = "serverId:projectRoot" */
	private connections = new Map<string, LSPConnection>();

	/** 正在进行的连接创建 Promise（防止并发 spawn） */
	private pendingConnections = new Map<string, Promise<LSPConnection | null>>();

	private constructor(cwd: string) {
		super();
		this.cwd = resolve(cwd);
	}

	/**
	 * 获取或创建指定工作目录的 LSPManager 实例
	 */
	static getOrCreateManager(cwd: string): LSPManager {
		const resolvedCwd = resolve(cwd);
		let manager = managerInstances.get(resolvedCwd);
		if (!manager) {
			manager = new LSPManager(resolvedCwd);
			managerInstances.set(resolvedCwd, manager);
		}
		return manager;
	}

	/**
	 * 根据文件路径获取对应的 LSP 连接（惰性启动）
	 * @returns LSP 连接实例，无法匹配或启动则返回 null
	 */
	async getServerForFile(filePath: string, signal?: AbortSignal): Promise<LSPConnection | null> {
		const resolvedPath = resolve(filePath);
		const ext = extname(resolvedPath).toLowerCase();

		// 查找匹配的语言服务器配置
		const serverConfig = LSP_SERVERS.find((cfg) => cfg.extensions.includes(ext));
		if (!serverConfig) return null;

		// 查找项目根目录
		const projectRoot = serverConfig.findRoot(resolvedPath);
		if (!projectRoot) return null;

		const connectionKey = `${serverConfig.id}:${projectRoot}`;

		// 检查已有连接
		const existing = this.connections.get(connectionKey);
		if (existing && existing.getState() === "ready") {
			return existing;
		}

		// 如果已有连接但状态异常，清理掉
		if (existing && (existing.getState() === "broken" || existing.getState() === "shutdown")) {
			this.connections.delete(connectionKey);
		}

		// 并发 spawn 防护
		const pending = this.pendingConnections.get(connectionKey);
		if (pending) {
			return pending;
		}

		// 创建新连接
		const createPromise = this._createConnection(serverConfig, projectRoot, connectionKey, signal);
		this.pendingConnections.set(connectionKey, createPromise);

		try {
			const conn = await createPromise;
			return conn;
		} finally {
			this.pendingConnections.delete(connectionKey);
		}
	}

	/**
	 * 打开或更新文件内容（自动路由到正确的语言服务器）
	 */
	async touchFile(filePath: string, content: string, signal?: AbortSignal): Promise<void> {
		const conn = await this.getServerForFile(filePath, signal);
		if (!conn) return;
		await conn.openFile(filePath, content);
	}

	/**
	 * 关闭文件
	 */
	closeFile(filePath: string): void {
		const resolvedPath = resolve(filePath);
		const ext = extname(resolvedPath).toLowerCase();
		const serverConfig = LSP_SERVERS.find((cfg) => cfg.extensions.includes(ext));
		if (!serverConfig) return;

		const projectRoot = serverConfig.findRoot(resolvedPath);
		if (!projectRoot) return;

		const connectionKey = `${serverConfig.id}:${projectRoot}`;
		const conn = this.connections.get(connectionKey);
		if (conn) {
			conn.closeFile(resolvedPath);
		}
	}

	/**
	 * 获取文件诊断信息
	 */
	async getDiagnostics(filePath: string, signal?: AbortSignal): Promise<Diagnostic[]> {
		const conn = await this.getServerForFile(filePath, signal);
		if (!conn) return [];
		return conn.getDiagnostics(resolve(filePath));
	}

	/**
	 * 获取定义位置
	 */
	async getDefinition(filePath: string, line: number, character: number, signal?: AbortSignal): Promise<Location[]> {
		const conn = await this.getServerForFile(filePath, signal);
		if (!conn) return [];
		return conn.getDefinition(resolve(filePath), line, character, signal);
	}

	/**
	 * 获取引用位置
	 */
	async getReferences(filePath: string, line: number, character: number, signal?: AbortSignal): Promise<Location[]> {
		const conn = await this.getServerForFile(filePath, signal);
		if (!conn) return [];
		return conn.getReferences(resolve(filePath), line, character, signal);
	}

	/**
	 * 获取悬停信息
	 */
	async getHover(filePath: string, line: number, character: number, signal?: AbortSignal): Promise<string | null> {
		const conn = await this.getServerForFile(filePath, signal);
		if (!conn) return null;
		return conn.getHover(resolve(filePath), line, character, signal);
	}

	/**
	 * 获取签名帮助
	 */
	async getSignatureHelp(
		filePath: string,
		line: number,
		character: number,
		signal?: AbortSignal,
	): Promise<string | null> {
		const conn = await this.getServerForFile(filePath, signal);
		if (!conn) return null;
		return conn.getSignatureHelp(resolve(filePath), line, character, signal);
	}

	/**
	 * 获取文档符号
	 */
	async getDocumentSymbols(filePath: string, signal?: AbortSignal): Promise<DocumentSymbol[]> {
		const conn = await this.getServerForFile(filePath, signal);
		if (!conn) return [];
		return conn.getDocumentSymbols(resolve(filePath), signal);
	}

	/**
	 * 重命名符号
	 */
	async rename(
		filePath: string,
		line: number,
		character: number,
		newName: string,
		signal?: AbortSignal,
	): Promise<WorkspaceEdit | null> {
		const conn = await this.getServerForFile(filePath, signal);
		if (!conn) return null;
		return conn.rename(resolve(filePath), line, character, newName, signal);
	}

	/**
	 * 获取代码操作
	 */
	async getCodeActions(
		filePath: string,
		line: number,
		character: number,
		signal?: AbortSignal,
	): Promise<CodeAction[]> {
		const conn = await this.getServerForFile(filePath, signal);
		if (!conn) return [];
		return conn.getCodeActions(resolve(filePath), line, character, signal);
	}

	/**
	 * 优雅关闭所有连接
	 */
	async shutdownAll(): Promise<void> {
		const shutdownPromises: Promise<void>[] = [];

		for (const conn of this.connections.values()) {
			shutdownPromises.push(conn.shutdown());
		}

		await Promise.allSettled(shutdownPromises);
		this.connections.clear();
		this.pendingConnections.clear();

		// 从全局缓存中移除
		managerInstances.delete(this.cwd);
	}

	// ---- 内部辅助方法 ----

	/**
	 * 创建并初始化一个新的 LSP 连接
	 */
	private async _createConnection(
		config: LSPServerConfig,
		projectRoot: string,
		connectionKey: string,
		signal?: AbortSignal,
	): Promise<LSPConnection | null> {
		// 启动语言服务器进程
		const childProcess = config.spawn(projectRoot);
		if (!childProcess) {
			// 二进制不存在，静默返回 null
			return null;
		}

		const conn = new LSPConnection({
			serverId: config.id,
			serverName: config.name,
			projectRoot,
			process: childProcess,
		});

		// 转发诊断事件
		conn.on("diagnostics", (filePath: string, diagnostics: Diagnostic[]) => {
			this.emit("diagnostics", filePath, diagnostics);
		});

		try {
			await conn.initialize(signal);
			this.connections.set(connectionKey, conn);
			return conn;
		} catch {
			// 初始化失败，清理资源
			await conn.shutdown().catch(() => {});
			return null;
		}
	}
}
