import { createUIHost } from "../../scripts/core/renderer.ts";
import { createElectronAdapter } from "../adapters/electron.ts";
import { bindNativeViewport } from '../adapters/viewport.js';
import { mountShell, resolveTemplateRoot } from "../../scripts/core/shell.ts";
import { init as bindPopover } from "../../scripts/components/popover.js";
import { init as bindComposer, renderSendState } from "../../scripts/components/composer.js";
import { emptyConversationMarkup, messageFrame, createMessageFrame, runningPanelMarkup, thinkingMarkup } from "../../scripts/features/conversation.ts";
import { mountConversation, createConversationFlow } from "../../scripts/flows/conversation.ts";
import { createConversationAdapter } from "../adapters/conversation.js";
import { createConversationRenderer } from "../../scripts/features/conversation-view.tsx";
import * as tools from '../../scripts/features/tools.js';
import { init as bindTools } from '../../scripts/components/tool-call.js';
import { toolView, activityView } from '../adapters/tools.js';
import * as permissions from '../../scripts/features/permissions.js';
import { init as bindInterruption } from '../../scripts/components/interruption-panel.js';
import { init as bindInterruptionFlow } from '../../scripts/flows/composer-interruption.js';
import { createInterruptionAdapter } from '../adapters/interruption.js';
import * as settings from '../../scripts/features/settings.tsx';
import { createSettingsAdapter, bindSettingSwitch } from '../adapters/settings.js';
import * as notifications from '../../scripts/features/notifications.js';
import { bindToast, bindErrorTray } from '../../scripts/components/notification.js';
import { subscribeNotifications, showRuntimeError } from '../adapters/notifications.js';
import * as sidebar from '../../scripts/features/sidebar.js';
import { loadTemplate } from '../../scripts/core/template-loader.ts';
import { renderMarkdown as renderMarkdownCore } from '../../scripts/features/markdown-core.ts';
import * as devFlow from '../../scripts/features/dev-flow.js';
import { devFlowSettingsView, devFlowDetailView } from '../adapters/sidebar-dev-flow.js';

declare global {
  interface Window {
    RozsaSessionCatalogReady: Promise<unknown>;
    RozsaInitialStateReady: Promise<unknown>;
    RozsaSidebarReady: Promise<void>;
    RozsaMarkdownRenderer: (source: string) => string;
    RozsaMarkdownWorker: { prepare(jobs: Array<{ key: string; source: string }>): Promise<Record<string, string>> };
    renderMarkdown: (source: string) => string;
  }
}

let markdownWorker: Worker | undefined;
let markdownWorkerFailure: Error | undefined;
let markdownRequestId = 0;
const markdownRequests = new Map<number, {
  resolve(value: Record<string, string>): void;
  reject(error: Error): void;
}>();
function requireMarkdownWorker(): Worker {
  if (markdownWorkerFailure) throw markdownWorkerFailure;
  if (!markdownWorker) {
    markdownWorker = new Worker(new URL('../../scripts/workers/markdown.worker.ts', import.meta.url), {
      type: 'module', name: 'rozsa-markdown',
    });
    markdownWorker.addEventListener('message', event => {
      const pending = markdownRequests.get(event.data.requestId);
      if (!pending) return;
      markdownRequests.delete(event.data.requestId);
      pending.resolve(event.data.html);
    });
    const fail = (event: ErrorEvent | MessageEvent): void => {
      markdownWorkerFailure = new Error('Markdown worker failed', { cause: event });
      for (const pending of markdownRequests.values()) pending.reject(markdownWorkerFailure);
      markdownRequests.clear();
    };
    markdownWorker.addEventListener('error', fail);
    markdownWorker.addEventListener('messageerror', fail);
  }
  return markdownWorker;
}
window.RozsaMarkdownRenderer = renderMarkdownCore;
window.renderMarkdown = renderMarkdownCore;
window.RozsaMarkdownWorker = Object.freeze({
  prepare(jobs: Array<{ key: string; source: string }>): Promise<Record<string, string>> {
    const worker = requireMarkdownWorker();
    const requestId = ++markdownRequestId;
    return new Promise((resolve, reject) => {
      markdownRequests.set(requestId, { resolve, reject });
      worker.postMessage({ requestId, jobs });
    });
  },
});

const performanceCapture = new URLSearchParams(location.search).get("rozsa_perf") === "1";
const startupAnchors = [
  ["main-panel", "#mainContentScene"],
  ["conversation", "#chatMessages"],
  ["session-list", "#sessionList"],
  ["composer", "#composer"],
  ["message-input", "#msgInput"],
  ["input-toolbar", ".input-toolbar"],
  ["send-button", "#send-btn"],
];
const observedAnchors = new Set<string>();
function markStartupStage(name: string): void {
  if (performanceCapture) performance.mark(`rozsa:${name}`);
}
if (performanceCapture) {
  const perfState = { longTasks: [] as Array<{ startTime: number; duration: number }>, frameGaps: [] as number[], markdownWorkerDurations: [] as number[], longTaskSupported: false };
  Object.defineProperty(window, "__ROZSA_PERF__", { configurable: false, value: perfState });
  markStartupStage("renderer-entry");
  perfState.longTaskSupported = PerformanceObserver.supportedEntryTypes.includes("longtask");
  if (perfState.longTaskSupported) {
    new PerformanceObserver(list => {
      for (const entry of list.getEntries()) perfState.longTasks.push({ startTime: entry.startTime, duration: entry.duration });
    }).observe({ type: "longtask", buffered: true });
  }
  let previousFrame = 0;
  const sampleFrames = (now: number): void => {
    if (previousFrame) perfState.frameGaps.push(now - previousFrame);
    previousFrame = now;
    requestAnimationFrame(sampleFrames);
  };
  requestAnimationFrame(sampleFrames);
  const markVisibleAnchors = (): void => {
    for (const [name, selector] of startupAnchors) {
      if (observedAnchors.has(name)) continue;
      const element = document.querySelector<HTMLElement>(selector);
      if (!element) continue;
      const style = getComputedStyle(element);
      if (style.display === "none" || style.visibility === "hidden" || Number(style.opacity) === 0 || !element.getClientRects().length) continue;
      observedAnchors.add(name);
      requestAnimationFrame(() => requestAnimationFrame(() => markStartupStage(`visible:${name}`)));
    }
  };
  new MutationObserver(markVisibleAnchors).observe(document.documentElement, { childList: true, subtree: true, attributes: true });
  markVisibleAnchors();
  window.addEventListener("load", () => markStartupStage("window-load"), { once: true });
}

document.body.classList.toggle('macos-host', navigator.platform === 'MacIntel');
window.RozsaUIHost = createUIHost(createElectronAdapter(window.rozsa));
markStartupStage("product-bridge-ready");
// Start the first backend reads as soon as the bridge exists. DOM composition
// continues in parallel; state.ts renders these results once their mounts land.
window.RozsaSessionCatalogReady = window.RozsaUIHost.port.request("get_sessions").then(value => {
  markStartupStage("session-catalog-response");
  return value;
});
window.RozsaInitialStateReady = window.RozsaUIHost.port.request("get_state").then(value => {
  markStartupStage("initial-state-response");
  return value;
});
void window.RozsaSessionCatalogReady.catch(() => {});
void window.RozsaInitialStateReady.catch(() => {});
window.addEventListener("pagehide", () => window.RozsaUIHost.port.dispose(), { once: true });
let conversationRenderer: ReturnType<typeof createConversationRenderer> | undefined;
window.addEventListener("pagehide", () => conversationRenderer?.dispose(), { once: true });
const sidebarTemplateReady = loadTemplate(new URL('features/sidebar.html', resolveTemplateRoot())).then(template => {
  document.querySelector<HTMLTemplateElement>('#fallbackSidebarTemplate')!.content.replaceChildren(template);
  window.RozsaSidebar = Object.freeze({ ...sidebar });
  markStartupStage("sidebar-template-loaded");
  return template;
});
window.RozsaSidebarReady = sidebarTemplateReady.then(() => {});
const toolTemplateReady = tools.loadToolTemplate();
window.RozsaUIReady = Promise.all([bindNativeViewport(), mountConversation(document.getElementById("mainContentScene"))]).then(() => {
  markStartupStage("conversation-template-mounted");
  return Promise.all([mountShell([
  { url: new URL("composites/popover.html", resolveTemplateRoot()), selector: "[data-template-mount=popover]", children: true },
  { url: new URL("composites/interruption-panel.html", resolveTemplateRoot()), selector: "[data-template-mount=interruption-panel]", children: true },
  { url: new URL("features/settings.html", resolveTemplateRoot()), selector: "[data-template-mount=settings]", children: true },
  { url: new URL("composites/notification.html", resolveTemplateRoot()), selector: "[data-template-mount=notification]", children: true },
  { url: new URL("features/dev-flow.html", resolveTemplateRoot()), selector: "[data-template-mount=dev-flow]", children: true },
  ]), sidebarTemplateReady, toolTemplateReady]);
}).then(async () => {
  markStartupStage("shell-templates-mounted");
  const conversationRoot = document.getElementById("chatMessages");
  if (!conversationRoot) throw new Error("Conversation React root not found after mounting the conversation template");
  conversationRenderer = createConversationRenderer(conversationRoot);
  window.RozsaNotifications = Object.freeze({ ...notifications, bindToast, bindErrorTray, subscribeNotifications, showRuntimeError });
  window.RozsaSettings = Object.freeze({ ...settings, bindSettingSwitch });
  window.RozsaDevFlow = Object.freeze({ ...devFlow, devFlowSettingsView, devFlowDetailView });
  devFlow.init({ boundModules: new Set(), byId: (id: string) => document.getElementById(id), closeDevFlowDetail });
  settings.init({ ...createSettingsAdapter({ switchSettingsTab, closeSettings, selectThemeModeCard,
    previewFontSize, commitFontSize }), boundModules: new Set(), byId: (id: string) => document.getElementById(id) });
  const disposeSettingsPanes = settings.mountSettingsPanes(document, window.rozsa);
  window.addEventListener('pagehide', disposeSettingsPanes, { once: true });
  window.RozsaTools = Object.freeze({ ...tools, toolView, activityView });
  bindTools({ boundModules: new Set(), byId: (id: string) => document.getElementById(id), toggleToolCall, toggleTurnDiff });
  window.RozsaPermissions = permissions;
  const interruption = {
    ...createInterruptionAdapter({ respondPermission, enterPermissionTrust, enterPermissionHint,
      togglePermissionCommand, showPermissionMainPage, submitPermissionHint, submitUserQuestion,
      normalizePermissionHint, handlePermissionHintKeydown }),
    boundModules: new Set(), byId: (id: string) => document.getElementById(id),
  };
  configureInterruptionSuspensions();
  bindInterruption(interruption);
  bindInterruptionFlow(interruption);
  document.querySelector('[data-od-id="chat-input"]')!.prepend(document.querySelector<HTMLTemplateElement>('#runtimeSubagentTemplate')!.content.cloneNode(true));
  configureConversationSuspensions();
  window.RozsaConversation = Object.freeze({ emptyConversationMarkup, messageFrame, createMessageFrame, runningPanelMarkup, thinkingMarkup,
    renderView: conversationRenderer.render });
  window.RozsaComposer = Object.freeze({ renderSendState });
  const flow = createConversationFlow(createConversationAdapter({
    attachFileReference, attachDirectoryReference, insertSlashCommandPrefix, showModelPicker,
    updateAbortButton,
    submit: () => isStreaming ? abortAgent() : sendMessage(),
  }));
  bindComposer({
    ...flow, boundModules: new Set(), byId: (id: string) => document.getElementById(id),
    handleInput, handleCompositionStart, handleCompositionUpdate, handleCompositionEnd,
  });
  window.RozsaPopover = bindPopover({
    boundModules: new Set(),
    byId: (id: string) => document.getElementById(id),
    beforeOpen: () => { dismissTransientPopups(); renderThinkingEffortPicker(); },
    previewThinkingEffort,
    selectThinkingEffort,
  });
  markStartupStage("ui-interactive-ready");
});
