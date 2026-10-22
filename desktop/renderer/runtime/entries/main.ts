import { createUIHost } from "../../scripts/core/renderer.ts";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { PanelLeft } from "lucide-react";
import { createElectronAdapter } from "../adapters/electron.ts";
import { bindNativeViewport } from '../adapters/viewport.js';
import { mountShell, resolveTemplateRoot } from "../../scripts/core/shell.ts";
import { init as bindPopover } from "../../scripts/components/popover.js";
import { init as bindComposer, renderSendState, mountComposerInput, mountSuggestionPopup } from "../../scripts/components/composer.js";
import { emptyConversationMarkup, messageFrame, createMessageFrame, runningPanelMarkup, thinkingMarkup } from "../../scripts/features/conversation.ts";
import { mountConversation, createConversationFlow } from "../../scripts/flows/conversation.ts";
import { createConversationAdapter } from "../adapters/conversation.js";
import { createConversationRenderer } from "../../scripts/features/conversation-view.tsx";
import { mountInterruptionPanel } from "../../scripts/components/interruption-panel.tsx";
import * as tools from '../../scripts/features/tools.js';
import { init as bindTools } from '../../scripts/components/tool-call.js';
import { toolView, activityView } from '../adapters/tools.js';
import { init as bindInterruptionFlow } from '../../scripts/flows/composer-interruption.js';
import * as settings from '../../scripts/features/settings.tsx';
import { createSettingsAdapter, bindSettingSwitch } from '../adapters/settings.js';
import * as notifications from '../../scripts/features/notifications.js';
import { bindToast, bindErrorTray } from '../../scripts/components/notification.js';
import { subscribeNotifications } from '../adapters/notifications.js';
import * as sidebar from '../../scripts/features/sidebar.js';
import { loadTemplate } from '../../scripts/core/template-loader.ts';
import { renderMarkdownStatic } from '../../scripts/features/markdown-renderer.tsx';
import * as devFlow from '../../scripts/features/dev-flow.js';
import { devFlowSettingsView, devFlowDetailView } from '../adapters/sidebar-dev-flow.js';
import { reportError, resolveError } from '../../error-reporting.ts';

const sidebarToggle = document.querySelector<HTMLButtonElement>(".sidebar-toggle-button");
if (!sidebarToggle) throw new Error("Sidebar toggle button is missing");
sidebarToggle.innerHTML = renderToStaticMarkup(createElement(PanelLeft, {
  className: "lucide", size: 17, strokeWidth: 1.7, "aria-hidden": true,
}));

declare global {
  interface Window {
    RozsaSessionCatalogReady: Promise<unknown>;
    RozsaInitialStateReady: Promise<unknown>;
    RozsaSidebarReady: Promise<void>;
    RozsaMarkdownRenderer: (source: string) => Promise<string>;
    renderMarkdown: (source: string) => Promise<string>;
  }
}

const renderMarkdown = (source: string): Promise<string> => import('../../scripts/features/markdown-renderer.tsx')
  .then(module => module.renderMarkdownStatic(source));
window.RozsaMarkdownRenderer = renderMarkdown;
window.renderMarkdown = renderMarkdown;
window.showError = reportError;
window.resolveError = resolveError;

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
  const perfState = {
    longTasks: [] as Array<{ startTime: number; duration: number }>,
    frameGaps: [] as number[],
    markdownStages: { renderCommit: [] as number[], peleMount: [] as number[], peleAdapt: [] as number[] },
    visibility: [{ at: performance.now(), state: document.visibilityState, focused: document.hasFocus() }],
    longTaskSupported: false,
  };
  Object.defineProperty(window, "__ROZSA_PERF__", { configurable: false, value: perfState });
  const captureVisibility = (): void => {
    perfState.visibility.push({ at: performance.now(), state: document.visibilityState, focused: document.hasFocus() });
  };
  document.addEventListener("visibilitychange", captureVisibility);
  window.addEventListener("focus", captureVisibility);
  window.addEventListener("blur", captureVisibility);
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
  window.RozsaNotifications = Object.freeze({ ...notifications, bindToast, bindErrorTray, subscribeNotifications });
  window.RozsaSettings = Object.freeze({ ...settings, bindSettingSwitch });
  window.RozsaDevFlow = Object.freeze({ ...devFlow, devFlowSettingsView, devFlowDetailView });
  devFlow.init({ boundModules: new Set(), byId: (id: string) => document.getElementById(id), closeDevFlowDetail });
  settings.init({ ...createSettingsAdapter({ switchSettingsTab, closeSettings, selectThemeModeCard,
    previewFontSize, commitFontSize }), boundModules: new Set(), byId: (id: string) => document.getElementById(id) });
  const disposeSettingsPanes = settings.mountSettingsPanes(document, window.rozsa);
  window.addEventListener('pagehide', disposeSettingsPanes, { once: true });
  window.RozsaTools = Object.freeze({ ...tools, toolView, activityView });
  bindTools({ boundModules: new Set(), byId: (id: string) => document.getElementById(id), toggleToolCall, toggleTurnDiff });
  const interruptionMount = document.getElementById("interruptionReactRoot");
  if (!interruptionMount) throw new Error("Interruption panel mount is missing");
  const interruptionPanel = mountInterruptionPanel(interruptionMount, window.rozsa);
  const detachInterruptionState = await window.RozsaUIHost.port.subscribe("ui-state", snapshot => {
    if (typeof snapshot === "object" && snapshot !== null && "sessionId" in snapshot && typeof snapshot.sessionId === "string") {
      interruptionPanel.setSessionId(snapshot.sessionId);
    }
  });
  void window.RozsaInitialStateReady.then(snapshot => {
    if (typeof snapshot === "object" && snapshot !== null && "sessionId" in snapshot && typeof snapshot.sessionId === "string") {
      interruptionPanel.setSessionId(snapshot.sessionId);
    }
  }, () => undefined);
  window.addEventListener("pagehide", () => {
    detachInterruptionState();
    interruptionPanel.dispose();
  }, { once: true });
  bindInterruptionFlow({ boundModules: new Set(), byId: (id: string) => document.getElementById(id) });
  document.querySelector('[data-od-id="chat-input"]')!.prepend(document.querySelector<HTMLTemplateElement>('#runtimeSubagentTemplate')!.content.cloneNode(true));
  configureConversationSuspensions();
  window.RozsaConversation = Object.freeze({ emptyConversationMarkup, messageFrame, createMessageFrame, runningPanelMarkup, thinkingMarkup,
    renderView: conversationRenderer.render });
  const inputMount = document.querySelector<HTMLElement>("[data-composer-input-mount]");
  const autocompleteMount = document.getElementById("autocomplete");
  if (!inputMount || !autocompleteMount) throw new Error("Composer template mounts are missing");
  const disposeInput = mountComposerInput(inputMount);
  const suggestionPopup = mountSuggestionPopup(autocompleteMount);
  window.addEventListener("pagehide", () => {
    disposeInput();
    suggestionPopup.dispose();
  }, { once: true });
  window.RozsaComposer = Object.freeze({
    renderSendState,
    renderSuggestions: suggestionPopup.render.bind(suggestionPopup),
    clearSuggestions: suggestionPopup.clear.bind(suggestionPopup),
    scrollSuggestionTo: suggestionPopup.scrollTo.bind(suggestionPopup),
  });
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
