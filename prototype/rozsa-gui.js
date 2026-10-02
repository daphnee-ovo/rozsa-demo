

"use strict";

(() => {
  const STORE_KEY = "rozsa-open-design-gui-v2";
  const root = document.documentElement;
  const appBody = document.querySelector("[data-od-id=app-body]");
  const mainContent = document.getElementById("mainContentScene");
  const settingsPanel = document.getElementById("settingsPanel");
  const settingsContent = document.querySelector(".settings-content");
  const sceneName = document.documentElement.dataset.rozsaScene || "";
  const legacyScriptUrl = new URL(document.currentScript?.src || "rozsa-gui.js", document.baseURI);
  const behaviorModulePaths = [
    "components/composer.js",
    "components/interruption-panel.js",
    "components/popover.js",
    "components/notification.js",
    "components/tool-call.js",
    "features/conversation.js",
    "features/settings.js",
    "features/sidebar.js",
    "features/dev-flow.js",
    "flows/composer-interruption.js",
    "flows/scenario-flow.js",
  ];
  const state = {
    themeMode: "light",
    fontSize: 14,
    activeSession: 0,
    sessions: [
      { name: "Current session", date: "" },
      { name: "Untitled", date: "" }
    ],
    messages: []
  };
  let devFlowDetailOpen = false;
  let devFlowFixture = null;
  let devFlowDashboardNotifications = null;
  let permissionRequest = null;
  let questionRequest = null;
  let prototypeResponses = null;
  let sharedConversation;

  async function loadBehaviorModules() {
    const api = window.RozsaPrototype;
    if (!api) throw new Error("Prototype behavior API is unavailable");
    const { createConversationFlow } = await import(new URL("../crates/rozsa-gui/frontend/ui/scripts/flows/conversation.js", legacyScriptUrl));
    const { createMockConversationAdapter } = await import(new URL("./scripts/adapters/mock-adapter.js", legacyScriptUrl));
    Object.assign(api, createConversationFlow(createMockConversationAdapter(api)));
    const flowEntry = document.body?.dataset.rozsaFlowEntry;
    const paths = behaviorModulePaths.filter(path => !path.startsWith("flows/") || flowEntry);
    const modules = await Promise.all(paths.map(path => import(new URL("./scripts/" + path, legacyScriptUrl))));
    modules.forEach(module => module.init(api));
  }

  function emitFlowEvent(name, detail = {}) {
    document.dispatchEvent(new CustomEvent(name, { detail }));
  }

  const byId = id => document.getElementById(id);
  const all = selector => Array.from(document.querySelectorAll(selector));
  const escapeHtml = value => String(value).replace(/[&<>'"]/g, char => ({
    "&": "&amp;", "<": "&lt;", ">": "&gt;", "'": "&#39;", "\"": "&quot;"
  }[char]));
  const lucidePaths = {
    "chevron-right": '<path d="m9 18 6-6-6-6"/>',
    "circle-alert": '<circle cx="12" cy="12" r="10"/><line x1="12" x2="12" y1="8" y2="12"/><line x1="12" x2="12.01" y1="16" y2="16"/>',
    "circle-check": '<circle cx="12" cy="12" r="10"/><path d="m9 12 2 2 4-4"/>',
    "circle-help": '<circle cx="12" cy="12" r="10"/><path d="M9.09 9a3 3 0 1 1 5.83 1c0 2-3 2-3 4"/><path d="M12 17h.01"/>',
    "circle-x": '<circle cx="12" cy="12" r="10"/><path d="m15 9-6 6"/><path d="m9 9 6 6"/>',
    "corner-down-right": '<polyline points="15 10 20 15 15 20"/><path d="M4 4v7a4 4 0 0 0 4 4h12"/>',
    "file-pen-line": '<path d="M12 20h9"/><path d="M16.5 3.5a2.12 2.12 0 0 1 3 3L8 18l-4 1 1-4Z"/><path d="m15 5 3 3"/>',
    "file-plus-2": '<path d="M4 22h14a2 2 0 0 0 2-2V7l-5-5H6a2 2 0 0 0-2 2v4"/><path d="M14 2v6h6"/><path d="M3 15h6"/><path d="M6 12v6"/>',
    "info": '<circle cx="12" cy="12" r="10"/><path d="M12 16v-4"/><path d="M12 8h.01"/>',
    "lightbulb": '<path d="M9 18h6"/><path d="M10 22h4"/><path d="M15.09 14c.18-.7.66-1.22 1.18-1.75A6 6 0 1 0 7.73 12.25c.52.52 1 1.04 1.18 1.75Z"/>',
    "triangle-alert": '<path d="m21.73 18-8-14a2 2 0 0 0-3.46 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3"/><path d="M12 9v4"/><path d="M12 17h.01"/>',
    "x": '<path d="M18 6 6 18"/><path d="m6 6 12 12"/>'
  };
  function lucideIcon(name, className = "") {
    const paths = lucidePaths[name];
    if (!paths) return "";
    return '<svg class="lucide' + (className ? " " + className : "") + '" data-lucide="' + name + '" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" focusable="false">' + paths + "</svg>";
  }

  function loadState() {
    const raw = localStorage.getItem(STORE_KEY);
    if (!raw) return;
    try {
      const saved = JSON.parse(raw);
      if (saved && ["light", "dark", "system"].includes(saved.themeMode)) state.themeMode = saved.themeMode;
      if (saved && Number.isFinite(saved.fontSize)) state.fontSize = Math.min(30, Math.max(5, saved.fontSize));
      if (saved && Number.isInteger(saved.activeSession)) state.activeSession = Math.max(0, saved.activeSession);
      if (saved && Array.isArray(saved.sessions) && saved.sessions.length) state.sessions = saved.sessions;
      if (saved && Array.isArray(saved.messages)) state.messages = saved.messages;
    } catch (error) {
      console.warn("Rózsa local state could not be restored.", error);
    }
  }

  function saveState() {
    localStorage.setItem(STORE_KEY, JSON.stringify({
      themeMode: state.themeMode,
      fontSize: state.fontSize,
      activeSession: state.activeSession,
      sessions: state.sessions,
      messages: state.messages
    }));
  }

  const themeTokens = {
    light: {
      "--surface": "oklch(100% 0 0)",
      "--muted": "oklch(55% 0.01 350)",
      "--border": "oklch(90% 0.006 350)",
      "--accent-hover": "oklch(54% 0.08 355)",
      "--accent-btn": "oklch(50% 0.08 355)",
      "--accent-bg": "oklch(96% 0.02 355)",
      "--accent-border": "oklch(88% 0.035 355)",
      "--success": "oklch(48% 0.10 155)",
      "--success-bg": "oklch(96% 0.025 155)",
      "--error": "oklch(52% 0.14 25)",
      "--error-bg": "oklch(96% 0.025 25)",
      "--warning": "oklch(70% 0.12 85)",
      "--warning-bg": "oklch(97% 0.03 85)",
      "--user-bg": "oklch(94% 0.015 355)",
      "--code-bg": "oklch(96% 0.003 260)",
      "--code-border": "oklch(90% 0.005 260)",
      "--sidebar-bg": "oklch(97.5% 0.004 350)",
      "--titlebar-bg": "oklch(98.5% 0.003 350)"
    },
    dark: {
      "--surface": "#282326",
      "--muted": "#b6a8ad",
      "--border": "#493f43",
      "--accent-hover": "#efabb1",
      "--accent-btn": "#c8757e",
      "--accent-bg": "#3b282d",
      "--accent-border": "#66434a",
      "--success": "#82c59a",
      "--success-bg": "#20392a",
      "--error": "#f09a9a",
      "--error-bg": "#43292b",
      "--warning": "#e5bf75",
      "--warning-bg": "#413722",
      "--user-bg": "#35262c",
      "--code-bg": "#171517",
      "--code-border": "#40383c",
      "--sidebar-bg": "#211d1f",
      "--titlebar-bg": "#181618"
    }
  };

  function effectiveTheme(mode) {
    return mode === "system"
      ? (window.matchMedia && window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light")
      : mode;
  }

  function applyTheme(mode, persist = true) {
    state.themeMode = mode;
    const effective = effectiveTheme(mode);
    root.dataset.themeMode = effective;
    const tokens = themeTokens[effective];
    Object.entries(tokens).forEach(([key, value]) => root.style.setProperty(key, value));
    root.style.setProperty("--accent", effective === "dark" ? "#d88991" : "#D7827E");
    root.style.setProperty("--fg", effective === "dark" ? "#f1e9eb" : "#575279");
    root.style.setProperty("--font-ui", "-apple-system, BlinkMacSystemFont, 'SF Pro Text', 'Segoe UI', system-ui, sans-serif");
    root.style.setProperty("--font-mono", "'JetBrains Mono', 'Cascadia Code', 'Fira Code', ui-monospace, Menlo, monospace");
    all("[data-theme-mode-card]").forEach(card => {
      card.classList.toggle("active", card.dataset.themeModeCard === mode);
      card.setAttribute("aria-pressed", String(card.dataset.themeModeCard === mode));
    });
    if (persist) saveState();
  }

  function mountTemplates() {
    if (appBody && mainContent && !byId("sidebar")) {
      const fragment = byId("fallbackSidebarTemplate").content.cloneNode(true);
      const sidebar = fragment.querySelector("[data-od-id=sidebar]");
      sidebar.id = "sidebar";
      sidebar.setAttribute("data-od-id", "sidebar");
      appBody.insertBefore(sidebar, mainContent);
    }
    mountDevFlowSidebar();
    const workspace = document.querySelector(".settings-workspace");
    if (workspace && !workspace.querySelector(".settings-tabs")) {
      const nav = byId("fallbackSettingsNavigationTemplate").content.cloneNode(true).querySelector(".settings-tabs");
      workspace.insertBefore(nav, settingsContent);
    }
  }

  function mountDevFlowSidebar() {
    const sidebar = byId("sidebar");
    const statusPanel = sidebar?.querySelector(".status-panel");
    if (!sidebar || !statusPanel || byId("devFlowSidebarGroup")) return;
    const statusGroup = document.createElement("div");
    statusGroup.className = "status-group";
    statusGroup.dataset.odId = "dev-flow-sidebar-status";
    statusGroup.innerHTML = '<div class="dev-flow-group" id="devFlowSidebarGroup" hidden>' +
      '<button class="dev-flow-summary" id="devFlowSidebarSummary" type="button" title="Show dev-flow work details">—</button>' +
      '<div class="dev-flow-claimed" id="devFlowSidebarClaimed"></div>' +
      '<button class="dev-flow-more" id="devFlowSidebarMore" type="button" hidden></button></div>';
    statusPanel.appendChild(statusGroup);
    const bottom = sidebar.querySelector(".sidebar-bottom");
    if (bottom && !byId("sidebarDevFlowDashboard")) {
      const dashboard = document.createElement("button");
      dashboard.className = "sidebar-btn";
      dashboard.id = "sidebarDevFlowDashboard";
      dashboard.type = "button";
      dashboard.hidden = true;
      dashboard.textContent = "Dashboard";
      dashboard.title = "Open Dev Flow dashboard";
      bottom.insertBefore(dashboard, bottom.firstChild);
    }
  }

  function sessionLabel(session) {
    return session.name || session.first_message || "Untitled";
  }

  function renderSessionList() {
    const list = byId("sessionList");
    if (!list) return;
    list.innerHTML = state.sessions.map((session, index) =>
      '<div class="session-item' + (index === state.activeSession ? " active" : "") +
      '" data-session-index="' + index + '" data-od-id="session-' + index + '" tabindex="0" role="button">' +
      '<span class="session-status ' + (index === state.activeSession ? "running" : "idle") + '"></span>' +
      '<div class="session-name">' + escapeHtml(sessionLabel(session)) + "</div>" +
      '<div class="session-meta"><span>' + escapeHtml(session.date || "") + "</span></div></div>"
    ).join("");
  }

  function textMarkup(value) {
    return escapeHtml(value || "").split(/\n{2,}/).filter(Boolean).map(paragraph =>
      "<p>" + paragraph.replace(/\n/g, "<br>") + "</p>"
    ).join("");
  }

  function messageFrame(role, body, index, continuation = false) {
    return sharedConversation.messageFrame(role, body, index, continuation);
  }

  function toolMarkup(id, name, arg, output, status, options = {}) {
    const stateClass = options.expanded ? " expanded" : "";
    const extraClass = options.className ? " " + options.className : "";
    const body = options.bodyHtml || '<pre style="white-space:pre-wrap;margin:0">' + escapeHtml(output || "") + "</pre>";
    return '<div class="tool-call' + stateClass + extraClass + '" data-tool-call-id="' + escapeHtml(id) + '" data-od-id="tool-call-' + escapeHtml(id) + '" tabindex="0">' +
      '<div class="tool-track"><div class="tool-icon">' + lucideIcon("corner-down-right") + '</div></div>' +
      '<div class="tool-content"><div class="tool-header"><span class="tool-call-status ' + status + '"></span>' +
      '<span class="tool-name">' + escapeHtml(name) + '</span><span class="tool-call-args">' + escapeHtml(arg) +
      '</span><span class="tool-call-toggle">' + lucideIcon("chevron-right") + '</span></div></div>' +
      '<div class="tool-call-body' + (options.bodyClass ? " " + options.bodyClass : "") + '">' + body + "</div></div>";
  }

  function thinkingMarkup(text, expanded = true, active = false, duration = "") {
    return sharedConversation.thinkingMarkup(textMarkup(text), { expanded, active, duration });
  }

  function codeViewMarkup(content) {
    return String(content || "").split("\n").map((line, index) =>
      '<div class="code-line"><span class="code-ln">' + (index + 1) + '</span><span class="code-text">' + escapeHtml(line) + "</span></div>"
    ).join("");
  }

  function diffViewMarkup(patch) {
    let oldLine = 1;
    let newLine = 1;
    return String(patch || "").split("\n").map(line => {
      const hunk = line.match(/^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/);
      if (hunk) {
        oldLine = Number.parseInt(hunk[1], 10);
        newLine = Number.parseInt(hunk[2], 10);
        return "";
      }
      if (line.startsWith("---") || line.startsWith("+++") || !line) return "";
      if (line.startsWith("-")) {
        return '<div class="diff-line diff-del"><span class="diff-sign">−</span><span class="diff-ln">' +
          oldLine++ + '</span><span class="diff-text">' + escapeHtml(line.slice(1)) + "</span></div>";
      }
      if (line.startsWith("+")) {
        return '<div class="diff-line diff-add"><span class="diff-sign">+</span><span class="diff-ln">' +
          newLine++ + '</span><span class="diff-text">' + escapeHtml(line.slice(1)) + "</span></div>";
      }
      const lineNumber = newLine;
      oldLine++;
      newLine++;
      return '<div class="diff-line"><span class="diff-sign"> </span><span class="diff-ln">' +
        lineNumber + '</span><span class="diff-text">' + escapeHtml(line.slice(1)) + "</span></div>";
    }).join("");
  }

  function evidenceMarkup(label, output) {
    return '<div class="tool-evidence"><div class="tool-evidence-section"><div class="tool-evidence-label">' +
      escapeHtml(label) + '</div><pre class="tool-evidence-output">' + escapeHtml(output || "") + "</pre></div></div>";
  }

  function changesMarkup(files, verification) {
    const rows = (files || []).map(file => '<div class="change-entry"><div class="change-row">' +
      '<span class="change-icon ' + (file.status === "added" ? "new" : "mod") + '">' + lucideIcon(file.status === "added" ? "file-plus-2" : "file-pen-line") + '</span>' +
      '<button class="change-name" type="button" aria-expanded="false" data-turn-diff="' + escapeHtml(JSON.stringify({ patch: file.patch || "" })) + '">' +
      escapeHtml(file.path) + '</button><span class="change-add">+' + escapeHtml(file.added) + '</span><span class="change-del">-' + escapeHtml(file.deleted) + '</span><span class="change-toggle">' + lucideIcon("chevron-right") + '</span>' +
      '</div><div class="turn-diff-inline" hidden></div></div>').join("");
    const footer = verification ? '<div class="changes-footer"><span class="change-add">Verified</span><span class="changes-runtime">' +
      escapeHtml(verification.command) + " · exit " + escapeHtml(verification.exitCode) + " · " + escapeHtml(verification.duration) + "</span></div>" : "";
    return '<div class="changes-card"><div class="changes-header"><span>Changes: ' + (files || []).length + ' files</span></div>' +
      '<div class="changes-list">' + rows + '</div>' + footer + "</div>";
  }

  function toggleTurnDiff(button) {
    const entry = button.closest(".change-entry");
    const panel = entry && entry.querySelector(".turn-diff-inline");
    if (!panel) return;
    const change = JSON.parse(button.dataset.turnDiff || "{}");
    const opening = panel.hidden;
    panel.hidden = !opening;
    button.setAttribute("aria-expanded", String(opening));
    entry.classList.toggle("expanded", opening);
    panel.innerHTML = opening ? '<div class="diff-view">' + diffViewMarkup(change.patch || "") + "</div>" : "";
  }

  function messageBody(message, index) {
    let body = "";
    if (message.error) body += '<div class="msg-content msg-error"><pre>' + escapeHtml(message.error) + "</pre></div>";
    if (message.thinking) body += thinkingMarkup(message.thinking, message.thinkingExpanded !== false, message.thinkingActive, message.thinkingDuration);
    if (message.text) body += textMarkup(message.text);
    if (message.tool) body += toolMarkup("read-" + index, "Read", "crates/rozsa-gui/frontend/index.html", "Read the current GUI entry and preserve its existing structure.", "s-success", { expanded: Boolean(message.toolExpanded) });
    if (Array.isArray(message.tools)) message.tools.forEach((tool, toolIndex) => {
      let bodyHtml = "";
      if (tool.kind === "code") bodyHtml = '<div class="tool-evidence"><div class="tool-evidence-section"><div class="tool-evidence-label">File content</div><div class="code-view tool-evidence-code-view">' + codeViewMarkup(tool.content) + "</div></div></div>";
      else if (tool.kind === "diff") bodyHtml = '<div class="tool-evidence"><div class="tool-evidence-section"><div class="tool-evidence-label">Diff</div><div class="diff-view tool-evidence-diff-view">' + diffViewMarkup(tool.patch) + "</div></div></div>";
      else bodyHtml = evidenceMarkup(tool.evidenceLabel || "Output", tool.output);
      body += toolMarkup(tool.id || ("tool-" + index + "-" + toolIndex), tool.name, tool.args, tool.output, tool.status || "s-success", {
        expanded: tool.expanded !== false,
        className: tool.className || "",
        bodyHtml
      });
    });
    if (message.changes) body += changesMarkup(message.changes.files, message.changes.verification);
    if (message.typing) body += '<div class="typing-indicator"><span></span><span></span><span></span></div>';
    return body;
  }

  function renderMessages() {
    const chat = byId("chatMessages");
    if (!chat) return;
    if (!state.messages.length) {
      chat.innerHTML = sharedConversation.emptyConversationMarkup();
      return;
    }
    chat.innerHTML = state.messages.map((message, index) => messageFrame(
      message.role === "user" ? "user" : "assistant",
      messageBody(message, index),
      index,
      Boolean(message.continuation),
    )).join("");
  }

  function openPermissionDemo(request = permissionRequest) {
    if (!request) throw new Error("Prototype permission request is unavailable");
    permissionRequest = request;
    const panel = byId("permPanel");
    if (!panel) return;
    const tool = byId("permTool");
    const desc = byId("permDesc");
    const cmd = byId("permCmd");
    if (tool) tool.textContent = request.tool;
    if (desc) desc.textContent = request.description;
    if (cmd) cmd.innerHTML = renderPermissionCommand(request.command, request.tool);
    panel.classList.add("visible");
    const input = byId("msgInput");
    if (input) input.style.display = "none";
    const first = byId("permPanelActions")?.querySelector("button");
    if (first) first.focus();
    emitFlowEvent("rozsa:interruption-open", { kind: "permission" });
  }

  function renderPermissionCommand(command, tool) {
    if (String(tool).toLowerCase() !== "bash") return escapeHtml(command);
    return '<span class="perm-syn-prompt">$ </span><span class="perm-syn-command">' +
      escapeHtml(command.split(/\s+/)[0]) + "</span>" + escapeHtml(command.slice(command.indexOf(" ") + 1));
  }

  function hidePermission() {
    const panel = byId("permPanel");
    if (panel) panel.classList.remove("visible");
    const input = byId("msgInput");
    if (input) {
      input.style.display = "";
    }
    emitFlowEvent("rozsa:interruption-close", { kind: "permission" });
    const notification = prototypeResponses?.cancel?.permissionClosedNotification;
    if (!notification) throw new Error("Prototype permission cancel response is unavailable");
    showNotification(notification);
  }

  function toast(text) {
    const stack = byId("notificationStack");
    if (!stack) return;
    const item = document.createElement("div");
    item.className = "notification-toast";
    item.textContent = text;
    stack.appendChild(item);
    window.setTimeout(() => item.remove(), 2200);
  }

  function openSettings() {
    if (!settingsPanel) return;
    settingsPanel.classList.add("visible");
    document.body.classList.add("settings-visible");
    switchSettingsTab("appearance");
  }

  function closeSettings() {
    if (!settingsPanel) return;
    settingsPanel.classList.remove("visible");
    document.body.classList.remove("settings-visible");
  }

  function toggleSettings() {
    if (settingsPanel?.classList.contains("visible")) closeSettings();
    else openSettings();
  }

  function switchSettingsTab(tabId, button) {
    all(".settings-tab").forEach(tab => tab.classList.toggle("active", tab.dataset.settingsPane === tabId));
    all(".settings-pane").forEach(pane => pane.classList.toggle("active", pane.id === "pane-" + tabId));
    if (button) button.classList.add("active");
  }

  function toggleSetting(button) {
    const next = button.getAttribute("aria-checked") !== "true";
    button.setAttribute("aria-checked", String(next));
    toast(next ? "Enabled" : "Disabled");
  }

  function toggleThinkingEffortPicker(event) {
    window.RozsaPrototype.popover.toggle(event);
  }

  function syncRangeProgress(input, propertyName) {
    if (!input) return;
    const min = Number(input.min) || 0;
    const max = Number(input.max) || 100;
    const value = Math.min(max, Math.max(min, Number(input.value) || 0));
    const progress = max === min ? 0 : ((value - min) / (max - min)) * 100;
    input.style.setProperty(propertyName, progress + "%");
  }

  function setFontSize(value, persist = true) {
    if (String(value).trim() === "") return;
    const next = Math.min(30, Math.max(5, Number(value) || state.fontSize));
    state.fontSize = next;
    root.style.setProperty("--ui-font-size", next + "px");
    const range = byId("settingsFontSizeRange");
    const input = byId("settingsFontSizeInput");
    if (range) {
      range.value = String(next);
      syncRangeProgress(range, "--appearance-progress");
    }
    if (input) input.value = String(next);
    if (persist) saveState();
  }

  function setThinking(value) {
    const labels = ["Off", "Low", "Medium", "High", "XHigh", "Max"];
    const index = Math.min(labels.length - 1, Math.max(0, Number(value) || 0));
    const label = labels[index];
    window.RozsaPrototype.popover.renderChoice(labels.map(label => ({ label })), index);
    if (byId("thinkingEffort")) byId("thinkingEffort").textContent = label.toLowerCase();
    if (byId("thinkingEffortPickerValue")) byId("thinkingEffortPickerValue").textContent = label;
  }

  function previewThinkingEffort(value) {
    setThinking(value);
  }

  function selectThinkingEffort(value) {
    setThinking(value);
    saveState();
  }

  function addMessage() {
    const input = byId("msgInput");
    if (!input) return;
    const text = input.textContent.trim();
    if (!text) return;
    state.messages.push({ role: "user", text: text });
    state.messages.push({ role: "assistant", text: "I will continue with the current Rózsa GUI structure.", tool: true });
    input.textContent = "";
    renderMessages();
    saveState();
  }

  function newSession() {
    state.sessions.unshift({ name: "Untitled", date: "" });
    state.activeSession = 0;
    state.messages = [];
    renderSessionList();
    renderMessages();
    saveState();
  }

  function setText(id, value) {
    const element = byId(id);
    if (element) element.textContent = value;
  }

  function setToggle(id, enabled) {
    const element = byId(id);
    if (element) element.setAttribute("aria-checked", String(Boolean(enabled)));
  }

  function populateWorkspaceFixture(workspace) {
    if (!workspace?.git || !workspace.quota || !Array.isArray(workspace.toolChips)) {
      throw new Error("Prototype workspace mock is incomplete");
    }
    setText("gitBranch", workspace.git.branch);
    setText("gitAdd", workspace.git.added);
    setText("gitDel", workspace.git.deleted);
    setText("gitFiles", workspace.git.files);
    const quota = byId("quotaGroup");
    if (quota) quota.style.display = workspace.quota.visible ? "" : "none";
    setText("quotaHour", workspace.quota.hour);
    setText("quotaWeek", workspace.quota.week);
    const hourBar = byId("quotaHourBar");
    const weekBar = byId("quotaWeekBar");
    if (hourBar) hourBar.style.width = workspace.quota.hourProgress + "%";
    if (weekBar) weekBar.style.width = workspace.quota.weekProgress + "%";
    const chips = byId("toolChips");
    if (chips) chips.innerHTML = workspace.toolChips.map(({ name, count }) => '<span class="tool-chip"><span class="tool-chip-name">' + escapeHtml(name) +
      '</span><span class="tool-chip-count">' + escapeHtml(count) + "</span></span>").join("");
    setText("contextTokens", workspace.contextTokens);
    setText("thinkingEffort", workspace.thinkingEffort);
    setText("modelSelector", workspace.model);
  }

  function runningPanelMarkup(panel) {
    return sharedConversation.runningPanelMarkup(panel);
  }

  function renderRunningFixture(config) {
    if (!config) throw new Error("Prototype running-state mock is unavailable");
    const panels = Object.fromEntries(["subagentPanel", "forkPicker", "queuedMessages", "steeringConversation"].map(id => [id, runningPanelMarkup(config[id])]));
    Object.entries(panels).forEach(([id, html]) => {
      const panel = byId(id);
      if (!panel) return;
      panel.hidden = !html;
      panel.innerHTML = html;
    });
  }

  function notificationMarkup(item) {
    const severity = item.severity || "info";
    const iconName = severity === "success" ? "circle-check" : (severity === "error" ? "circle-x" : (severity === "warning" ? "triangle-alert" : "info"));
    return '<div class="notification-toast notification-' + severity + '" role="' + (severity === "error" ? "alert" : "status") + '" data-notification-id="' + escapeHtml(item.id) + '">' +
      '<div class="notification-icon" aria-hidden="true">' + lucideIcon(iconName) + '</div><div class="notification-body"><div class="notification-title">' +
      escapeHtml(item.title) + '</div><div class="notification-message">' + escapeHtml(item.message) +
      '</div></div><button class="notification-close" type="button" aria-label="Dismiss notification">' + lucideIcon("x") + '</button></div>';
  }

  function renderNotifications(items = [], errors = []) {
    const stack = byId("notificationStack");
    if (stack) {
      stack.innerHTML = items.map(notificationMarkup).join("");
    }
    const tray = byId("notificationErrorTray");
    const count = byId("notificationErrorCount");
    const list = byId("notificationErrorList");
    if (tray && count && list) {
      tray.hidden = errors.length === 0;
      count.textContent = String(errors.length);
      list.hidden = true;
      list.innerHTML = errors.map(error => '<div class="notification-error-item" role="listitem"><span class="notification-error-item-icon">' + lucideIcon("circle-alert") + '</span><div><div class="notification-error-item-title">' +
        escapeHtml(error.title) + '</div><div class="notification-error-item-message">' + escapeHtml(error.message) + "</div></div></div>").join("");
    }
  }

  function showQuestionDemo(request = questionRequest) {
    if (!request || !Array.isArray(request.options)) throw new Error("Prototype question request is unavailable");
    questionRequest = request;
    const panel = byId("questionPanel");
    const options = byId("questionPanelOptions");
    const otherInput = byId("questionPanelOtherInput");
    const error = byId("questionPanelError");
    if (!panel || !options || !otherInput || !error) return;
    setText("questionPanelTitle", request.title);
    options.innerHTML = request.options.map(({ label, description, other }, index) => '<label class="question-panel-option"><span class="question-panel-option-key">' + (index + 1) +
      '</span><input type="radio" name="question-demo" value="' + escapeHtml(label) + '" data-option-number="' + (index + 1) + '" data-other="' + String(Boolean(other)) + '"><span class="question-panel-option-copy"><span class="question-panel-option-label">' +
      escapeHtml(label) + '</span><span class="question-panel-option-description">' + escapeHtml(description) + "</span></span></label>").join("");
    otherInput.value = "";
    otherInput.hidden = true;
    error.textContent = "";
    all("#questionPanelOptions input").forEach(input => input.addEventListener("change", () => {
      const isOther = input.dataset.other === "true";
      otherInput.hidden = !(isOther && input.checked);
      if (otherInput.hidden) otherInput.value = "";
      error.textContent = "";
      if (!otherInput.hidden) otherInput.focus();
    }));
    otherInput.addEventListener("input", () => { error.textContent = ""; });
    panel.classList.add("visible");
    options.querySelector("input")?.focus();
    emitFlowEvent("rozsa:interruption-open", { kind: "question" });
  }

  function hideQuestionDemo() {
    byId("questionPanel")?.classList.remove("visible");
    emitFlowEvent("rozsa:interruption-close", { kind: "question" });
  }

  function submitQuestionDemo() {
    const panel = byId("questionPanel");
    const selected = panel?.querySelector("input:checked");
    const error = byId("questionPanelError");
    const otherInput = byId("questionPanelOtherInput");
    if (!prototypeResponses?.error || !prototypeResponses.success?.questionSubmittedNotification) {
      throw new Error("Prototype question response mock is unavailable");
    }
    if (!selected) {
      if (error) error.textContent = prototypeResponses.error.questionMissing;
      return;
    }
    if (selected.dataset.other === "true" && !otherInput?.value.trim()) {
      if (error) error.textContent = prototypeResponses.error.questionOtherMissing;
      otherInput?.focus();
      return;
    }
    hideQuestionDemo();
    state.messages.push({ role: "assistant", continuation: true, text: prototypeResponses.success.questionSubmittedText });
    renderMessages();
    renderNotifications([prototypeResponses.success.questionSubmittedNotification]);
  }

  function closeDevFlowDetail() {
    const detail = byId("devFlowDetail");
    if (detail) detail.hidden = true;
    document.body?.classList.remove("dev-flow-detail-open");
    devFlowDetailOpen = false;
  }

  function openDevFlowDetailDemo() {
    if (!devFlowFixture) throw new Error("Prototype Dev Flow fixture is unavailable");
    renderDevFlowFixture(devFlowFixture, { showDetail: true });
    const detail = byId("devFlowDetail");
    if (!detail) return;
    detail.hidden = false;
    document.body?.classList.add("dev-flow-detail-open");
    devFlowDetailOpen = true;
    byId("devFlowDetailClose")?.focus();
  }

  function openDevFlowDashboardDemo() {
    if (!devFlowDashboardNotifications) throw new Error("Prototype Dev Flow dashboard response is unavailable");
    renderNotifications(devFlowDashboardNotifications);
  }

  function renderDevFlowFixture(fixture, { showDetail = false } = {}) {
    if (!fixture?.settings || !fixture.detail) throw new Error("Prototype Dev Flow mock is incomplete");
    devFlowFixture = fixture;
    const missing = byId("devFlowMissing");
    if (missing) missing.hidden = true;
    const ready = byId("devFlowDashboardStatus");
    ready?.classList.add("is-ready");
    setText("devFlowDashboardAvailability", fixture.settings.availability);
    setText("devFlowDashboardAddressText", fixture.settings.address);
    setText("devFlowVersion", fixture.settings.version);
    setText("devFlowMemoryAmount", fixture.settings.memory.amount);
    setText("devFlowMemoryUnit", fixture.settings.memory.unit);
    const address = byId("devFlowDashboardAddress");
    if (address) address.disabled = false;
    const path = byId("devFlowExecutablePath");
    if (path) path.value = fixture.settings.executablePath;
    setToggle("devFlowEnabled", fixture.settings.enabled);
    setToggle("devFlowSidebarStatus", fixture.settings.sidebarEnabled);
    setToggle("devFlowDashboardButton", fixture.settings.dashboardEnabled);
    const detail = byId("devFlowDetail");
    if (detail) {
      detail.hidden = !showDetail;
      document.body?.classList.toggle("dev-flow-detail-open", showDetail);
    }
    setText("devFlowDetailRevision", fixture.detail.revision);
    setText("devFlowDetailProject", fixture.detail.project);
    setText("devFlowDetailSummary", fixture.detail.summary);
    const list = byId("devFlowDetailList");
    if (list) list.innerHTML = fixture.detail.items.map(item => '<div class="dev-flow-detail-item' + (item.status === "in-progress" ? " focus" : "") + '" role="listitem" tabindex="0"><div class="dev-flow-detail-item-head"><span class="dev-flow-detail-item-id">' +
      escapeHtml(item.id) + '</span><span class="dev-flow-detail-item-title">' + escapeHtml(item.title) + '</span><span class="dev-flow-detail-item-status' + (item.status === "in-progress" ? " in-progress" : "") + '">' + escapeHtml(item.status) +
      '</span></div><div class="dev-flow-detail-item-meta">' + escapeHtml(item.meta) + '</div><div class="dev-flow-detail-item-desc">' + escapeHtml(item.description) + '</div><details class="dev-flow-detail-disclosure"><summary>Files (' + item.files.length + ')</summary><div class="dev-flow-detail-disclosure-body">' + item.files.map(file => "<div>" + escapeHtml(file) + "</div>").join("") + '</div></details></div>').join("");
  }

  function renderDevFlowRuntimeFixture(fixture) {
    if (!fixture?.sidebar) throw new Error("Prototype Dev Flow sidebar mock is unavailable");
    closeSettings();
    closeDevFlowDetail();
    renderDevFlowFixture(fixture, { showDetail: false });
    const group = byId("devFlowSidebarGroup");
    const summary = byId("devFlowSidebarSummary");
    const claimed = byId("devFlowSidebarClaimed");
    const more = byId("devFlowSidebarMore");
    const dashboard = byId("sidebarDevFlowDashboard");
    if (!group || !summary || !claimed || !more || !dashboard) return;
    group.hidden = false;
    summary.textContent = fixture.sidebar.summary;
    summary.disabled = false;
    claimed.innerHTML = fixture.sidebar.claimed.map(({ id, title }) => '<button class="dev-flow-claimed-row" type="button" title="' + escapeHtml(title) + '" data-dev-flow-trigger="true"><span class="dev-flow-claimed-dot" aria-hidden="true"></span><span class="dev-flow-claimed-id">' +
      escapeHtml(id) + '</span><span class="dev-flow-claimed-title">' + escapeHtml(title) + "</span></button>").join("");
    more.hidden = false;
    more.textContent = fixture.sidebar.more;
    dashboard.hidden = false;
  }

  function applySceneFixture(scene) {
    const { id, definition, sessions, activeSession, messages, workspace, running, permission, question, settings, notifications, devFlow, responses } = scene;
    state.sessions = sessions;
    state.activeSession = activeSession;
    state.messages = messages;
    permissionRequest = permission;
    questionRequest = question;
    prototypeResponses = responses;
    devFlowDashboardNotifications = notifications?.["dev-flow-dashboard"] || null;

    if (workspace) populateWorkspaceFixture(workspace);
    if (Object.keys(running).length) renderRunningFixture(running);
    if (id === "complete-session") {
      setThinking(definition.initialState.thinkingEffort);
      toggleThinkingEffortPicker({ currentTarget: byId("thinkingEffort") });
    }
    if (id === "notifications") {
      renderNotifications(notifications.notifications, notifications.errors);
      openPermissionDemo(permission);
      return;
    }
    if (id === "permission-request") {
      openPermissionDemo(permission);
      return;
    }
    if (id === "ask-user-question") {
      showQuestionDemo(question);
      return;
    }
    if (id === "dev-flow-runtime") {
      renderDevFlowRuntimeFixture(devFlow);
      renderNotifications(notifications["dev-flow-runtime"]);
      return;
    }
    if (settings?.tab) {
      openSettings();
      switchSettingsTab(settings.tab);
    }
    if (id === "dev-flow-active" || id === "dev-flow") {
      openSettings();
      switchSettingsTab(definition.initialState.settings);
      renderDevFlowFixture(devFlow, { showDetail: false });
      renderNotifications(notifications["dev-flow-ready"]);
    }
  }

  function toggleSidebar() {
    if (appBody) appBody.classList.toggle("sidebar-collapsed");
  }

  function selectThemeModeCard(mode) {
    applyTheme(mode);
  }

  function selectThemeModeCardAlias(mode) {
    selectThemeModeCard(mode);
  }

  function renderKeyBindings(query) {
    const value = String(query || "").toLowerCase();
    all(".shortcut-row").forEach(row => {
      row.hidden = value && !row.textContent.toLowerCase().includes(value);
    });
  }

  function noOpToast(label) {
    return () => toast(label);
  }

  window.toggleSettings = toggleSettings;
  window.closeSettings = closeSettings;
  window.closeDevFlowDetail = closeDevFlowDetail;
  window.openDevFlowDetailDemo = openDevFlowDetailDemo;
  window.openDevFlowDashboardDemo = openDevFlowDashboardDemo;
  window.switchSettingsTab = switchSettingsTab;
  window.newSession = newSession;
  window.doSwitchSession = index => {
    state.activeSession = Number(index);
    renderSessionList();
    renderMessages();
    saveState();
  };
  window.toggleMainSidebar = toggleSidebar;
  window.toggleToolCall = card => card.classList.toggle("expanded");
  window.toggleThinking = header => {
    const block = header.closest(".thinking-block");
    if (!block) return;
    const expanded = block.classList.toggle("expanded");
    header.setAttribute("aria-expanded", String(expanded));
  };
  window.toggleThinkingEffortPicker = toggleThinkingEffortPicker;
  window.toggleTurnDiff = toggleTurnDiff;
  window.showPermission = openPermissionDemo;
  window.respondPermission = hidePermission;
  window.enterPermissionTrust = openPermissionDemo;
  window.enterPermissionHint = () => {
    byId("permPanelMain").hidden = true;
    byId("permPanelHint").hidden = false;
    byId("permHintInput")?.focus();
  };
  window.showPermissionMainPage = () => {
    byId("permPanelMain").hidden = false;
    byId("permPanelHint").hidden = true;
  };
  window.togglePermissionCommand = () => {
    const command = byId("permCmd");
    if (!command) return;
    const expanded = command.classList.toggle("expanded");
    command.classList.toggle("collapsed", !expanded);
    byId("permCmdToggle")?.setAttribute("aria-expanded", String(expanded));
  };
  window.submitPermissionHint = hidePermission;
  window.normalizePermissionHint = input => {
    if (input && !input.value.startsWith("Deny, ")) input.value = "Deny, " + input.value.replace(/^Deny,\s*/i, "");
  };
  window.handlePermissionHintKeydown = event => {
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      hidePermission();
    }
  };
  window.selectThemeModeCard = selectThemeModeCardAlias;
  window.previewThinkingEffort = previewThinkingEffort;
  window.selectThinkingEffort = selectThinkingEffort;
  window.saveThemeAsCustom = mode => toast("Saved " + mode + " theme as custom");
  window.renderKeyBindings = renderKeyBindings;
  window.renderSettingsSelection = switchSettingsTab;
  window.renderSettingsPane = () => {};
  window.renderCapabilitySettings = () => {};
  window.renderPermissionSettings = () => {};
  window.renderDevFlowSettings = () => {};
  window.openPermissionRuleEditor = noOpToast("Permission rule editor opened");
  window.closePermissionRuleEditor = noOpToast("Permission rule editor closed");
  window.savePermissionRule = noOpToast("Permission rule added");
  window.resetPermissionRules = noOpToast("Permission rules restored");
  window.onModelChange = noOpToast("Model changed");
  window.showModelPicker = noOpToast("Model picker is available in the desktop app");
  window.attachFileReference = noOpToast("File attachment is available in the desktop app");
  window.attachDirectoryReference = noOpToast("Folder attachment is available in the desktop app");
  window.insertSlashCommandPrefix = () => {
    const input = byId("msgInput");
    if (input) {
      input.textContent = "/";
      input.focus();
    }
  };
  window.handleInput = () => {};
  window.handleCompositionStart = () => {};
  window.handleCompositionUpdate = () => {};
  window.handleCompositionEnd = () => {};
  window.sendMessage = addMessage;
  window.updateAbortButton = () => {};
  window.showError = toast;
  window.showNotification = toast;
  window.showUserQuestion = showQuestionDemo;
  window.hidePermPanel = hidePermission;
  window.hideQuestionPanel = hideQuestionDemo;
  window.selectQuestionOption = number => {
    const option = byId("questionPanelOptions")?.querySelector('input[data-option-number="' + number + '"]');
    if (!option) return;
    option.checked = true;
    option.dispatchEvent(new Event("change", { bubbles: true }));
  };
  window.submitUserQuestion = submitQuestionDemo;

  window.RozsaPrototype = {
    all,
    byId,
    state,
    applySceneFixture,
    boundModules: new Set(),
    addMessage,
    applyTheme,
    closeDevFlowDetail,
    closeSettings,
    enterPermissionHint: window.enterPermissionHint,
    enterPermissionTrust: window.enterPermissionTrust,
    handleCompositionEnd: window.handleCompositionEnd,
    handleCompositionStart: window.handleCompositionStart,
    handleCompositionUpdate: window.handleCompositionUpdate,
    handleInput: window.handleInput,
    handlePermissionHintKeydown: window.handlePermissionHintKeydown,
    hidePermission,
    hideQuestionDemo,
    insertSlashCommandPrefix: window.insertSlashCommandPrefix,
    lucideIcon,
    newSession,
    noOpToast,
    normalizePermissionHint: window.normalizePermissionHint,
    onModelChange: window.onModelChange,
    openDevFlowDashboardDemo,
    openDevFlowDetailDemo,
    openPermissionDemo,
    openPermissionRuleEditor: window.openPermissionRuleEditor,
    previewThinkingEffort,
    renderKeyBindings,
    renderMessages,
    renderNotifications,
    renderSessionList,
    saveState,
    selectQuestionOption: window.selectQuestionOption,
    selectThemeModeCard,
    selectThinkingEffort,
    setFontSize,
    setThinking,
    showModelPicker: window.showModelPicker,
    showQuestionDemo,
    showPermissionMainPage: window.showPermissionMainPage,
    closePermissionRuleEditor: window.closePermissionRuleEditor,
    resetPermissionRules: window.resetPermissionRules,
    savePermissionRule: window.savePermissionRule,
    submitPermissionHint: window.submitPermissionHint,
    submitQuestionDemo,
    switchSettingsTab,
    togglePermissionCommand: window.togglePermissionCommand,
    toggleSetting,
    toggleThinking: window.toggleThinking,
    toggleThinkingEffortPicker,
    toggleTurnDiff,
    toggleToolCall: window.toggleToolCall,
    toast,
    updateAbortButton: window.updateAbortButton,
    attachFileReference: window.attachFileReference,
    attachDirectoryReference: window.attachDirectoryReference,
    respondPermission: window.respondPermission,
    toggleSettings,
  };

  document.addEventListener("DOMContentLoaded", () => {
    const bootstrap = async () => {
      sharedConversation = await import(new URL("../crates/rozsa-gui/frontend/ui/scripts/features/conversation.js", legacyScriptUrl));
      loadState();
      mountTemplates();
      const popoverModule = await import(new URL("./scripts/components/popover.js", legacyScriptUrl));
      popoverModule.init(window.RozsaPrototype);
      all(".settings-hint").forEach(hint => { hint.innerHTML = lucideIcon("circle-help"); });
      const sceneLoader = await import(new URL("./scripts/core/scene-loader.js", legacyScriptUrl));
      const scene = await sceneLoader.loadPrototypeScene();
      document.documentElement.dataset.rozsaScene = scene.id;
      sceneLoader.applyPrototypeScene(window.RozsaPrototype, scene);
      const isApplicationShell = document.body?.dataset.rozsaEntry === "application-shell";
      if (state.activeSession >= state.sessions.length) state.activeSession = 0;
      setFontSize(state.fontSize, false);
      applyTheme(state.themeMode, false);
      renderSessionList();
      if (!isApplicationShell) renderMessages();
      const sidebar = byId("sidebar");
      if (sidebar) sidebar.removeAttribute("hidden");
      const model = byId("modelSelector");
      if (model) model.textContent = "gpt-5";
      const currentSession = byId("currentSessionName");
      if (currentSession) currentSession.textContent = sessionLabel(state.sessions[state.activeSession]);
      const slider = byId("thinkingEffortSlider");
      if (!scene.definition.initialState.thinkingEffort) setThinking(slider?.value || 0);
      const sceneSettingsTab = scene.settings?.tab || scene.definition.initialState.settings || "appearance";
      all("[data-settings-pane]").forEach(pane => pane.classList.remove("active"));
      byId("pane-" + sceneSettingsTab)?.classList.add("active");
      all(".settings-tab").forEach(tab => tab.classList.toggle("active", tab.dataset.settingsPane === sceneSettingsTab));
      if (isApplicationShell) {
        await loadBehaviorModules();
        emitFlowEvent("rozsa:scene-ready", { scene: scene.id });
      }
    };
    bootstrap().catch(error => {
      console.error("Prototype scene failed to load.", error);
      const shell = document.querySelector("[data-application-shell]");
      if (!shell) return;
      shell.dataset.sceneError = "load-failed";
      const message = document.createElement("p");
      message.setAttribute("role", "alert");
      message.textContent = error.message;
      shell.appendChild(message);
    });
  });

  if (window.matchMedia) {
    window.matchMedia("(prefers-color-scheme: dark)").addEventListener?.("change", () => {
      if (state.themeMode === "system") applyTheme("system", false);
    });
  }
})();
