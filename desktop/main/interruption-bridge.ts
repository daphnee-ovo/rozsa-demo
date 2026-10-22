import type { BrowserWindow, IpcMain, IpcMainInvokeEvent } from "electron";
import type { BackendService } from "./backend-service.js";
import {
  INTERRUPTION_STATE_CHANNEL,
  PERMISSION_RESPOND_CHANNEL,
  USER_QUESTION_CANCEL_CHANNEL,
  USER_QUESTION_RESPOND_CHANNEL,
  bridgeFault,
  transportFailure,
  type PermissionResponse,
  type QuestionAnswers,
} from "./session-contract.js";

export function installInterruptionBridge(window: BrowserWindow, ipc: IpcMain, backend: BackendService, trustedURL: string): void {
  let generation = 0;
  let disposed = false;
  const channels = [INTERRUPTION_STATE_CHANNEL, PERMISSION_RESPOND_CHANNEL, USER_QUESTION_RESPOND_CHANNEL, USER_QUESTION_CANCEL_CHANNEL];

  const handle = (channel: string, count: number, operation: (...args: unknown[]) => Promise<Record<string, unknown>>): void => {
    ipc.handle(channel, async (event: IpcMainInvokeEvent, ...args: unknown[]) => {
      if (disposed || window.isDestroyed() || event.sender !== window.webContents || event.senderFrame !== window.webContents.mainFrame
        || event.senderFrame?.url !== trustedURL || args.length !== count) throw new Error("Unauthorized interruption request");
      const started = generation;
      try {
        const result = await operation(...args);
        if (started !== generation) throw transportFailure("closed", "Interruption page was replaced");
        return { ok: true, ...result };
      } catch (error) {
        return { ok: false, error: bridgeFault(error) };
      }
    });
  };

  handle(INTERRUPTION_STATE_CHANNEL, 1, async sessionId => {
    if (!validIdentity(sessionId)) throw new Error("Invalid interruption session");
    return { state: await backend.getInterruptionState(sessionId) };
  });
  handle(PERMISSION_RESPOND_CHANNEL, 1, async value => {
    const response = permissionResponse(value);
    await backend.respondPermission(response);
    return {};
  });
  handle(USER_QUESTION_RESPOND_CHANNEL, 2, async (requestId, value) => {
    if (!validIdentity(requestId)) throw new Error("Invalid user question request");
    const answers = questionAnswers(value);
    await backend.respondUserQuestion(requestId, answers);
    return {};
  });
  handle(USER_QUESTION_CANCEL_CHANNEL, 1, async requestId => {
    if (!validIdentity(requestId)) throw new Error("Invalid user question request");
    await backend.cancelUserQuestion(requestId);
    return {};
  });

  const invalidate = (): void => { generation++; };
  window.webContents.on("did-start-navigation", (_event, _url, _inPlace, mainFrame) => { if (mainFrame) invalidate(); });
  window.webContents.on("render-process-gone", invalidate);
  window.once("closed", () => {
    disposed = true;
    invalidate();
    for (const channel of channels) ipc.removeHandler(channel);
  });
}

function validIdentity(value: unknown): value is string {
  return typeof value === "string" && value.trim() !== "" && value.trim() === value;
}

function permissionResponse(value: unknown): PermissionResponse {
  if (!isRecord(value) || !validIdentity(value.sessionId) || !validIdentity(value.requestId)
    || !["allow", "allow-once", "allow-session", "allow-project", "deny", "deny-hint"].includes(value.choice as string)
    || (value.trustKey !== undefined && typeof value.trustKey !== "string")
    || (value.trustKeys !== undefined && (!Array.isArray(value.trustKeys) || !value.trustKeys.every(key => typeof key === "string")))
    || (value.hint !== undefined && typeof value.hint !== "string")) throw new Error("Invalid permission response");
  return value as unknown as PermissionResponse;
}

function questionAnswers(value: unknown): QuestionAnswers {
  if (!isRecord(value) || Object.keys(value).length === 0 || !Object.values(value).every(answer => typeof answer === "string"
    || Array.isArray(answer) && answer.every(item => typeof item === "string"))) throw new Error("Invalid user question answers");
  return value as QuestionAnswers;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
