import { type ChildProcess, spawn } from "node:child_process";
import { randomUUID } from "node:crypto";
import { existsSync, mkdirSync, unlinkSync } from "node:fs";
import { createServer, type Server, type Socket } from "node:net";
import { join, resolve } from "node:path";
import type { AgentMessage } from "@earendil-works/pi-agent-core";
import type { ImageContent } from "@earendil-works/pi-model-types";
import { APP_NAME, VERSION } from "../../config.ts";
import type { AgentSessionRuntime } from "../../core/agent-session-runtime.ts";
import type {
	AutocompleteProviderFactory,
	ExtensionUIContext,
	ExtensionUIDialogOptions,
	ExtensionWidgetOptions,
	WorkingIndicatorOptions,
} from "../../core/extensions/index.ts";
import { findExactModelReferenceMatch } from "../../core/model-resolver.ts";
import { SessionManager } from "../../core/session-manager.ts";
import { ensureTool } from "../../utils/tools-manager.ts";
import {
	getAvailableThemesWithPaths,
	getThemeByName,
	setRegisteredThemes,
	setTheme,
	theme,
} from "../interactive/theme/theme.ts";
import { getNativeAutocomplete } from "./native-autocomplete.ts";
import { handleNativeBuiltinCommand } from "./native-builtins.ts";
import { linesRecord, parseNativeLine, resolveNativeCommand, stringRecord } from "./native-command.ts";
import { expandNativeFileReferences } from "./native-file-attachments.ts";
import { graphMessageFromEntries } from "./native-graph.ts";
import { loadNativeKeybindings } from "./native-keybindings.ts";
import { type PendingPermission, requestNativePermission, resolveNativePermission } from "./native-permission.ts";
import type { HostToNativeMessage, NativeToHostMessage, NativeUiState } from "./protocol.ts";

export interface NativeModeOptions {
	initialMessage?: string;
	initialImages?: ImageContent[];
	initialMessages?: string[];
	modelFallbackMessage?: string;
	binaryPath?: string;
	hostSocketPath?: string;
}

interface PendingDialog<T> {
	resolve: (value: T) => void;
	defaultValue: T;
}

export class NativeModeUnavailableError extends Error {
	constructor(message: string) {
		super(message);
		this.name = "NativeModeUnavailableError";
	}
}

export function nativeMessagesWithStreaming(
	messages: readonly AgentMessage[],
	streamingMessage?: AgentMessage,
): AgentMessage[] {
	return streamingMessage ? [...messages, streamingMessage] : [...messages];
}

export class NativeMode {
	private readonly runtimeHost: AgentSessionRuntime;
	private readonly options: NativeModeOptions;
	private child?: ChildProcess;
	private server?: Server;
	private socket?: Socket;
	private unsubscribe?: () => void;
	private disposed = false;
	private receiveBuffer = "";
	private readonly pendingDialogs = new Map<string, PendingDialog<unknown>>();
	private readonly pendingPermissions = new Map<string, PendingPermission>();
	private readonly status = new Map<string, string>();
	private readonly widgetsAbove = new Map<string, string[]>();
	private readonly widgetsBelow = new Map<string, string[]>();
	private readonly autocompleteProviderFactories: AutocompleteProviderFactory[] = [];
	private keybindings = loadNativeKeybindings();
	private activeSubagentId: string | undefined;
	private editorText = "";
	private fdPath: string | undefined;

	constructor(runtimeHost: AgentSessionRuntime, options: NativeModeOptions = {}) {
		this.runtimeHost = runtimeHost;
		this.options = options;
		this.runtimeHost.setRebindSession(async () => {
			await this.bindSession();
			this.sendState();
		});
		this.runtimeHost.setBeforeSessionInvalidate(() => {
			this.resetExtensionUi();
		});
		setRegisteredThemes(this.session.resourceLoader.getThemes().themes);
	}

	private get session() {
		return this.runtimeHost.session;
	}

	async run(): Promise<void> {
		await this.startProtocolServer();
		await this.bindSession();
		this.populateStartupResources();
		// 初始化 fd 工具（用于 @ 文件补全）
		this.initFd();
		this.sendState();
		if (this.options.modelFallbackMessage) {
			this.send({ type: "notify", level: "warning", message: this.options.modelFallbackMessage });
		}
		if (this.options.initialMessage) {
			await this.session.prompt(this.options.initialMessage, { images: this.options.initialImages });
		}
		for (const message of this.options.initialMessages ?? []) {
			await this.session.prompt(message);
		}
		await new Promise<void>((resolvePromise) => {
			this.child?.once("exit", () => resolvePromise());
		});
	}

	private initFd(): void {
		void ensureTool("fd").then((path) => {
			if (path) this.fdPath = path;
		});
	}

	stop(): void {
		void this.dispose();
	}

	private async startProtocolServer(): Promise<void> {
		const socketPath = this.options.hostSocketPath ?? this.createSocketPath();
		if (existsSync(socketPath)) unlinkSync(socketPath);

		this.server = createServer((socket) => {
			this.socket = socket;
			socket.setEncoding("utf8");
			socket.on("data", (chunk) => this.handleSocketData(String(chunk)));
			socket.once("close", () => {
				if (!this.disposed) void this.dispose();
			});
		});

		await new Promise<void>((resolvePromise, reject) => {
			this.server?.once("error", reject);
			this.server?.listen(socketPath, resolvePromise);
		});

		let childStartError: Error | undefined;
		if (!this.options.hostSocketPath) {
			const nativeCommand = this.options.binaryPath
				? { command: this.options.binaryPath, args: [] }
				: resolveNativeCommand();
			if (!existsSync(nativeCommand.command) && nativeCommand.command !== "cargo") {
				throw new NativeModeUnavailableError(`Native TUI binary not found: ${nativeCommand.command}`);
			}

			this.child = spawn(nativeCommand.command, nativeCommand.args, {
				stdio: "inherit",
				env: {
					...process.env,
					ROZSA_NATIVE_TUI_SOCKET: socketPath,
				},
			});
			this.child.once("exit", () => {
				if (existsSync(socketPath)) unlinkSync(socketPath);
				this.server?.close();
			});
			this.child.once("error", (error) => {
				childStartError = error;
			});
		}

		await new Promise<void>((resolvePromise, reject) => {
			const startedAt = Date.now();
			const waitForSocket = () => {
				if (this.socket) {
					resolvePromise();
					return;
				}
				if (childStartError) {
					reject(new NativeModeUnavailableError(childStartError.message));
					return;
				}
				if (this.child && this.child.exitCode !== null) {
					reject(new NativeModeUnavailableError(`Native TUI exited before connecting: ${this.child?.exitCode}`));
					return;
				}
				if (Date.now() - startedAt > 10_000) {
					reject(new NativeModeUnavailableError("Timed out waiting for native TUI to connect"));
					return;
				}
				setTimeout(waitForSocket, 10);
			};
			waitForSocket();
		});
	}

	private createSocketPath(): string {
		const tempDir = resolve(this.runtimeHost.cwd, "temp");
		mkdirSync(tempDir, { recursive: true });
		return join(tempDir, `rozsa-native-tui-${process.pid}-${Date.now()}.sock`);
	}

	private handleSocketData(chunk: string): void {
		this.receiveBuffer += chunk;
		while (true) {
			const newline = this.receiveBuffer.indexOf("\n");
			if (newline === -1) break;
			const line = this.receiveBuffer.slice(0, newline);
			this.receiveBuffer = this.receiveBuffer.slice(newline + 1);
			const message = parseNativeLine(line);
			if (message) void this.handleNativeMessage(message);
		}
	}

	private send(message: HostToNativeMessage): void {
		this.socket?.write(`${JSON.stringify(message)}\n`);
	}

	private sendState(error?: string): void {
		const subagentSnapshot = this.activeSubagentId
			? this.session.getSubagentSnapshot(this.activeSubagentId)
			: undefined;
		const messages = nativeMessagesWithStreaming(
			subagentSnapshot?.messages ?? this.session.state.messages,
			subagentSnapshot?.streamingMessage ??
				(this.activeSubagentId ? undefined : this.session.state.streamingMessage),
		);
		const state: NativeUiState = {
			appName: APP_NAME,
			version: VERSION,
			cwd: this.session.sessionManager.getCwd(),
			sessionName: this.session.sessionName,
			model: this.session.model,
			thinkingLevel: this.session.thinkingLevel,
			isStreaming: this.activeSubagentId
				? this.session.isSubagentStreaming(this.activeSubagentId)
				: this.session.isStreaming,
			isCompacting: this.session.isCompacting,
			hideThinking: this.session.settingsManager.getHideThinkingBlock(),
			showImages: this.session.settingsManager.getShowImages(),
			messages,
			pendingMessages: [...this.session.getSteeringMessages(), ...this.session.getFollowUpMessages()],
			status: stringRecord(this.status),
			widgetsAbove: linesRecord(this.widgetsAbove),
			widgetsBelow: linesRecord(this.widgetsBelow),
			stats: this.session.getSessionStats(),
			runtimeState: this.session.runtimeState.getSnapshot(),
			contextUsage: this.session.getContextUsage(),
			keybindings: this.keybindings,
			error,
		};
		this.send({ type: "state", state });
	}

	private async bindSession(): Promise<void> {
		this.session.permissionPromptOverride = (request, context) =>
			requestNativePermission((message) => this.send(message), this.pendingPermissions, request, context);
		await this.session.bindExtensions({
			uiContext: this.createExtensionUiContext(),
			commandContextActions: {
				waitForIdle: () => this.session.agent.waitForIdle(),
				newSession: (options) => this.runtimeHost.newSession(options),
				fork: (entryId, options) => this.runtimeHost.fork(entryId, options),
				navigateTree: (targetId, options) =>
					this.session.navigateTree(targetId, {
						summarize: options?.summarize,
						customInstructions: options?.customInstructions,
						replaceInstructions: options?.replaceInstructions,
						label: options?.label,
					}),
				switchSession: (sessionPath, options) => this.runtimeHost.switchSession(sessionPath, options),
				reload: () => this.session.reload(),
			},
			abortHandler: () => void this.session.abort(),
			shutdownHandler: () => void this.dispose(),
			onError: (error) => {
				this.send({
					type: "notify",
					level: "error",
					message: `Extension error (${error.extensionPath}): ${error.error}`,
				});
			},
		});

		this.unsubscribe?.();
		this.unsubscribe = this.session.subscribe(() => {
			this.sendState();
		});
	}

	private async handleNativeMessage(message: NativeToHostMessage): Promise<void> {
		try {
			switch (message.type) {
				case "submit":
					await this.submit(message.text, message.images);
					break;
				case "autocomplete_request":
					this.send({
						type: "autocomplete",
						id: message.id,
						...(await getNativeAutocomplete(
							this.session,
							this.autocompleteProviderFactories,
							message,
							this.fdPath,
						)),
					});
					break;
				case "follow_up":
					if (this.activeSubagentId) {
						await this.session.sendSubagentPrompt(this.activeSubagentId, message.text, { deliverAs: "followUp" });
					} else {
						await this.session.followUp(message.text, message.images);
					}
					break;
				case "steer":
					await this.session.steer(message.text, message.images);
					break;
				case "bash":
					await this.session.executeBash(message.command, undefined, {
						permissionAlreadyChecked: true,
						userInitiated: true,
					});
					break;
				case "abort":
					if (this.activeSubagentId && this.session.isSubagentStreaming(this.activeSubagentId)) {
						await this.session.abortSubagent(this.activeSubagentId);
					} else {
						await this.session.abort();
					}
					break;
				case "compact":
					await this.session.compact();
					break;
				case "cycle_model":
					await this.session.cycleModel(message.direction);
					break;
				case "cycle_thinking":
					this.session.cycleThinkingLevel();
					break;
				case "cycle_edit_mode":
					this.session.cycleEditMode();
					break;
				case "dialog_response":
					this.resolveDialog(message);
					break;
				case "permission_response":
					resolveNativePermission(this.pendingPermissions, message);
					break;
				case "switch_agent":
					if (
						"switchSubagentView" in this.session &&
						typeof (this.session as any).switchSubagentView === "function"
					) {
						await (this.session as any).switchSubagentView(message.id);
					}
					break;
				case "switch_model": {
					const allModels = this.session.modelRegistry.getAll();
					const target = message.provider
						? allModels.find((m) => m.provider === message.provider && m.id === message.id)
						: findExactModelReferenceMatch(message.id, allModels);
					if (!target) {
						const reference = message.provider ? `${message.provider}/${message.id}` : message.id;
						this.send({
							type: "notify",
							level: "warning",
							message: `Model not found or ambiguous: ${reference}`,
						});
						break;
					}
					await this.session.setModel(target);
					break;
				}
				case "switch_session":
					await this.runtimeHost.switchSession(message.path);
					break;
				case "delete_session": {
					const delPath = message.path;
					let delMethod: "trash" | "unlink" = "unlink";
					let delError: string | undefined;
					try {
						const { spawnSync } = await import("node:child_process");
						const trashArgs = delPath.startsWith("-") ? ["--", delPath] : [delPath];
						const trashResult = spawnSync("trash", trashArgs, { encoding: "utf-8" });
						if (trashResult.status === 0 || !existsSync(delPath)) {
							delMethod = "trash";
						} else if (existsSync(delPath)) {
							unlinkSync(delPath);
							delMethod = "unlink";
						}
					} catch (err) {
						delError = err instanceof Error ? err.message : String(err);
					}
					this.send({ type: "session_deleted", path: delPath, method: delMethod, error: delError });
					break;
				}
				case "rename_session": {
					const mgr = SessionManager.open(message.path);
					mgr.appendSessionInfo(message.name);
					break;
				}
				case "list_sessions": {
					await this.handleListSessions(message.scope ?? "current");
					break;
				}
				case "list_models": {
					const models = this.session.modelRegistry.getAvailable();
					const currentModel = this.session.model;
					const entries = models.map((m) => ({
						id: m.id,
						provider: m.provider,
						is_current: currentModel ? m.id === currentModel.id && m.provider === currentModel.provider : false,
					}));
					this.send({ type: "models", entries });
					break;
				}
				case "exit":
					await this.dispose();
					break;
			}
			this.sendState();
		} catch (error) {
			this.sendState(error instanceof Error ? error.message : String(error));
		}
	}

	private async submit(text: string, images?: ImageContent[]): Promise<void> {
		const trimmed = text.trim();
		this.editorText = "";
		if (!trimmed) {
			await this.dispose();
			return;
		}
		if (trimmed.startsWith("!")) {
			const excludeFromContext = trimmed.startsWith("!!");
			const command = trimmed.slice(excludeFromContext ? 2 : 1).trim();
			if (command)
				await this.session.executeBash(command, undefined, {
					excludeFromContext,
					permissionAlreadyChecked: true,
					userInitiated: true,
				});
			return;
		}
		if (trimmed === "/graph") {
			this.send(graphMessageFromEntries(this.session.sessionManager.getEntries()));
			return;
		}
		if (
			await handleNativeBuiltinCommand(trimmed, {
				session: this.session,
				runtimeHost: this.runtimeHost,
				keybindings: this.keybindings,
				notify: (message, level = "info") => this.send({ type: "notify", level, message }),
				select: (title, options, selectedIndex) =>
					this.createDialog<string | undefined>("select", undefined, { title, options, selected: selectedIndex }),
				listSessions: (scope) => {
					void this.handleListSessions(scope);
				},
				listModels: () => {
					this.session.modelRegistry.refresh();
					const models = this.session.modelRegistry.getAvailable();
					const currentModel = this.session.model;
					const entries = models.map((m) => ({
						id: m.id,
						provider: m.provider,
						is_current: currentModel ? m.id === currentModel.id && m.provider === currentModel.provider : false,
					}));
					this.send({ type: "models", entries });
				},
				setInput: (nextText) => {
					this.editorText = nextText;
					this.send({ type: "set_input", text: nextText });
				},
				setActiveSubagent: (id) => {
					this.activeSubagentId = id;
					this.session.runtimeState.setViewingSubagent(id);
				},
				activeSubagentId: () => this.activeSubagentId,
				dispose: () => this.dispose(),
			})
		) {
			return;
		}
		const expanded = await expandNativeFileReferences(trimmed, {
			cwd: this.session.sessionManager.getCwd(),
			autoResizeImages: this.session.settingsManager.getImageAutoResize(),
		});
		const nextImages = [...(images ?? []), ...(expanded.images ?? [])];
		if (this.activeSubagentId) {
			await this.session.sendSubagentPrompt(this.activeSubagentId, expanded.text, { deliverAs: "steer" });
			return;
		}
		if (this.session.isStreaming) {
			await this.session.prompt(expanded.text, {
				images: nextImages.length > 0 ? nextImages : undefined,
				streamingBehavior: "steer",
			});
		} else {
			await this.session.prompt(expanded.text, { images: nextImages.length > 0 ? nextImages : undefined });
		}
	}

	private async handleListSessions(scope: string): Promise<void> {
		const sessions =
			scope === "all" ? await SessionManager.listAll() : await SessionManager.list(this.runtimeHost.cwd);
		const maxTextLen = 4096;
		const entries = sessions.slice(0, 50).map((s) => ({
			path: s.path,
			name: s.name || undefined,
			firstMessage: s.firstMessage.slice(0, 200),
			cwd: s.cwd,
			messageCount: s.messageCount,
			lastModified: s.modified.toISOString(),
			parentSessionPath: s.parentSessionPath || undefined,
			allMessagesText: s.allMessagesText.slice(0, maxTextLen),
		}));
		const currentSessionPath = this.session.sessionManager.getSessionFile() ?? "";
		this.send({ type: "sessions", entries, currentSessionPath });
	}

	private async dispose(): Promise<void> {
		if (this.disposed) return;
		this.disposed = true;
		this.unsubscribe?.();
		this.send({ type: "shutdown" });
		this.socket?.destroy();
		this.server?.close();
		this.child?.kill();
		await this.runtimeHost.dispose();
	}

	private createDialog<T>(
		kind: "select" | "confirm" | "input" | "editor",
		defaultValue: T,
		payload: { title: string; message?: string; options?: string[]; text?: string; selected?: number },
		opts?: ExtensionUIDialogOptions,
	): Promise<T> {
		if (opts?.signal?.aborted) return Promise.resolve(defaultValue);
		const id = randomUUID();
		return new Promise<T>((resolvePromise) => {
			let timeout: ReturnType<typeof setTimeout> | undefined;
			const finish = (value: T) => {
				if (timeout) clearTimeout(timeout);
				opts?.signal?.removeEventListener("abort", onAbort);
				resolvePromise(value);
			};
			const onAbort = () => {
				this.pendingDialogs.delete(id);
				finish(defaultValue);
			};
			opts?.signal?.addEventListener("abort", onAbort, { once: true });
			if (opts?.timeout) timeout = setTimeout(onAbort, opts.timeout);
			this.pendingDialogs.set(id, { resolve: (value) => finish(value as T), defaultValue });
			this.send({ type: "dialog", id, kind, ...payload });
		});
	}

	private resolveDialog(message: Extract<NativeToHostMessage, { type: "dialog_response" }>): void {
		const pending = this.pendingDialogs.get(message.id);
		if (!pending) return;
		this.pendingDialogs.delete(message.id);
		if (message.cancelled) {
			pending.resolve(pending.defaultValue);
		} else if (message.value !== undefined) {
			pending.resolve(message.value);
		} else if (message.confirmed !== undefined) {
			pending.resolve(message.confirmed);
		} else {
			pending.resolve(pending.defaultValue);
		}
	}

	private resetExtensionUi(): void {
		this.status.clear();
		this.widgetsAbove.clear();
		this.widgetsBelow.clear();
	}

	private populateStartupResources(): void {
		const lines: string[] = [];
		const loader = this.session.resourceLoader;

		const contextFiles = loader.getAgentsFiles().agentsFiles;
		if (contextFiles.length > 0) {
			lines.push("[Context]");
			lines.push(`  ${contextFiles.map((f) => f.path.split("/").pop() ?? f.path).join(", ")}`);
		}

		const skills = loader.getSkills().skills;
		if (skills.length > 0) {
			lines.push("[Skills]");
			lines.push(`  ${skills.map((s) => s.name).join(", ")}`);
		}

		const prompts = loader.getPrompts().prompts;
		if (prompts.length > 0) {
			lines.push("[Prompts]");
			lines.push(`  ${prompts.map((p) => `/${p.name}`).join(", ")}`);
		}

		const extensions = loader.getExtensions().extensions;
		if (extensions.length > 0) {
			lines.push("[Extensions]");
			lines.push(`  ${extensions.map((e) => e.path.split("/").pop() ?? e.path).join(", ")}`);
		}

		if (lines.length > 0) {
			this.widgetsAbove.set("__startup", lines);
		}
	}

	private createExtensionUiContext(): ExtensionUIContext {
		return {
			select: (title, options, opts) =>
				this.createDialog<string | undefined>("select", undefined, { title, options }, opts),
			confirm: (title, message, opts) => this.createDialog<boolean>("confirm", false, { title, message }, opts),
			input: (title, placeholder, opts) =>
				this.createDialog<string | undefined>("input", undefined, { title, text: placeholder }, opts),
			notify: (message, level = "info") => this.send({ type: "notify", level, message }),
			onTerminalInput: () => () => {},
			setStatus: (key, text) => {
				if (text === undefined) this.status.delete(key);
				else this.status.set(key, text);
				this.sendState();
			},
			setWorkingMessage: (message) => {
				if (message) this.status.set("working", message);
				else this.status.delete("working");
				this.sendState();
			},
			setWorkingVisible: () => {},
			setWorkingIndicator: (_options?: WorkingIndicatorOptions) => {},
			setHiddenThinkingLabel: () => {},
			setWidget: (key, content, options?: ExtensionWidgetOptions) => {
				const target = options?.placement === "belowEditor" ? this.widgetsBelow : this.widgetsAbove;
				if (content === undefined) target.delete(key);
				else if (Array.isArray(content)) target.set(key, content);
				this.sendState();
			},
			setFooter: () => {},
			setHeader: () => {},
			setTitle: (title) => this.send({ type: "set_title", title }),
			custom: async () => undefined as never,
			pasteToEditor: (text) => {
				this.editorText += text;
				this.send({ type: "set_input", text: this.editorText });
			},
			setEditorText: (text) => {
				this.editorText = text;
				this.send({ type: "set_input", text });
			},
			getEditorText: () => this.editorText,
			editor: (title, prefill) =>
				this.createDialog<string | undefined>("editor", undefined, { title, text: prefill }),
			addAutocompleteProvider: (factory) => {
				this.autocompleteProviderFactories.push(factory);
			},
			setEditorComponent: () => {},
			getEditorComponent: () => undefined,
			theme,
			getAllThemes: () => getAvailableThemesWithPaths(),
			getTheme: (name) => getThemeByName(name),
			setTheme: (nextTheme) => {
				try {
					const themeName = typeof nextTheme === "string" ? nextTheme : nextTheme.name;
					if (!themeName) {
						return { success: false, error: "Anonymous Theme instances are not supported by the native TUI" };
					}
					setTheme(themeName);
					return { success: true };
				} catch (error) {
					return { success: false, error: error instanceof Error ? error.message : String(error) };
				}
			},
			getToolsExpanded: () => false,
			setToolsExpanded: () => {},
		};
	}
}
