import { randomUUID } from "node:crypto";
import {
	generateTrustLevels,
	type PermissionPromptContext,
	type PermissionRequest,
	type UserPermissionChoice,
} from "../../core/permissions.ts";
import type { HostToNativeMessage, NativePermissionPrompt, NativeToHostMessage } from "./protocol.ts";

export interface PendingPermission {
	resolve: (value: { choice: UserPermissionChoice; reason?: string; trustKey?: string }) => void;
	request: PermissionRequest;
}

export function createNativePermissionPrompt(
	id: string,
	request: PermissionRequest,
	context: PermissionPromptContext,
): NativePermissionPrompt {
	return {
		id,
		request,
		context,
		trustLevels: generateTrustLevels(request),
	};
}

export function permissionMessage(prompt: NativePermissionPrompt): HostToNativeMessage {
	return { type: "permission", prompt };
}

export function requestNativePermission(
	send: (message: HostToNativeMessage) => void,
	pending: Map<string, PendingPermission>,
	request: PermissionRequest,
	context: PermissionPromptContext,
): Promise<{ choice: UserPermissionChoice; reason?: string; trustKey?: string }> {
	const id = randomUUID();
	return new Promise((resolve) => {
		pending.set(id, { resolve, request });
		send(permissionMessage(createNativePermissionPrompt(id, request, context)));
	});
}

export function resolveNativePermission(
	pending: Map<string, PendingPermission>,
	message: Extract<NativeToHostMessage, { type: "permission_response" }>,
): void {
	const permission = pending.get(message.id);
	if (!permission) return;
	pending.delete(message.id);
	if (message.choice === "reject_alternative") {
		permission.resolve({
			choice: message.choice,
			reason: getAlternativeHint(permission.request),
			trustKey: message.trustKey,
		});
	} else {
		permission.resolve({ choice: message.choice, trustKey: message.trustKey });
	}
}

function getAlternativeHint(request: PermissionRequest): string {
	const tool = request.toolName;
	const cmd = request.command ?? "";
	if (tool === "bash" || tool === "shell") {
		if (/\brm\b/.test(cmd)) return "不要删除文件，请用 read 先确认内容，或移动到临时目录";
		if (/\bgit\s+(push|reset|checkout\s+\.)/.test(cmd)) return "不要执行破坏性 git 操作，请用更安全的 git 命令";
		return "请使用更安全的命令，或拆分为只读操作先确认再执行";
	}
	if (tool === "write" || tool === "edit") return "请先用 read 查看文件当前内容，确认修改范围后再操作";
	return "请选择更安全的替代方案";
}
