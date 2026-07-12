// 会话数据（模拟）
const sessions = [
  { name: '重构 agent_loop.rs', state: 'running', time: '2h' },
  { name: '修复权限面板布局', state: 'unread', time: '1d' },
  { name: '添加 subagent 工具支持', state: 'approval', time: '3d' },
  { name: 'models.json 迁移', state: 'unread', time: '7d' }
];

// 切换会话
function switchSession(el, idx) {
  document.querySelectorAll('.session-item').forEach(s => s.classList.remove('active'));
  el.classList.add('active');
  document.getElementById('currentSessionName').textContent = sessions[idx].name;
}

// 新建会话
function newSession() {
  const list = document.getElementById('sessionList');
  const item = document.createElement('div');
  item.className = 'session-item active';
  item.setAttribute('tabindex', '0');
  item.innerHTML = '<span class="session-status running" title="Running"></span><div class="session-name">New session</div><div class="session-meta"><span>Just now</span></div>';
  document.querySelectorAll('.session-item').forEach(s => s.classList.remove('active'));
  list.prepend(item);
  item.onclick = function() { switchSession(this, -1); };
  document.getElementById('currentSessionName').textContent = 'New session';
  document.getElementById('chatMessages').innerHTML = '';
  showEmptyState(true);
  // 重新添加 typing 指示器
  const typing = document.createElement('div');
  typing.className = 'msg msg-assistant';
  typing.id = 'typingMsg';
  typing.style.display = 'none';
  typing.innerHTML = '<div class="msg-avatar">R</div><div class="msg-body"><div class="msg-role">Rózsa</div><div class="typing-indicator"><span></span><span></span><span></span></div></div>';
  document.getElementById('chatMessages').appendChild(typing);
}

// 空状态管理
function showEmptyState(show) {
  const el = document.getElementById('emptyState');
  if (el) el.style.display = show ? 'flex' : 'none';
}

// 展开/收起工具调用
function toggleToolCall(el) {
  el.classList.toggle('expanded');
}

let prototypeInputComposing = false;

function getPrototypeInputText(input) {
  return (input?.innerText || '').replace(/\u00a0/g, '');
}

function setPrototypeInputText(input, text) {
  if (!input) return;
  input.textContent = text;
}

function handlePrototypeInput(input) {
  autoResize(input);
  if (!prototypeInputComposing) updateAutocomplete();
}

function handlePrototypeCompositionStart() {
  prototypeInputComposing = true;
}

function handlePrototypeCompositionUpdate(input) {
  autoResize(input);
}

function handlePrototypeCompositionEnd(input) {
  prototypeInputComposing = false;
  autoResize(input);
  updateAutocomplete();
}

// 发送消息
function sendMessage() {
  const input = document.getElementById('msgInput');
  const text = getPrototypeInputText(input).trim();
  if (!text) return;

  const messages = document.getElementById('chatMessages');
  const typing = document.getElementById('typingMsg');

  // 添加用户消息
  showEmptyState(false);
  const msgDiv = document.createElement('div');
  msgDiv.className = 'msg msg-user msg-enter';
  msgDiv.innerHTML = `<div class="msg-avatar">U</div><div class="msg-body"><div class="msg-role">你</div><div class="msg-content markdown-body">${renderMarkdown(text)}</div></div>`;
  messages.insertBefore(msgDiv, typing);

  // 清空输入框
  setPrototypeInputText(input, '');
  input.style.height = 'auto';
  hideAutocomplete(document.getElementById('autocomplete'));

  // 显示打字指示器
  typing.style.display = 'flex';
  messages.scrollTop = messages.scrollHeight;

  // 模拟流式响应
  setTimeout(() => {
    typing.style.display = 'none';
    const reply = document.createElement('div');
    reply.className = 'msg msg-assistant msg-enter';
    reply.innerHTML = `<div class="msg-avatar">R</div><div class="msg-body"><div class="msg-role">Rózsa</div>
      <div class="thinking-block active" data-auto="true">
        <div class="thinking-header" onclick="toggleThinkingBlock(this)">
          <svg class="thinking-icon" width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="M8 1.5C5 1.5 3 3.5 3 6c0 1.5.8 2.7 2 3.5V12a1 1 0 001 1h4a1 1 0 001-1V9.5c1.2-.8 2-2 2-3.5 0-2.5-2-4.5-5-4.5z"/><path d="M6 14.5h4"/></svg>
          <span class="thinking-label">思考中</span>
          <span class="thinking-duration"></span>
          <span class="thinking-chevron">▸</span>
        </div>
        <div class="thinking-content"></div>
      </div>
      <div class="msg-content"><div class="markdown-body reply-markdown"><span class="stream-text"></span><span class="stream-cursor">▌</span></div></div></div>`;
    messages.insertBefore(reply, typing);

    // Thinking 阶段 — 自动展开 + 流式
    const thinkStart = Date.now();
    const thinkBlock = reply.querySelector('.thinking-block');
    const thinkLabel = thinkBlock.querySelector('.thinking-label');
    const thinkDuration = thinkBlock.querySelector('.thinking-duration');
    thinkBlock.classList.add('expanded');
    const thinkContent = thinkBlock.querySelector('.thinking-content');
    const thinkText = '检查发布条件：版本号、依赖状态、变更日志…';
    let ti = 0;
    const thinkInterval = setInterval(() => {
      if (ti < thinkText.length) {
        thinkContent.textContent += thinkText[ti];
        ti++;
        messages.scrollTop = messages.scrollHeight;
      } else {
        clearInterval(thinkInterval);
        const elapsed = ((Date.now() - thinkStart) / 1000).toFixed(1);
        thinkBlock.classList.remove('active');
        thinkLabel.textContent = 'Thinked';
        thinkDuration.textContent = elapsed + 's';
        // 未手动展开则自动折叠
        if (thinkBlock.dataset.auto === 'true') {
          thinkBlock.classList.remove('expanded');
        }
        delete thinkBlock.dataset.auto;

        // 用户选择后继续：先注册回调，再显示面板，避免面板渲染异常时按钮拿不到处理函数。
        window._continueAfterPerm = function() {
          hidePermPanel();
          // 创建工具调用
          const toolDiv = document.createElement('div');
          toolDiv.className = 'tool-call expanded';
          toolDiv.setAttribute('onclick', 'toggleToolCall(this)');
          toolDiv.innerHTML = `<div class="tool-track">
            <div class="tool-icon"><svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="4 5.5 1.5 8 4 10.5"/><line x1="8" y1="10" x2="13" y2="10"/></svg></div>
            <span class="tool-name">Bash</span>
          </div>
          <div class="tool-content">
            <div class="tool-header">
              <span class="tool-call-status s-running"></span>
              <span class="tool-call-args">cargo publish --dry-run --package rozsa-core</span>
              <span class="tool-call-toggle">></span>
            </div>
          </div>
          <div class="tool-call-body tool-output"><div class="tool-output-steps"><div class="tool-step step-done"><span class="step-icon">✓</span><span class="step-text">Compiling <span class="step-pkg">rozsa-model</span> v0.3.0</span></div></div></div>`;
          reply.querySelector('.msg-content').prepend(toolDiv);

          // 工具调用完成后更新（> 1s 才显示用时）
          const toolStatus = toolDiv.querySelector('.tool-call-status');
          const toolHeader = toolDiv.querySelector('.tool-header');
          setTimeout(() => {
            toolStatus.className = 'tool-call-status s-success';
            const dur = document.createElement('span');
            dur.className = 'tool-call-duration';
            dur.textContent = '1.2s';
            toolHeader.insertBefore(dur, toolHeader.querySelector('.tool-call-toggle'));
          }, 1200);

          // 流式输出主消息
          const text = '## 发布前检查完成\n\nDry-run 检查通过，所有依赖均满足发布条件。\n\n- 版本号与变更日志已对齐\n- workspace 依赖解析正常\n- 发布前检查命令：`cargo publish --dry-run --package rozsa-core`\n\n> 建议在真正发布前再跑一次完整测试，并确认权限模式仍为 `on-request`。\n\n```bash\ncargo test --workspace\ncargo publish --dry-run --package rozsa-core\n```\n\n参考：[Rust 发布检查清单](https://doc.rust-lang.org/cargo/reference/publishing.html)。';
          const streamEl = reply.querySelector('.stream-text');
          const cursorEl = reply.querySelector('.stream-cursor');
          const markdownEl = reply.querySelector('.reply-markdown');
          let i = 0;
          const interval = setInterval(() => {
            if (i < text.length) {
              streamEl.textContent += text[i];
              i++;
              messages.scrollTop = messages.scrollHeight;
            } else {
              clearInterval(interval);
              markdownEl.innerHTML = renderMarkdown(text);
              toolDiv.classList.remove('expanded');
            }
          }, 25);
        };

        // 显示权限审批面板（替换输入框）
        showPermPanel();
      }
    }, 25);
  }, 600);
}

// 权限审批面板
function showPermPanel() {
  const panel = document.getElementById('permPanel');
  const input = document.getElementById('msgInput');
  panel.classList.add('visible');
  input.style.display = 'none';
  const firstAction = panel.querySelector('.perm-panel-opt');
  if (firstAction) firstAction.focus();
  const messages = document.getElementById('chatMessages');
  if (messages) messages.scrollTop = messages.scrollHeight;
}
function hidePermPanel() {
  const panel = document.getElementById('permPanel');
  const input = document.getElementById('msgInput');
  panel.classList.remove('visible');
  input.style.display = '';
  input.focus();
}

// 自动调整输入框高度
function autoResize(el) {
  el.style.height = 'auto';
  el.style.height = Math.min(el.scrollHeight, 120) + 'px';
}

// 键盘快捷键
document.addEventListener('keydown', function(e) {
  const input = document.getElementById('msgInput');
  if (prototypeInputComposing || e.isComposing || e.keyCode === 229) return;
  // 自动补全导航
  if (acVisible && document.activeElement === input) {
    const popup = document.getElementById('autocomplete');
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      acSelected = Math.min(acSelected + 1, acItems.length - 1);
      updateAcSelection(popup);
      return;
    }
    if (e.key === 'ArrowUp') {
      e.preventDefault();
      acSelected = Math.max(acSelected - 1, 0);
      updateAcSelection(popup);
      return;
    }
    if (e.key === 'Tab' || (e.key === 'Enter' && !e.shiftKey)) {
      if (acItems.length > 0) {
        e.preventDefault();
        selectAcItem(acSelected);
        return;
      }
    }
    if (e.key === 'Escape') {
      hideAutocomplete(popup);
      return;
    }
  }
  // Ctrl+T 展开/折叠 thinking 内容
  if (e.key === 't' && e.ctrlKey && !e.shiftKey && !e.altKey) {
    e.preventDefault();
    document.body.classList.toggle('thinking-expanded');
    return;
  }
  // Enter 发送（不含 Shift）
  if (e.key === 'Enter' && !e.shiftKey && document.activeElement === input) {
    e.preventDefault();
    sendMessage();
  }
  // 权限面板快捷键
  const permPanel = document.getElementById('permPanel');
  if (permPanel.classList.contains('visible')) {
    const key = e.key.toLowerCase();
    if (key === 'y') {
      e.preventDefault();
      window._continueAfterPerm && window._continueAfterPerm();
      return;
    }
    if (key === 't') {
      e.preventDefault();
      window._continueAfterPerm && window._continueAfterPerm();
      return;
    }
    if (key === 'n') {
      e.preventDefault();
      hidePermPanel();
      return;
    }
    if (key === 'a') {
      e.preventDefault();
      hidePermPanel();
      return;
    }
  }
  // Escape 关闭弹窗
  if (e.key === 'Escape') {
    closeSettings();
  }
});

function updateAcSelection(popup) {
  popup.querySelectorAll('.ac-item').forEach((el, i) => {
    el.classList.toggle('selected', i === acSelected);
  });
}

/*
 * Rózsa GUI prototype shared interactions.
 * Entry: docs/gui/prototype/index.html
 * Scenes: #app (default) and #settings
 */

// Settings scene prototype
const prototypeThemes = {
  light: { accent: '#D7827E', background: '#FFFFFF', foreground: '#575279' },
  dark: { accent: '#D88991', background: '#1D1A1C', foreground: '#F1E9EB' }
};

function resolvedPreviewTheme(mode) {
  return mode === 'dark' || (mode === 'system' && window.matchMedia?.('(prefers-color-scheme: dark)')?.matches)
    ? 'dark'
    : 'light';
}

function toggleSettings() {
  const panel = document.getElementById('settingsPanel');
  const visible = panel.classList.toggle('visible');
  document.body.classList.toggle('settings-layer-open', visible);
  if (visible) {
    const main = document.querySelector('.settings-main');
    const sidebar = document.querySelector('.settings-sidebar');
    if (main) main.scrollTop = 0;
    if (sidebar) sidebar.scrollTop = 0;
    switchSettingsPage('appearance');
    const title = document.querySelector('#page-appearance .settings-page-title');
    if (title) title.focus({ preventScroll: true });
    if (main) main.scrollTop = 0;
  }
}

function closeSettings() {
  document.getElementById('settingsPanel').classList.remove('visible');
  document.body.classList.remove('settings-layer-open');
}

function switchSettingsPage(page, button) {
  document.querySelectorAll('.settings-nav-item').forEach(item => item.classList.remove('active'));
  document.querySelectorAll('.settings-page').forEach(item => item.classList.remove('active'));
  if (button) button.classList.add('active');
  else {
    const active = document.querySelector(`.settings-nav-item[onclick*="'${page}'"]`);
    if (active) active.classList.add('active');
  }
  const pane = document.getElementById('page-' + page);
  if (pane) pane.classList.add('active');
}

function filterSettingsNav(query) {
  const needle = query.trim().toLowerCase();
  document.querySelectorAll('.settings-nav-group').forEach(group => {
    const items = [...group.querySelectorAll('.settings-nav-item')];
    const matching = items.filter(item => item.textContent.toLowerCase().includes(needle));
    items.forEach(item => { item.hidden = needle !== '' && !matching.includes(item); });
    group.hidden = needle !== '' && matching.length === 0;
  });
}

function setThemeMode(mode) {
  document.querySelectorAll('[data-theme-mode-card]').forEach(card => {
    const active = card.dataset.themeModeCard === mode;
    card.classList.toggle('active', active);
    card.setAttribute('aria-pressed', active ? 'true' : 'false');
  });
  document.documentElement.dataset.previewTheme = mode;
  const resolved = resolvedPreviewTheme(mode);
  document.documentElement.dataset.previewResolvedTheme = resolved;
  applyPrototypeTheme(prototypeThemes[resolved], resolved);
}

function syncFontSize(value) {
  const size = Math.min(50, Math.max(5, Number.parseInt(value, 10) || 14));
  document.getElementById('prototypeFontSize').value = size;
  document.getElementById('prototypeFontSizeValue').value = size;
  document.documentElement.style.setProperty('--ui-font-size', size + 'px');
}

function normalizeHex(value, fallback) {
  const normalized = value.trim().toUpperCase();
  return /^#[0-9A-F]{6}$/.test(normalized) ? normalized : fallback;
}

function hexToHsv(hex) {
  const value = hex.slice(1);
  const r = Number.parseInt(value.slice(0, 2), 16) / 255;
  const g = Number.parseInt(value.slice(2, 4), 16) / 255;
  const b = Number.parseInt(value.slice(4, 6), 16) / 255;
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const delta = max - min;
  let hue = 0;
  if (delta !== 0) {
    if (max === r) hue = 60 * (((g - b) / delta) % 6);
    else if (max === g) hue = 60 * ((b - r) / delta + 2);
    else hue = 60 * ((r - g) / delta + 4);
  }
  if (hue < 0) hue += 360;
  return { hue, saturation: max === 0 ? 0 : delta / max, value: max };
}

function hsvToHex(hue, saturation, value) {
  const chroma = value * saturation;
  const x = chroma * (1 - Math.abs((hue / 60) % 2 - 1));
  const match = value - chroma;
  let rgb = [0, 0, 0];
  if (hue < 60) rgb = [chroma, x, 0];
  else if (hue < 120) rgb = [x, chroma, 0];
  else if (hue < 180) rgb = [0, chroma, x];
  else if (hue < 240) rgb = [0, x, chroma];
  else if (hue < 300) rgb = [x, 0, chroma];
  else rgb = [chroma, 0, x];
  return '#' + rgb.map(channel => Math.round((channel + match) * 255).toString(16).padStart(2, '0')).join('').toUpperCase();
}

function colorTextColor(hex) {
  const value = hex.slice(1);
  const r = Number.parseInt(value.slice(0, 2), 16);
  const g = Number.parseInt(value.slice(2, 4), 16);
  const b = Number.parseInt(value.slice(4, 6), 16);
  return (r * 299 + g * 587 + b * 114) / 1000 > 155 ? '#312A2D' : '#FFFFFF';
}

function colorControlPrefix(theme, field) {
  return theme + field.charAt(0).toUpperCase() + field.slice(1);
}

function updateColorPicker(theme, field) {
  const hex = prototypeThemes[theme][field];
  const prefix = colorControlPrefix(theme, field);
  const chip = document.getElementById(prefix + 'Chip');
  const popover = document.getElementById(prefix + 'Popover');
  const hueInput = document.getElementById(prefix + 'Hue');
  const current = document.getElementById(prefix + 'Current');
  const pickerValue = document.getElementById(prefix + 'PickerValue');
  if (!chip) return;
  const hsv = hexToHsv(hex);
  chip.style.setProperty('--chip-color', hex);
  chip.style.setProperty('--chip-text', colorTextColor(hex));
  if (popover) {
    popover.style.setProperty('--picker-hue', hsv.hue);
    popover.style.setProperty('--picker-x', (hsv.saturation * 100) + '%');
    popover.style.setProperty('--picker-y', ((1 - hsv.value) * 100) + '%');
    popover.style.setProperty('--picker-color', hex);
  }
  if (hueInput) hueInput.value = Math.round(hsv.hue);
  if (current) current.style.background = hex;
  if (pickerValue) pickerValue.textContent = hex;
}

function toggleColorPicker(theme, field) {
  const prefix = colorControlPrefix(theme, field);
  const popover = document.getElementById(prefix + 'Popover');
  const wasOpen = popover && !popover.hidden;
  document.querySelectorAll('.theme-picker-popover').forEach(item => { item.hidden = true; });
  if (popover && !wasOpen) {
    updateColorPicker(theme, field);
    popover.hidden = false;
  }
}

function setColorPickerHue(theme, field, value) {
  const prefix = colorControlPrefix(theme, field);
  const popover = document.getElementById(prefix + 'Popover');
  if (popover) popover.style.setProperty('--picker-hue', value);
  const hsv = hexToHsv(prototypeThemes[theme][field]);
  const nextHex = hsvToHex(Number(value), hsv.saturation, hsv.value);
  syncColorControl(theme, field, 'picker', nextHex);
}

function pickThemeColor(event, theme, field) {
  const surface = event.currentTarget;
  const rect = surface.getBoundingClientRect();
  const saturation = Math.min(1, Math.max(0, (event.clientX - rect.left) / rect.width));
  const value = Math.min(1, Math.max(0, 1 - (event.clientY - rect.top) / rect.height));
  const prefix = colorControlPrefix(theme, field);
  const hueInput = document.getElementById(prefix + 'Hue');
  const hue = hueInput ? Number(hueInput.value) : hexToHsv(prototypeThemes[theme][field]).hue;
  syncColorControl(theme, field, 'picker', hsvToHex(hue, saturation, value));
}

function syncColorControl(theme, field, source, value) {
  const fallback = prototypeThemes[theme][field];
  const normalized = value.trim().toUpperCase();
  if (!/^#[0-9A-F]{6}$/.test(normalized)) return;
  const hex = normalizeHex(normalized, fallback);
  const prefix = colorControlPrefix(theme, field);
  const input = document.getElementById(prefix + 'Hex');
  if (source === 'picker' && input) input.value = hex;
  prototypeThemes[theme][field] = hex;
  updateColorPicker(theme, field);
  if (document.documentElement.dataset.previewResolvedTheme === theme) {
    applyPrototypeTheme(prototypeThemes[theme], theme);
  }
}

function applyPrototypeTheme(theme, mode) {
  const resolved = mode || document.documentElement.dataset.previewResolvedTheme || 'light';
  document.documentElement.style.setProperty('--accent', theme.accent);
  document.documentElement.style.setProperty('--semantic-accent', theme.accent);
  document.documentElement.style.setProperty('--bg', theme.background);
  document.documentElement.style.setProperty('--app-body-bg', theme.background);
  document.documentElement.style.setProperty('--fg', theme.foreground);
  document.documentElement.dataset.previewResolvedTheme = resolved;
}

function selectThemeProfile(theme, profile) {
  if (profile === 'rose-pine' || profile === 'rose-pine-dark') {
    const values = theme === 'light'
      ? { accent: '#D7827E', background: '#FAF4ED', foreground: '#575279' }
      : { accent: '#EBBCBA', background: '#191724', foreground: '#E0DEF4' };
    Object.assign(prototypeThemes[theme], values);
    ['accent', 'background', 'foreground'].forEach(field => {
      const prefix = colorControlPrefix(theme, field);
      document.getElementById(prefix + 'Hex').value = values[field];
      updateColorPicker(theme, field);
    });
  }
  if (document.documentElement.dataset.previewResolvedTheme === theme) applyPrototypeTheme(prototypeThemes[theme], theme);
}

setThemeMode('system');
['light', 'dark'].forEach(theme => ['accent', 'background', 'foreground'].forEach(field => updateColorPicker(theme, field)));
document.addEventListener('click', event => {
  if (!event.target.closest('.theme-color-control')) {
    document.querySelectorAll('.theme-picker-popover').forEach(item => { item.hidden = true; });
  }
});

// 自动补全数据
const slashCommands = [
  { cmd: '/init', desc: '初始化项目', hint: '' },
  { cmd: '/brainstorm', desc: '协作需求探索', hint: '' },
  { cmd: '/prd', desc: '启动 PRD 阶段', hint: '' },
  { cmd: '/spec', desc: '启动 SPEC 阶段', hint: '' },
  { cmd: '/task', desc: '启动 TASK 阶段', hint: '' },
  { cmd: '/devtest', desc: '日常开发测试', hint: '' },
  { cmd: '/fix', desc: '自动修复 open issues', hint: '' },
  { cmd: '/test', desc: '完整测试阶段', hint: '' },
  { cmd: '/status', desc: '报告当前状态', hint: '' },
  { cmd: '/mode', desc: '选择开发模式', hint: '' }
];
const mentions = [
  { name: '@rozsa-core', desc: 'Agent loop engine crate' },
  { name: '@rozsa-app', desc: 'Application runtime crate' },
  { name: '@rozsa-model', desc: 'LLM abstraction layer' },
  { name: '@rozsa-tui', desc: 'Terminal frontend (ratatui)' },
  { name: '@rozsa-cli', desc: 'Binary entry point (clap)' }
];

let acVisible = false;
let acSelected = 0;
let acItems = [];

function updateAutocomplete() {
  const input = document.getElementById('msgInput');
  const popup = document.getElementById('autocomplete');
  const val = getPrototypeInputText(input);

  // / 在开头触发
  if (/^\/\w*$/.test(val)) {
    const q = val.slice(1).toLowerCase();
    acItems = slashCommands.filter(c => c.cmd.slice(1).toLowerCase().includes(q));
    acSelected = 0;
    renderAutocomplete(popup, acItems.map(i =>
      `<div class="ac-cmd">${i.cmd}</div><div class="ac-desc">${i.desc}</div>${i.hint ? `<div class="ac-hint">${i.hint}</div>` : ''}`
    ));
    return;
  }

  // @ 在任意位置触发
  const atMatch = val.match(/@(\w*)$/);
  if (atMatch) {
    const q = atMatch[1].toLowerCase();
    acItems = mentions.filter(m => m.name.slice(1).toLowerCase().includes(q));
    acSelected = 0;
    renderAutocomplete(popup, acItems.map(i =>
      `<div class="ac-cmd">${i.name}</div><div class="ac-desc">${i.desc}</div>`
    ));
    return;
  }

  hideAutocomplete(popup);
}

function renderAutocomplete(popup, html) {
  if (html.length === 0) { hideAutocomplete(popup); return; }
  popup.innerHTML = html.map((h, i) =>
    `<div class="ac-item${i === 0 ? ' selected' : ''}" data-idx="${i}" onmousedown="selectAcItem(${i})">${h}</div>`
  ).join('');
  popup.classList.add('visible');
  acVisible = true;
}

function hideAutocomplete(popup) {
  popup.classList.remove('visible');
  acVisible = false;
}

function selectAcItem(idx) {
  const item = acItems[idx];
  if (!item) return;
  const input = document.getElementById('msgInput');
  const val = getPrototypeInputText(input);
  const replacement = item.cmd || item.name;
  // 替换触发部分
  if (val.startsWith('/')) {
    setPrototypeInputText(input, replacement + ' ');
  } else {
    const atIdx = val.lastIndexOf('@');
    setPrototypeInputText(input, val.slice(0, atIdx) + replacement + ' ');
  }
  hideAutocomplete(document.getElementById('autocomplete'));
  input.focus();
}

// Thinking 块折叠/展开（手动点击时清除 auto 标记）
function toggleThinkingBlock(header) {
  const block = header.closest('.thinking-block');
  block.classList.toggle('expanded');
  delete block.dataset.auto;
}

// Markdown 渲染：安全转义后支持常见块级语法，不做语法高亮。
function renderMarkdown(source) {
  const lines = source.replace(/\r\n/g, '\n').split('\n');
  const html = [];
  let paragraph = [];
  let list = null;
  let quote = [];

  const flushParagraph = () => {
    if (paragraph.length) {
      html.push(`<p>${parseInlineMarkdown(paragraph.join(' '))}</p>`);
      paragraph = [];
    }
  };
  const flushList = () => {
    if (list) {
      html.push(`<${list.type}>${list.items.map(item => `<li>${parseInlineMarkdown(item)}</li>`).join('')}</${list.type}>`);
      list = null;
    }
  };
  const flushQuote = () => {
    if (quote.length) {
      html.push(`<blockquote>${quote.map(line => `<p>${parseInlineMarkdown(line)}</p>`).join('')}</blockquote>`);
      quote = [];
    }
  };
  const flushAll = () => {
    flushParagraph();
    flushList();
    flushQuote();
  };

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const trimmed = line.trim();

    if (/^```/.test(trimmed)) {
      flushAll();
      const lang = trimmed.replace(/^```/, '').trim();
      const code = [];
      i++;
      while (i < lines.length && !/^```/.test(lines[i].trim())) {
        code.push(lines[i]);
        i++;
      }
      html.push(renderCodeBlock(code.join('\n'), lang));
      continue;
    }

    if (!trimmed) {
      flushAll();
      continue;
    }

    const heading = trimmed.match(/^(#{1,3})\s+(.+)$/);
    if (heading) {
      flushAll();
      const level = heading[1].length;
      html.push(`<h${level}>${parseInlineMarkdown(heading[2])}</h${level}>`);
      continue;
    }

    if (/^(-{3,}|\*{3,})$/.test(trimmed)) {
      flushAll();
      html.push('<hr>');
      continue;
    }

    const quoteLine = trimmed.match(/^>\s?(.*)$/);
    if (quoteLine) {
      flushParagraph();
      flushList();
      quote.push(quoteLine[1]);
      continue;
    }

    const unordered = trimmed.match(/^[-*]\s+(.+)$/);
    const ordered = trimmed.match(/^\d+[.)]\s+(.+)$/);
    if (unordered || ordered) {
      flushParagraph();
      flushQuote();
      const type = unordered ? 'ul' : 'ol';
      if (!list || list.type !== type) flushList();
      if (!list) list = { type, items: [] };
      list.items.push((unordered || ordered)[1]);
      continue;
    }

    flushList();
    flushQuote();
    paragraph.push(trimmed);
  }

  flushAll();
  return html.join('') || '<p></p>';
}

function parseInlineMarkdown(raw) {
  const codeSpans = [];
  let text = raw.replace(/`([^`]+)`/g, function(_, code) {
    const id = codeSpans.push(escapeHtml(code)) - 1;
    return `@@CODE${id}@@`;
  });

  let html = escapeHtml(text);
  html = html.replace(/\[([^\]]+)\]\(([^)\s]+)\)/g, function(_, label, href) {
    const safe = safeMarkdownHref(href);
    if (!safe) return label;
    return `<a href="${safe}" target="_blank" rel="noreferrer">${label}</a>`;
  });
  html = html.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>');
  html = html.replace(/__([^_]+)__/g, '<strong>$1</strong>');
  html = html.replace(/(^|[^*])\*([^*\n]+)\*/g, '$1<em>$2</em>');
  html = html.replace(/(^|[^_])_([^_\n]+)_/g, '$1<em>$2</em>');
  html = html.replace(/~~([^~]+)~~/g, '<s>$1</s>');
  html = html.replace(/@@CODE(\d+)@@/g, function(_, id) {
    return `<code>${codeSpans[Number(id)] || ''}</code>`;
  });
  return html;
}

function safeMarkdownHref(rawHref) {
  const href = rawHref.trim();
  try {
    const url = new URL(href, window.location.href);
    if (['http:', 'https:', 'mailto:'].includes(url.protocol)) {
      return escapeHtml(href);
    }
  } catch (_) {
    return '';
  }
  return '';
}

function renderCodeBlock(code, lang) {
  const cleanLang = (lang || 'text').trim().toLowerCase();
  const label = escapeHtml(cleanLang);
  return `<div class="md-code-block"><div class="md-code-head"><span class="md-code-lang">${label}</span><button class="md-copy" type="button" onclick="copyMarkdownCode(this)" aria-label="复制代码" title="复制代码">${copyIconSvg('copy')}</button></div><pre><code data-lang="${label}">${highlightCode(code, cleanLang)}</code></pre></div>`;
}

function highlightCode(code, lang) {
  const language = lang || 'text';
  return code.split('\n').map(line => highlightCodeLine(line, language)).join('\n');
}

function highlightCodeLine(line, lang) {
  const trimmed = line.trim();
  if (/^(#|\/\/)/.test(trimmed)) {
    return `<span class="md-syn-comment">${escapeHtml(line)}</span>`;
  }

  const stringTokens = [];
  let working = line.replace(/("(?:\\.|[^"])*"|'(?:\\.|[^'])*'|`(?:\\.|[^`])*`)/g, function(match) {
    const id = stringTokens.push(`<span class="md-syn-string">${escapeHtml(match)}</span>`) - 1;
    return `@@STR${id}@@`;
  });

  let html = escapeHtml(working);
  if (/^(bash|sh|shell|zsh)$/.test(lang)) {
    html = html.replace(/^(\s*)([a-zA-Z][\w.-]*)/, '$1<span class="md-syn-command">$2</span>');
    html = html.replace(/(\s)(--?[\w-]+)/g, '$1<span class="md-syn-option">$2</span>');
  } else if (/^(js|javascript|ts|typescript|jsx|tsx|rust|rs|json|toml)$/.test(lang)) {
    html = html.replace(/\b(const|let|var|function|return|if|else|for|while|class|new|async|await|import|export|from|pub|fn|struct|impl|trait|use|mod|match|enum|type|where|mut|self|true|false|null)\b/g, '<span class="md-syn-keyword">$1</span>');
    html = html.replace(/\b(\d+(?:\.\d+)?)\b/g, '<span class="md-syn-number">$1</span>');
  }

  return html.replace(/@@STR(\d+)@@/g, function(_, id) {
    return stringTokens[Number(id)] || '';
  });
}

function copyIconSvg(kind) {
  if (kind === 'check') {
    return '<svg viewBox="0 0 16 16" fill="none" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M13 4.5L6.5 11 3 7.5"/></svg>';
  }
  return '<svg viewBox="0 0 16 16" fill="none" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="5" y="4" width="8" height="9" rx="1.5"/><path d="M3 10.5V3.5A1.5 1.5 0 014.5 2h6"/></svg>';
}

function copyMarkdownCode(btn) {
  const block = btn.closest('.md-code-block');
  const code = block ? block.querySelector('code') : null;
  if (!code) return;
  const setCopied = () => {
    btn.classList.add('copied');
    btn.innerHTML = copyIconSvg('check');
    btn.setAttribute('aria-label', '已复制');
    btn.setAttribute('title', '已复制');
    setTimeout(() => {
      btn.classList.remove('copied');
      btn.innerHTML = copyIconSvg('copy');
      btn.setAttribute('aria-label', '复制代码');
      btn.setAttribute('title', '复制代码');
    }, 1200);
  };
  if (navigator.clipboard && navigator.clipboard.writeText) {
    navigator.clipboard.writeText(code.textContent).then(setCopied).catch(() => fallbackCopy(code.textContent, setCopied));
    return;
  }
  fallbackCopy(code.textContent, setCopied);
}

function fallbackCopy(text, onDone) {
  const area = document.createElement('textarea');
  area.value = text;
  area.setAttribute('readonly', '');
  area.style.position = 'fixed';
  area.style.left = '-9999px';
  document.body.appendChild(area);
  area.select();
  try {
    document.execCommand('copy');
    onDone();
  } finally {
    document.body.removeChild(area);
  }
}

// HTML 转义
function escapeHtml(s) {
  const d = document.createElement('div');
  d.textContent = s;
  return d.innerHTML;
}

let sidebarCollapsedByUser = false;

function updateSidebarToggleButton(collapsed) {
  ['sidebarToggleButton', 'settingsSidebarToggleButton'].forEach(id => {
    const button = document.getElementById(id);
    if (!button) return;
    button.setAttribute('aria-pressed', String(!collapsed));
    button.setAttribute('aria-label', collapsed ? 'Show sidebar' : 'Hide sidebar');
    button.title = collapsed ? 'Show sidebar' : 'Hide sidebar';
  });
}

function setMainSidebarCollapsed(collapsed) {
  const layout = document.querySelector('[data-od-id="app-body"]');
  if (!layout) return;
  layout.classList.toggle('sidebar-collapsed', collapsed);
  if (!collapsed) layout.classList.remove('sidebar-edge-visible');
  const titlebar = document.querySelector('[data-od-id="titlebar"]');
  if (titlebar) titlebar.classList.toggle('sidebar-collapsed', collapsed);
  const settingsPanel = document.getElementById('settingsPanel');
  if (settingsPanel) {
    settingsPanel.classList.toggle('settings-sidebar-collapsed', collapsed);
    if (!collapsed) settingsPanel.classList.remove('settings-edge-visible');
  }
  updateSidebarToggleButton(collapsed);
}

function toggleMainSidebar() {
  const layout = document.querySelector('[data-od-id="app-body"]');
  if (!layout) return;
  const collapsed = !layout.classList.contains('sidebar-collapsed');
  sidebarCollapsedByUser = collapsed;
  setMainSidebarCollapsed(collapsed);
}

function syncMainSidebarViewport() {
  const layout = document.querySelector('[data-od-id="app-body"]');
  if (!layout) return;
  const shouldCollapse = window.innerWidth <= 900;
  if (shouldCollapse) setMainSidebarCollapsed(true);
  else if (!sidebarCollapsedByUser) setMainSidebarCollapsed(false);
}

window.addEventListener('resize', syncMainSidebarViewport);
document.addEventListener('pointermove', event => {
  const targets = [
    { panel: document.querySelector('[data-od-id="app-body"]'), sidebar: document.querySelector('[data-od-id="sidebar"]'), collapsed: 'sidebar-collapsed', edge: 'sidebar-edge-visible', selector: '[data-od-id="sidebar"]' },
    { panel: document.getElementById('settingsPanel'), sidebar: document.querySelector('#settingsPanel .settings-sidebar'), collapsed: 'settings-sidebar-collapsed', edge: 'settings-edge-visible', selector: '#settingsPanel .settings-sidebar' },
  ];
  targets.forEach(target => {
    if (!target.panel || !target.sidebar || !target.panel.classList.contains(target.collapsed)) return;
    const sidebarWidth = target.sidebar.getBoundingClientRect().width || 260;
    if (event.clientX <= 14 || (target.panel.classList.contains(target.edge) && event.clientX <= sidebarWidth + 12)) {
      target.panel.classList.add(target.edge);
    } else if (!event.target.closest(target.selector)) {
      target.panel.classList.remove(target.edge);
    }
  });
});
syncMainSidebarViewport();

function applyPrototypeSceneFromLocation() {
  const scene = new URLSearchParams(window.location.search).get('scene') || window.location.hash.slice(1);
  const settingsPanel = document.getElementById('settingsPanel');
  if (!settingsPanel) return;
  if (scene === 'settings') {
    if (!settingsPanel.classList.contains('visible')) toggleSettings();
  } else if (settingsPanel.classList.contains('visible')) {
    closeSettings();
  }
}

window.addEventListener('hashchange', applyPrototypeSceneFromLocation);
applyPrototypeSceneFromLocation();
