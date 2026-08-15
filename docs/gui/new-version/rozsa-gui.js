

"use strict";

(() => {
  const STORE_KEY = "rozsa-open-design-gui-v2";
  const root = document.documentElement;
  const appBody = document.querySelector("[data-od-id=app-body]");
  const mainContent = document.getElementById("mainContentScene");
  const settingsPanel = document.getElementById("settingsPanel");
  const settingsContent = document.querySelector(".settings-content");
  const sceneName = document.documentElement.dataset.rozsaScene || "";
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
      dashboard.addEventListener("click", openDevFlowDashboardDemo);
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
    all(".session-item").forEach(item => {
      const activate = () => {
        state.activeSession = Number(item.dataset.sessionIndex);
        renderSessionList();
        renderMessages();
        saveState();
      };
      item.addEventListener("click", activate);
      item.addEventListener("keydown", event => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          activate();
        }
      });
    });
  }

  function textMarkup(value) {
    return escapeHtml(value || "").split(/\n{2,}/).filter(Boolean).map(paragraph =>
      "<p>" + paragraph.replace(/\n/g, "<br>") + "</p>"
    ).join("");
  }

  function messageFrame(role, body, index, continuation = false) {
    const avatar = role === "user" ? "U" : "R";
    const name = role === "user" ? "You" : "Rozsa";
    if (continuation && role === "assistant") {
      return '<div class="msg msg-assistant msg-assistant-continuation" data-od-id="message-' + index + '">' +
        '<div class="msg-body"><div class="msg-content markdown-body">' + body + "</div></div></div>";
    }
    return '<div class="msg msg-' + role + '" data-od-id="message-' + index + '">' +
      '<div class="msg-avatar">' + avatar + '</div><div class="msg-body">' +
      '<div class="msg-role">' + name + '</div><div class="msg-content markdown-body">' + body + "</div></div></div>";
  }

  function toolMarkup(id, name, arg, output, status, options = {}) {
    const stateClass = options.expanded ? " expanded" : "";
    const extraClass = options.className ? " " + options.className : "";
    const body = options.bodyHtml || '<pre style="white-space:pre-wrap;margin:0">' + escapeHtml(output || "") + "</pre>";
    return '<div class="tool-call' + stateClass + extraClass + '" data-tool-call-id="' + escapeHtml(id) + '" data-od-id="tool-call-' + escapeHtml(id) + '" tabindex="0" onclick="toggleToolCall(this)">' +
      '<div class="tool-track"><div class="tool-icon">' + lucideIcon("corner-down-right") + '</div></div>' +
      '<div class="tool-content"><div class="tool-header"><span class="tool-call-status ' + status + '"></span>' +
      '<span class="tool-name">' + escapeHtml(name) + '</span><span class="tool-call-args">' + escapeHtml(arg) +
      '</span><span class="tool-call-toggle">' + lucideIcon("chevron-right") + '</span></div></div>' +
      '<div class="tool-call-body' + (options.bodyClass ? " " + options.bodyClass : "") + '">' + body + "</div></div>";
  }

  function thinkingMarkup(text, expanded = true, active = false, duration = "") {
    return '<div class="thinking-block' + (expanded ? " expanded" : "") + (active ? " active" : "") + '">' +
      '<div class="thinking-header" role="button" tabindex="0" aria-expanded="' + String(expanded) + '" onclick="toggleThinking(this)" onkeydown="if(event.key===\'Enter\'||event.key===\' \'){event.preventDefault();toggleThinking(this)}">' +
      lucideIcon("lightbulb", "thinking-icon") +
      '<span class="thinking-label">' + (active ? "THINKING" : "THINKED") + '</span>' +
      (duration ? '<span class="thinking-duration">' + escapeHtml(duration) + '</span>' : "") +
      '<span class="thinking-chevron">' + lucideIcon("chevron-right") + '</span></div>' +
      '<div class="thinking-content"><div class="thinking-markdown markdown-body">' + textMarkup(text) + "</div></div></div>";
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
      '<button class="change-name" type="button" aria-expanded="false" data-turn-diff="' + escapeHtml(JSON.stringify({ patch: file.patch || "" })) + '" onclick="toggleTurnDiff(this)">' +
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
      chat.innerHTML = '<div class="chat-empty" data-od-id="chat-empty"><div class="chat-empty-icon">R</div>' +
        '<div class="chat-empty-title">Start a new conversation</div>' +
        '<div class="chat-empty-hint">Describe your coding task to Rózsa' +
        '<div class="chat-empty-kbd"><kbd>Enter</kbd> Send <kbd>⇧ Enter</kbd> New line</div></div></div>';
      return;
    }
    chat.innerHTML = state.messages.map((message, index) => messageFrame(
      message.role === "user" ? "user" : "assistant",
      messageBody(message, index),
      index,
      Boolean(message.continuation),
    )).join("");
    all(".tool-call").forEach(card => {
      card.addEventListener("keydown", event => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          toggleToolCall(card);
        }
      });
    });
  }

  function openPermissionDemo() {
    const panel = byId("permPanel");
    if (!panel) return;
    const tool = byId("permTool");
    const desc = byId("permDesc");
    const cmd = byId("permCmd");
    if (tool) tool.textContent = "Bash";
    if (desc) desc.textContent = "Rózsa wants to run a command in the current workspace.";
    if (cmd) cmd.innerHTML = renderPermissionCommand("cargo test -p rozsa-gui", "Bash");
    panel.classList.add("visible");
    const input = byId("msgInput");
    if (input) input.style.display = "none";
    const first = byId("permPanelActions")?.querySelector("button");
    if (first) first.focus();
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
      input.focus();
    }
    showNotification("Permission request closed");
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
    const popover = byId("thinkingEffortPopover");
    const trigger = event && event.currentTarget ? event.currentTarget : byId("thinkingEffort");
    if (!popover || !trigger) return;
    const hidden = popover.hidden;
    popover.hidden = !hidden;
    trigger.setAttribute("aria-expanded", String(hidden));
    if (hidden) {
      const rect = trigger.getBoundingClientRect();
      popover.style.top = Math.min(window.innerHeight - 190, rect.bottom + 6) + "px";
      popover.style.left = Math.min(window.innerWidth - 330, Math.max(16, rect.left - 150)) + "px";
      byId("thinkingEffortSlider")?.focus();
    }
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
    const slider = byId("thinkingEffortSlider");
    if (slider) {
      slider.value = String(index);
      syncRangeProgress(slider, "--thinking-progress");
    }
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

  function populateWorkspaceFixture() {
    setText("gitBranch", "feature/gui-showcase");
    setText("gitAdd", "+18");
    setText("gitDel", "−4");
    setText("gitFiles", "3 files");
    const quota = byId("quotaGroup");
    if (quota) quota.style.display = "";
    setText("quotaHour", "3h 42m");
    setText("quotaWeek", "18h 06m");
    const hourBar = byId("quotaHourBar");
    const weekBar = byId("quotaWeekBar");
    if (hourBar) hourBar.style.width = "36%";
    if (weekBar) weekBar.style.width = "62%";
    const chips = byId("toolChips");
    if (chips) chips.innerHTML = [
      ["Read", "4"], ["Edit", "2"], ["Bash", "3"], ["AskUserQuestion", "1"]
    ].map(([name, count]) => '<span class="tool-chip"><span class="tool-chip-name">' + name +
      '</span><span class="tool-chip-count">' + count + "</span></span>").join("");
    setText("contextTokens", "12.8k");
    setText("thinkingEffort", "high");
    setText("modelSelector", "gpt-5");
  }

  function renderRunningFixture(config = {}) {
    const panels = {
      subagentPanel: config.subagent ? '<div class="running-messages-title">Subagents <span>1</span></div><ol><li>workspace-review · running</li></ol>' : "",
      forkPicker: config.fork ? '<div class="running-messages-title">Fork from this turn</div><ol><li>Review current GUI state</li><li>Continue from verification</li></ol>' : "",
      queuedMessages: config.queue ? '<div class="running-messages-title">Queue <span>2</span></div><ol><li>Check the settings surface</li><li>Summarize changed files</li></ol>' : "",
      steeringConversation: config.steering ? '<div class="running-messages-title">Steering</div><ol><li>Keep the current structure and show the completed state.</li></ol>' : ""
    };
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
      all(".notification-close").forEach(button => button.addEventListener("click", () => button.closest(".notification-toast")?.remove()));
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
      byId("notificationErrorTrayButton")?.addEventListener("click", () => {
        list.hidden = !list.hidden;
        byId("notificationErrorTrayButton")?.setAttribute("aria-expanded", String(!list.hidden));
      });
    }
  }

  function showQuestionDemo() {
    const panel = byId("questionPanel");
    const options = byId("questionPanelOptions");
    const otherInput = byId("questionPanelOtherInput");
    const error = byId("questionPanelError");
    if (!panel || !options || !otherInput || !error) return;
    setText("questionPanelTitle", "[1/1] Which verification should Rózsa run next?");
    options.innerHTML = [
      ["Run the focused GUI checks", "Fast feedback for the current interface changes."],
      ["Open the settings surface", "Review the active configuration before continuing."],
      ["Other", "Type a custom answer."]
    ].map(([label, description], index) => '<label class="question-panel-option"><span class="question-panel-option-key">' + (index + 1) +
      '</span><input type="radio" name="question-demo" value="' + escapeHtml(label) + '" data-option-number="' + (index + 1) + '" data-other="' + String(label === "Other") + '"><span class="question-panel-option-copy"><span class="question-panel-option-label">' +
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
  }

  function hideQuestionDemo() {
    byId("questionPanel")?.classList.remove("visible");
    byId("msgInput")?.focus();
  }

  function submitQuestionDemo() {
    const panel = byId("questionPanel");
    const selected = panel?.querySelector("input:checked");
    const error = byId("questionPanelError");
    const otherInput = byId("questionPanelOtherInput");
    if (!selected) {
      if (error) error.textContent = "Select an option or choose Other.";
      return;
    }
    if (selected.dataset.other === "true" && !otherInput?.value.trim()) {
      if (error) error.textContent = "Type your answer in the Other field.";
      otherInput?.focus();
      return;
    }
    hideQuestionDemo();
    state.messages.push({ role: "assistant", continuation: true, text: "Thanks — I’ll continue with the selected verification path." });
    renderMessages();
    renderNotifications([{ id: "question-complete", severity: "success", title: "Answer received", message: "Rózsa will continue with the selected path." }]);
  }

  function closeDevFlowDetail() {
    const detail = byId("devFlowDetail");
    if (detail) detail.hidden = true;
    document.body?.classList.remove("dev-flow-detail-open");
    devFlowDetailOpen = false;
  }

  function openDevFlowDetailDemo() {
    renderDevFlowFixture({ showDetail: true });
    const detail = byId("devFlowDetail");
    if (!detail) return;
    detail.hidden = false;
    document.body?.classList.add("dev-flow-detail-open");
    devFlowDetailOpen = true;
    byId("devFlowDetailClose")?.focus();
  }

  function openDevFlowDashboardDemo() {
    renderNotifications([{ id: "dev-flow-dashboard", severity: "info", title: "Dashboard", message: "Dev Flow Dashboard is ready at http://127.0.0.1:54122." }]);
  }

  function renderDevFlowFixture({ showDetail = false } = {}) {
    const missing = byId("devFlowMissing");
    if (missing) missing.hidden = true;
    const ready = byId("devFlowDashboardStatus");
    ready?.classList.add("is-ready");
    setText("devFlowDashboardAvailability", "Ready");
    setText("devFlowDashboardAddressText", "http://127.0.0.1:54122");
    setText("devFlowVersion", "0.1.0");
    setText("devFlowMemoryAmount", "42");
    setText("devFlowMemoryUnit", "MB");
    const address = byId("devFlowDashboardAddress");
    if (address) address.disabled = false;
    const path = byId("devFlowExecutablePath");
    if (path) path.value = "/usr/local/bin/dow";
    setToggle("devFlowEnabled", true);
    setToggle("devFlowSidebarStatus", true);
    setToggle("devFlowDashboardButton", true);
    const detail = byId("devFlowDetail");
    if (detail) {
      detail.hidden = !showDetail;
      document.body?.classList.toggle("dev-flow-detail-open", showDetail);
    }
    setText("devFlowDetailRevision", "#18");
    setText("devFlowDetailProject", "rozsa-demo · DEV");
    setText("devFlowDetailSummary", "2 Tasks · 1 Issue");
    const list = byId("devFlowDetailList");
    if (list) list.innerHTML = [
      ["T005", "Build realistic GUI showcase scenarios", "in-progress", "P1 · L · feat"],
      ["T004", "Split current GUI into reusable files", "done", "P1 · M · refactor"],
      ["I002", "Permission panel should remain visible during approval", "open", "P1 · UI"]
    ].map(([id, title, status, meta]) => '<div class="dev-flow-detail-item' + (status === "in-progress" ? " focus" : "") + '" role="listitem" tabindex="0"><div class="dev-flow-detail-item-head"><span class="dev-flow-detail-item-id">' +
      id + '</span><span class="dev-flow-detail-item-title">' + escapeHtml(title) + '</span><span class="dev-flow-detail-item-status' + (status === "in-progress" ? " in-progress" : "") + '">' + status +
      '</span></div><div class="dev-flow-detail-item-meta">' + escapeHtml(meta) + '</div><div class="dev-flow-detail-item-desc">Current project state is visible from the active Dev Flow integration.</div><details class="dev-flow-detail-disclosure"><summary>Files (2)</summary><div class="dev-flow-detail-disclosure-body"><div>MODIFY: rozsa-gui.js</div><div>CREATE: scenes/complete-session.html</div></div></details></div>').join("");
  }

  function renderDevFlowRuntimeFixture() {
    closeSettings();
    closeDevFlowDetail();
    renderDevFlowFixture({ showDetail: false });
    const group = byId("devFlowSidebarGroup");
    const summary = byId("devFlowSidebarSummary");
    const claimed = byId("devFlowSidebarClaimed");
    const more = byId("devFlowSidebarMore");
    const dashboard = byId("sidebarDevFlowDashboard");
    if (!group || !summary || !claimed || !more || !dashboard) return;
    group.hidden = false;
    summary.textContent = "2 tasks · 1 issue";
    summary.disabled = false;
    summary.onclick = openDevFlowDetailDemo;
    claimed.innerHTML = [
      ["T007", "Fix Dev Flow showcase interactions"],
      ["T004", "Split current GUI into reusable files"]
    ].map(([id, title]) => '<button class="dev-flow-claimed-row" type="button" title="' + escapeHtml(title) + '" data-dev-flow-trigger="true"><span class="dev-flow-claimed-dot" aria-hidden="true"></span><span class="dev-flow-claimed-id">' +
      id + '</span><span class="dev-flow-claimed-title">' + escapeHtml(title) + "</span></button>").join("");
    all("#devFlowSidebarClaimed [data-dev-flow-trigger]").forEach(row => { row.onclick = openDevFlowDetailDemo; });
    more.hidden = false;
    more.textContent = "more 1";
    more.onclick = openDevFlowDetailDemo;
    dashboard.hidden = false;
  }

  function completeSessionMessages() {
    return [
      { role: "user", text: "Review the current GUI, run the checks, and keep the existing interaction model intact." },
      { role: "assistant", thinking: "I traced the current GUI from the message stream through the tool timeline, permission surface, settings scenes, and Dev Flow status. The showcase should expose those existing states without introducing a parallel visual system.", thinkingDuration: "4.8s", text: "The current interface already has the pieces needed for a complete agent session.", tools: [
        { id: "read-gui", name: "Read", args: "crates/rozsa-gui/frontend/index.html", kind: "code", content: "<main data-od-id=\"main-panel\">\n  <div class=\"chat-messages\">\n  <div class=\"input-wrapper\">", output: "Current GUI structure loaded" },
        { id: "edit-adapter", name: "Edit", args: "rozsa-gui.js", kind: "diff", patch: "@@ -1,3 +1,5 @@\n const root = document.documentElement;\n+const sceneName = root.dataset.rozsaScene || '';\n+applySceneFixture();\n const state = {};", output: "Updated shared scene adapter" },
        { id: "bash-check", name: "Bash", args: "node --check rozsa-gui.js", output: "\n> rozsa-gui@0.1.0 check\n> node --check rozsa-gui.js\n\nProcess exited with code 0", className: "dev-flow-tool-call" }
      ] },
      { role: "assistant", continuation: true, text: "The adapter preserves the current shell and only changes the fixture state for this showcase.", changes: { files: [
        { path: "rozsa-gui.js", status: "modified", added: 148, deleted: 12, patch: "@@ -330,2 +330,5 @@\n function applySceneFixture() {\n+  if (sceneName === 'complete-session') {\n+    populateWorkspaceFixture();\n+  }" },
        { path: "scenes/complete-session.html", status: "added", added: 431, deleted: 0, patch: "@@ -1,0 +1,3 @@\n+<!doctype html>\n+<html data-rozsa-scene=\"complete-session\">\n+</html>" }
      ], verification: { command: "node --check rozsa-gui.js", exitCode: 0, duration: "82ms" } } },
      { role: "user", text: "Run the final verification and summarize the current state." },
      { role: "assistant", thinking: "The structure check is complete. I am recording the result as a finished agent turn.", thinkingDuration: "1.2s", text: "All checks passed. The current workspace has a complete conversation state, visible tool evidence, a file-change summary, and a ready-to-continue composer." }
    ];
  }

  function applySceneFixture() {
    if (sceneName === "empty-session") {
      state.messages = [];
      return;
    }
    if (sceneName === "complete-session") {
      state.sessions = [{ name: "GUI review and verification", date: "now" }, { name: "Permission flow", date: "10m ago" }, { name: "Dev Flow setup", date: "yesterday" }];
      state.messages = completeSessionMessages();
      populateWorkspaceFixture();
      renderRunningFixture({ subagent: true, fork: true, queue: true, steering: true });
      setThinking("3");
      toggleThinkingEffortPicker({ currentTarget: byId("thinkingEffort") });
      return;
    }
    if (sceneName === "notifications") {
      state.messages = [{ role: "user", text: "Run the verification command in the current workspace." }, { role: "assistant", text: "I need approval before I can run the command." }];
      populateWorkspaceFixture();
      renderRunningFixture({ queue: true, steering: true });
      renderNotifications([
        { id: "run-started", severity: "info", title: "Command started", message: "Preparing the verification request." },
        { id: "workspace-updated", severity: "success", title: "Workspace indexed", message: "3 files are ready for review." },
        { id: "approval-needed", severity: "warning", title: "Approval needed", message: "Rózsa is waiting for permission to continue." }
      ], [{ title: "Verification could not start", message: "The command is waiting for user approval." }]);
      openPermissionDemo();
      return;
    }
    if (sceneName === "ask-user-question") {
      state.messages = [{ role: "user", text: "Prepare the next verification step." }, { role: "assistant", thinking: "There are two valid ways to verify this state. I should ask before choosing one.", thinkingDuration: "2.1s", text: "I need one decision before continuing.", tools: [{ id: "ask-question", name: "AskUserQuestion", args: "1 question · waiting for answer", output: "The separate question panel is awaiting your answer.", expanded: true }] }];
      populateWorkspaceFixture();
      renderRunningFixture({ subagent: true });
      showQuestionDemo();
      return;
    }
    if (sceneName === "dev-flow-runtime") {
      state.sessions = [{ name: "Dev Flow active work", date: "now" }, { name: "GUI review and verification", date: "10m ago" }];
      state.messages = [
        { role: "user", text: "Continue the active Dev Flow work and show the project status." },
        { role: "assistant", text: "Dev Flow is active in the main workspace. The sidebar shows claimed work and the current project status.", tools: [
          { id: "dev-flow-status", name: "Bash", args: "dow status", output: "phase DEV · 2 tasks · 1 issue · dashboard ready", className: "dev-flow-tool-call", expanded: true }
        ] },
        { role: "assistant", continuation: true, text: "Click a task in the sidebar to inspect the read-only work details. The detail panel can be closed with ×, Escape, or by clicking outside it." }
      ];
      populateWorkspaceFixture();
      renderRunningFixture({ queue: true, steering: true });
      renderDevFlowRuntimeFixture();
      renderNotifications([{ id: "dev-flow-runtime", severity: "success", title: "Dev Flow active", message: "Project status is available from the main workspace." }]);
      return;
    }
    if (sceneName === "dev-flow-active" || sceneName === "dev-flow") {
      state.messages = [{ role: "assistant", text: "Dev Flow is active for this project. The current stage and open work are available in Settings." }];
      populateWorkspaceFixture();
      openSettings();
      switchSettingsTab("dev-flow");
      renderDevFlowFixture({ showDetail: false });
      renderNotifications([{ id: "dev-flow-ready", severity: "success", title: "Dev Flow connected", message: "Project status and dashboard actions are available." }]);
      return;
    }
    if (sceneName === "tool-calls" || sceneName === "permission-request") {
      state.messages = [
        { role: "user", text: "Review the current GUI implementation." },
        { role: "assistant", text: "I inspected the current GUI structure.", tools: [
          { id: "read-current-gui", name: "Read", args: "crates/rozsa-gui/frontend/index.html", output: "430 lines read · structure preserved", expanded: true },
          { id: "bash-current-check", name: "Bash", args: "node --check rozsa-gui.js", output: "Process exited with code 0", className: "dev-flow-tool-call", expanded: true }
        ] }
      ];
      if (sceneName === "permission-request") openPermissionDemo();
      return;
    }
    if (sceneName === "settings-appearance" || sceneName === "permissions" || sceneName === "dev-flow") {
      state.messages = [];
      openSettings();
      switchSettingsTab(sceneName === "settings-appearance" ? "appearance" : sceneName);
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

  document.addEventListener("click", event => {
    const target = event.target.closest("[data-settings-theme-mode-card]");
    if (target) selectThemeModeCard(target.dataset.settingsThemeModeCard);
  });

  document.addEventListener("DOMContentLoaded", () => {
    loadState();
    mountTemplates();
    all(".settings-hint").forEach(hint => { hint.innerHTML = lucideIcon("circle-help"); });
    applySceneFixture();
    if (state.activeSession >= state.sessions.length) state.activeSession = 0;
    setFontSize(state.fontSize, false);
    applyTheme(state.themeMode, false);
    renderSessionList();
    renderMessages();
    const sidebar = byId("sidebar");
    if (sidebar) sidebar.removeAttribute("hidden");
    const model = byId("modelSelector");
    if (model) model.textContent = "gpt-5";
    const currentSession = byId("currentSessionName");
    if (currentSession) currentSession.textContent = sessionLabel(state.sessions[state.activeSession]);
    if (!sceneName) {
      setText("gitBranch", "—");
      setText("gitAdd", "—");
      setText("gitDel", "—");
      setText("gitFiles", "—");
    }
    const send = byId("send-btn");
    if (send) send.onclick = addMessage;
    const slider = byId("thinkingEffortSlider");
    setThinking(slider?.value || 0);
    slider?.addEventListener("input", event => setThinking(event.target.value));
    byId("settingsFontSizeRange")?.addEventListener("input", event => setFontSize(event.target.value));
    byId("settingsFontSizeInput")?.addEventListener("input", event => setFontSize(event.target.value));
    const sceneSettingsTab = sceneName === "settings-appearance" ? "appearance" :
      (sceneName === "permissions" ? "permissions" : ((sceneName === "dev-flow-active" || sceneName === "dev-flow") ? "dev-flow" : "appearance"));
    all("[data-settings-pane]").forEach(pane => pane.classList.remove("active"));
    byId("pane-" + sceneSettingsTab)?.classList.add("active");
    all(".settings-tab").forEach(tab => tab.classList.toggle("active", tab.dataset.settingsPane === sceneSettingsTab));
    all(".setting-toggle").forEach(button => {
      if (!button.dataset.boundLocalToggle) {
        button.dataset.boundLocalToggle = "true";
        button.addEventListener("click", () => toggleSetting(button));
      }
    });
    byId("devFlowDetailClose")?.addEventListener("click", closeDevFlowDetail);
    document.addEventListener("pointerdown", event => {
      if (!devFlowDetailOpen || event.target.closest("#devFlowDetail")) return;
      closeDevFlowDetail();
    });
    document.addEventListener("keydown", event => {
      if (devFlowDetailOpen && event.key === "Escape") {
        event.preventDefault();
        closeDevFlowDetail();
      }
    });
  });

  if (window.matchMedia) {
    window.matchMedia("(prefers-color-scheme: dark)").addEventListener?.("change", () => {
      if (state.themeMode === "system") applyTheme("system", false);
    });
  }
})();
