---
source: test
nums: 1
---

- [x] ISSUE-I023：Test TASK-T102 fail:node:internal/modules/run_main:111
  - severity: P1
  - location：desktop/tests/notification-center.test.js
  - description：node:internal/modules/run_main:111
        triggerUncaughtException(
        ^
    
    file:///Users/xinyue/ballad/rozsa-demo/desktop/preload/bridge.ts:4
        import { authEvent, oauthProvider, AUTH_LOGIN_CHANNEL, AUTH_INPUT_CHANNEL, AUTH_NEXT_CHANNEL, AUTH_CANCEL_CHANNEL, AUTH_LOGOUT_CHANNEL } from "../main/session-contract.js";
      > export namespace RozsaBridge {
          export function create(invoke: (channel: string, ...args: unknown[]) => Promise<unknown>, subscribe?: (channel: string, receive: (value: unknown) => void) => () => void): import("../main/session-contract").ProductBridge {
            const messageSubscriptions = new Map<string, import("../main/session-contract").MessageSubscription>();
            return Object.freeze({
              async startAuth(provider: import("../../contracts/ipc/session.js").OAuthProvider) {
                return messageOperation(messageSuccess(await invoke(AUTH_LOGIN_CHANNEL, oauthProvider(provider))));
              },
              async submitAuth(operationId: string, value: string) {
                if (!validKey(operationId) || !validKey(value)) throw new Error("Invalid OAuth input");
                messageSuccess(await invoke(AUTH_INPUT_CHANNEL, operationId, value));
              },
              async nextAuthEvent(operationId: string) {
                if (!validKey(operationId)) throw new Error("Invalid OAuth operation");
                return authEvent(messageSuccess(await invoke(AUTH_NEXT_CHANNEL, operationId)), operationId);
              },
              async cancelAuth(operationId: string) {
                if (!validKey(operationId)) throw new Error("Invalid OAuth operation");
                messageSuccess(await invoke(AUTH_CANCEL_CHANNEL, operationId));
              },
              async logoutAuth(provider: import("../../contracts/ipc/session.js").OAuthProvider) {
                messageSuccess(await invoke(AUTH_LOGOUT_CHANNEL, oauthProvider(provider)));
              },
              ...(subscribe ? { onModelsChanged(receive: (update: import("../../contracts/ipc/session.js").ModelUpdate) => void) {
                if (typeof receive !== "function" || !subscribe) throw new Error("Model events are unavailable");
                return subscribe("product:models-changed", value => {
                  if (typeof value !== "object" || value === null || !("models" in value) || !("diagnostics" in value)
                    || !Array.isArray(value.models) || !value.models.every(validModelOption) || !validDiagnostics(value.diagnostics)) throw new Error("Invalid model update");
                  receive(value as import("../../contracts/ipc/session.js").ModelUpdate);
                });
              }, onRuntimeInitializationError(receive: (message: string) => void) {
                if (typeof receive !== "function" || !subscribe) throw new Error("Runtime error events are unavailable");
                return subscribe(RUNTIME_INITIALIZATION_ERROR_CHANNEL, value => {
                  if (typeof value !== "string") throw new Error("Invalid runtime initialization error");
                  receive(value);
                });
              } } : {}),
              async getSessions(...args: never[]): Promise<import("../main/session-contract").SessionListReply> {
                if (args.length !== 0) throw new Error("getSessions accepts no arguments");
                return sessionReply(await invoke("product:get-sessions"));
              },
              async switchSession(...args: [import("../main/session-contract").SessionTarget]): Promise<import("../main/session-contract").SessionHistoryReply> {
                if (args.length !== 1) throw new Error("Invalid session target");
                const target = sessionTarget(args[0]);
                return historyReply(await invoke("product:switch-session", target), target);
              },
              async subscribeSessions(...args: never[]) {
                if (args.length !== 0) throw new Error("subscribeSessions accepts no arguments");
                return syncReply(await invoke(SESSION_SUBSCRIBE_CHANNEL));
              },
              async nextSessionEvent(...args: [string]) {
                if (args.length !== 1) throw new Error("Invalid session subscription");
                const id = subscriptionId(args[0]);
                return syncReply(await invoke(SESSION_NEXT_CHANNEL, id), id);
              },
              async unsubscribeSessions(...args: [string]) {
                if (args.length !== 1) throw new Error("Invalid session subscription");
                return unsubscribeReply(await invoke(SESSION_UNSUBSCRIBE_CHANNEL, subscriptionId(args[0])));
              },
              async startMessage(...args: [string]) {
                if (args.length !== 1 || typeof args[0] !== "string" || args[0].trim() === "") throw new Error("Invalid message text");
                return messageOperation(messageSuccess(await invoke(MESSAGE_START_CHANNEL, { text: args[0] })));
              },
              async continueMessage(...args: never[]) {
                if (args.length !== 0) throw new Error("continueMessage accepts no arguments");
                return messageOperation(messageSuccess(await invoke(MESSAGE_CONTINUE_CHANNEL)));
              },
              async getActiveMessage(...args: never[]) {
                if (args.length !== 0) throw new Error("getActiveMessage accepts no arguments");
                const value = messageSuccess(await invoke(MESSAGE_ACTIVE_CHANNEL));
                if (typeof value !== "object" || value === null || !("active" in value)) throw new Error("Invalid active message response");
                return value.active === null ? null : messageOperation(value.active);
              },
              async subscribeMessageEvents(...args: [string]) {
                if (args.length !== 1 || typeof args[0] !== "string" || !args[0]) throw new Error("Invalid message operation");
                const subscription = messageSubscription(messageSuccess(await invoke(MESSAGE_SUBSCRIBE_CHANNEL, { operationId: args[0] })), args[0]);
                messageSubscriptions.set(subscription.subscriptionId, subscription);
                return subscription;
              },
              async nextMessageEvent(...args: [string]) {
                if (args.length !== 1) throw new Error("Invalid message subscription");
                const id = subscriptionId(args[0]);
                const subscription = messageSubscriptions.get(id);
                if (!subscription) throw new Error("Unknown message event subscription");
                const value = messageSuccess(await invoke(MESSAGE_NEXT_CHANNEL, { subscriptionId: id }));
                return messageEventResult(value, subscription);
              },
              async unsubscribeMessageEvents(...args: [string]) {
                if (args.length !== 1) throw new Error("Invalid message subscription");
                const id = subscriptionId(args[0]);
                const result = messageSuccess(await invoke(MESSAGE_UNSUBSCRIBE_CHANNEL, id));
                if (result !== true && !(typeof result === "object" && result !== null && Object.keys(result).length === 0)) throw new Error("Invalid message unsubscribe response");
                messageSubscriptions.delete(id);
              },
              async cancelMessage(...args: [string]) {
                if (args.length !== 1 || typeof args[0] !== "string" || !args[0]) throw new Error("Invalid message operation");
                const result = messageSuccess(await invoke(MESSAGE_CANCEL_CHANNEL, { operationId: args[0] }));
                if (result !== true && !(typeof result === "object" && result !== null && Object.keys(result).length === 0)) throw new Error("Invalid message cancellation response");
              },
              async getSettings(...args: ["global" | "project"]) {
                if (args.length !== 1 || !validScope(args[0])) throw new Error("Invalid settings scope");
                return settingsSnapshot(messageSuccess(await invoke(SETTINGS_GET_CHANNEL, args[0])));
              },
              async updateSetting(...args: ["global" | "project", string, unknown]) {
                if (args.length !== 3 || !validScope(args[0]) || !validKey(args[1])) throw new Error("Invalid settings update");
                return settingsSnapshot(messageSuccess(await invoke(SETTINGS_UPDATE_CHANNEL, args[0], args[1], args[2])));
              },
              async resetSetting(...args: ["global" | "project", string]) {
                if (args.length !== 2 || !validScope(args[0]) || !validKey(args[1])) throw new Error("Invalid settings reset");
                return settingsSnapshot(messageSuccess(await invoke(SETTINGS_RESET_CHANNEL, args[0], args[1])));
              },
              async listThemes(...args: ["global" | "project"]) {
                if (args.length !== 1 || !validScope(args[0])) throw new Error("Invalid settings scope");
                const value = messageSuccess(await invoke(THEMES_LIST_CHANNEL, args[0]));
                if (typeof value !== "object" || value === null || !("themes" in value) || !Array.isArray(value.themes) || !value.themes.every(validThemeSummary)) throw new Error("Invalid theme list response");
                return value.themes;
              },
              async getTheme(...args: [string, "light" | "dark"]) {
                if (args.length !== 2 || !validKey(args[0]) || !validThemeMode(args[1])) throw new Error("Invalid theme request");
                return themeDefinition(messageSuccess(await invoke(THEME_GET_CHANNEL, args[0], args[1])));
              },
              async saveTheme(...args: [import("../../contracts/ipc/session.js").ThemeDefinition]) {
                if (args.length !== 1 || !validThemeDefinition(args[0])) throw new Error("Invalid theme definition");
                const value = messageSuccess(await invoke(THEME_SAVE_CHANNEL, args[0]));
                if (!(typeof value === "object" && value !== null && Object.keys(value).length === 0)) throw new Error("Invalid theme save response");
              },
            });
          }
      > }
    
    SyntaxError [ERR_UNSUPPORTED_TYPESCRIPT_SYNTAX]: TypeScript namespace declaration is not supported in strip-only mode
        at parseTypeScript (node:internal/modules/typescript:57:40)
        at processTypeScriptCode (node:internal/modules/typescript:129:37)
        at Object.stripTypeScriptModuleTypes (node:internal/modules/typescript:172:22)
        at stripTypeScriptModuleTypes (node:internal/modules/esm/translators:35:49)
        at ModuleLoader.<anonymous> (node:internal/modules/esm/translators:684:29)
        at #translate (node:internal/modules/esm/loader:441:20)
        at afterLoad (node:internal/modules/esm/loader:509:29)
        at ModuleLoader.loadAndTranslate (node:internal/modules/esm/loader:514:12)
        at #getOrCreateModuleJobAfterResolve (node:internal/modules/esm/loader:567:36)
        at afterResolve (node:internal/modules/esm/loader:614:52) {
      code: 'ERR_UNSUPPORTED_TYPESCRIPT_SYNTAX'
    }
    
    Node.js v26.10.0
    ✖ notification-center.test.js (168.443833ms)
    ℹ tests 1
    ℹ suites 0
    ℹ pass 0
    ℹ fail 1
    ℹ cancelled 0
    ℹ skipped 0
    ℹ todo 0
    ℹ duration_ms 176.392416
    
    ✖ failing tests:
    
    test at notification-center.test.js:1:1
    ✖ notification-center.test.js (168.443833ms)
      'test failed'
  - reproduce：node --test 'notification-center.test.js'
    project_root: /Users/xinyue/ballad/rozsa-demo
  - fix：Removed TypeScript-only preload imports from the JavaScript notification test; node --test desktop/tests/notification-center.test.js passes 6/6.
  - files_modify: [desktop/tests/notification-center.test.js]
  - files_create: []
