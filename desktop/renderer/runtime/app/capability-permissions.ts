// @ts-nocheck
function renderCapabilitySettings() {
  // This settings UI has no approved prototype yet; late callbacks must stay inert.
  return;
  if (!capabilitySettings) return;
  renderCapabilityPane('tools', capabilitySettings.tools || []);
  const skills = capabilityScope.skills === 'project'
    ? mergeCapabilityItems(
        capabilitySettings.globalSkills || [],
        capabilitySettings.projectSkills || []
      )
    : capabilitySettings.globalSkills || [];
  renderCapabilityPane('skills', skills);
}

function mergeCapabilityItems(base, overlay) {
  const merged = new Map(base.map(item => [item.name, item]));
  for (const item of overlay) merged.set(item.name, item);
  return [...merged.values()].sort((left, right) => left.label.localeCompare(right.label));
}

function renderCapabilityPane(kind, items) {
  // This settings UI has no approved prototype yet; late callbacks must stay inert.
  return;
  const scope = capabilityScope[kind];
  const scopeHost = document.getElementById(kind + 'Scope');
  const list = document.getElementById(kind === 'tools' ? 'settingsToolList' : 'settingsSkillList');
  if (!scopeHost || !list) return;
  scopeHost.replaceChildren();
  for (const candidate of ['global', 'project']) {
    const button = document.createElement('button');
    button.type = 'button';
    button.textContent = candidate === 'global' ? 'Global' : 'Project';
    button.classList.toggle('active', scope === candidate);
    button.onclick = () => {
      capabilityScope[kind] = candidate;
      renderCapabilitySettings();
    };
    scopeHost.appendChild(button);
  }

  list.replaceChildren();
  if (!items.length) {
    const empty = document.createElement('p');
    empty.className = 'capability-description';
    empty.textContent = kind === 'skills' ? 'No skills found in this scope.' : 'No tools registered.';
    list.appendChild(empty);
    return;
  }
  for (const item of items) {
    const row = document.createElement('div');
    row.className = 'setting-item capability-row';
    const copy = document.createElement('div');
    copy.className = 'capability-copy';
    copy.innerHTML = `<div class="capability-name">${escapeHtml(item.label)}</div>` +
      `<div class="capability-description">${escapeHtml(item.description || item.name)}</div>`;
    const override = scope === 'global' ? item.globalOverride : item.projectOverride;
    const controls = document.createElement('div');
    controls.className = 'capability-controls';
    if (override == null) {
      const inherited = document.createElement('span');
      inherited.className = 'capability-inherited';
      inherited.textContent = scope === 'project' ? 'Inherited' : 'Default';
      controls.appendChild(inherited);
    } else {
      const reset = document.createElement('button');
      reset.type = 'button';
      reset.className = 'capability-reset';
      reset.textContent = 'Reset';
      reset.title = scope === 'project' ? 'Restore global inheritance' : 'Restore default';
      reset.onclick = async () => {
        try {
          capabilitySettings = await invoke('update_capability_setting', {
            kind, scope, name: item.name, enabled: null,
          });
          renderCapabilitySettings();
        } catch (error) {
          showError(`Failed to update ${kind}: ${String(error)}`);
        }
      };
      controls.appendChild(reset);
    }
    const toggle = document.createElement('button');
    toggle.type = 'button';
    toggle.className = 'setting-toggle';
    toggle.setAttribute('role', 'switch');
    toggle.setAttribute('aria-label', `${item.label} ${scope}`);
    setSettingSwitch(toggle, item.effective);
    toggle.onclick = async () => {
      try {
        capabilitySettings = await invoke('update_capability_setting', {
          kind, scope, name: item.name, enabled: !item.effective,
        });
        renderCapabilitySettings();
      } catch (error) {
        showError(`Failed to update ${kind}: ${String(error)}`);
        renderCapabilitySettings();
      }
    };
    controls.appendChild(toggle);
    row.append(copy, controls);
    list.appendChild(row);
  }
}

function permissionLayerRules(kind) {
  if (!permissionSettings) return null;
  return permissionSettings[`${permissionScope}${kind[0].toUpperCase()}${kind.slice(1)}`];
}

function effectivePermissionRules(kind) {
  return permissionSettings?.[`effective${kind[0].toUpperCase()}${kind.slice(1)}`] || [];
}

function defaultPermissionRules(kind) {
  return permissionSettings?.[`default${kind[0].toUpperCase()}${kind.slice(1)}`] || [];
}

function displayedPermissionRules(kind) {
  const local = permissionLayerRules(kind);
  if (local != null) return [...local];
  return permissionScope === 'global'
    ? [...defaultPermissionRules(kind)]
    : [...effectivePermissionRules(kind)];
}

function permissionRuleGroups(kind, rows) {
  if (kind !== 'allow') return [{ name: null, rules: rows }];
  const groupedRules = new Set();
  const groups = (permissionSettings?.defaultAllowGroups || []).map(group => {
    const rules = rows.filter(rule => group.rules.includes(rule));
    rules.forEach(rule => groupedRules.add(rule));
    return { name: group.name, rules };
  });
  return [
    { name: null, rules: rows.filter(rule => !groupedRules.has(rule)) },
    ...groups,
  ];
}

function renderPermissionRuleRows(list, kind, rules, inherited) {
  list.dataset.permissionKind = kind;
  for (const rule of rules) {
    const row = document.createElement('div');
    row.className = 'permission-rule-row';
    wirePermissionRulePointerDrag(row, kind, rule);
    const copy = document.createElement('div');
    copy.className = 'permission-rule-copy';
    copy.textContent = rule;
    row.appendChild(copy);
    if (inherited) {
      const badge = document.createElement('span');
      badge.className = 'capability-inherited';
      badge.textContent = permissionScope === 'global' ? 'Default' : 'Inherited';
      row.appendChild(badge);
    }
    const remove = document.createElement('button');
    remove.type = 'button';
    remove.className = 'permission-rule-delete';
    remove.setAttribute('aria-label', `Delete ${rule}`);
    remove.textContent = 'Delete';
    remove.onclick = () => removePermissionRule(kind, rule);
    row.appendChild(remove);
    list.appendChild(row);
  }
  return rules.length;
}

function renderPermissionRuleEmpty(list) {
  const empty = document.createElement('div');
  empty.className = 'permission-rule-empty';
  empty.textContent = 'No rules configured';
  list.appendChild(empty);
}

function renderPermissionSettings() {
  // This settings UI has no approved prototype yet; late callbacks must stay inert.
  return;
  if (!permissionSettings) return;
  const scopeHost = document.getElementById('permissionScope');
  if (scopeHost) {
    scopeHost.replaceChildren();
    for (const candidate of ['global', 'project']) {
      const button = document.createElement('button');
      button.type = 'button';
      button.textContent = candidate === 'global' ? 'Global' : 'Project';
      button.classList.toggle('active', permissionScope === candidate);
      button.onclick = () => {
        permissionScope = candidate;
        renderPermissionSettings();
      };
      scopeHost.appendChild(button);
    }
  }

  const mode = document.getElementById('settingsPermMode');
  if (mode) {
    mode.innerHTML =
      '<option value="on-request">On request</option>' +
      '<option value="auto-approve">Auto approve (not implemented)</option>' +
      '<option value="yolo">Yolo</option>';
    mode.value = permissionScope === 'global'
      ? permissionSettings.globalEffectiveMode
      : permissionSettings.effectiveMode;
    mode.title = `Effective: ${permissionSettings.effectiveMode}`;
    mode.onchange = async () => {
      try {
        permissionSettings = await invoke('update_permission_mode', {
          scope: permissionScope, mode: mode.value,
        });
        setPermissionSettingsError('');
      } catch (error) {
        setPermissionSettingsError(String(error));
      }
      renderPermissionSettings();
    };
  }

  for (const kind of ['deny', 'ask', 'allow']) renderPermissionRuleList(kind);
}

function setPermissionSettingsError(message) {
  const error = document.getElementById('permissionSettingsError');
  if (!error) return;
  error.textContent = message;
  error.hidden = !message;
}

function renderPermissionRuleList(kind) {
  // This settings UI has no approved prototype yet; late callbacks must stay inert.
  return;
  const host = document.getElementById(`permission${kind[0].toUpperCase()}${kind.slice(1)}List`);
  if (!host) return;
  const previousGroupOpen = new Map(
    [...(host.parentElement?.querySelectorAll('.permission-rule-group') || [])]
      .map(group => [group.dataset.permissionRuleGroup || '', group.open])
  );
  host.parentElement?.querySelectorAll('.permission-rule-group')
    .forEach(group => group.remove());
  host.replaceChildren();
  host.dataset.permissionKind = kind;
  const localRules = permissionLayerRules(kind);
  const inherited = localRules == null;
  const rows = displayedPermissionRules(kind);
  const groups = permissionRuleGroups(kind, rows);
  const ungrouped = groups.find(group => !group.name) || { rules: [] };
  if (!renderPermissionRuleRows(host, kind, ungrouped.rules, inherited)) {
    renderPermissionRuleEmpty(host);
  }

  let insertionPoint = host;
  for (const group of groups) {
    if (!group.name) continue;
    const details = document.createElement('details');
    details.className = 'permission-rule-group';
    details.dataset.permissionRuleGroup = group.name;
    details.open = previousGroupOpen.get(group.name) === true;

    const summary = document.createElement('summary');
    summary.className = 'permission-rule-group-summary';
    const title = document.createElement('span');
    title.className = 'permission-rule-group-title';
    title.textContent = group.name;
    const count = document.createElement('span');
    count.className = 'permission-rule-group-count';
    count.textContent = `${group.rules.length} ${group.rules.length === 1 ? 'rule' : 'rules'}`;
    summary.append(title, count);

    const list = document.createElement('div');
    list.className = 'permission-rule-list permission-rule-group-list';
    const renderedRows = renderPermissionRuleRows(list, kind, group.rules, inherited);
    if (!renderedRows) renderPermissionRuleEmpty(list);
    details.append(summary, list);
    insertionPoint.after(details);
    insertionPoint = details;
  }
  const reset = document.getElementById(`permission${kind[0].toUpperCase()}${kind.slice(1)}Reset`);
  if (reset) {
    reset.hidden = localRules == null;
    reset.textContent = permissionScope === 'global' ? 'Restore defaults' : 'Restore inheritance';
  }
}

function wirePermissionRulePointerDrag(row, kind, rule) {
  let origin = null;
  let dragging = false;
  row.onpointerdown = event => {
    if (event.button !== 0 || event.target.closest('button')) return;
    event.preventDefault();
    origin = { x: event.clientX, y: event.clientY, pointerId: event.pointerId };
    row.setPointerCapture(event.pointerId);
  };
  row.onpointermove = event => {
    if (!origin || origin.pointerId !== event.pointerId) return;
    if (!dragging && Math.hypot(event.clientX - origin.x, event.clientY - origin.y) < 5) return;
    dragging = true;
    row.classList.add('permission-rule-dragging');
    document.querySelectorAll('.permission-rule-drop-target')
      .forEach(element => element.classList.remove('permission-rule-drop-target'));
    const target = document.elementFromPoint(event.clientX, event.clientY)
      ?.closest('.permission-rule-list');
    if (target && target !== row.parentElement) target.classList.add('permission-rule-drop-target');
  };
  const finish = event => {
    if (!origin || origin.pointerId !== event.pointerId) return;
    const target = document.elementFromPoint(event.clientX, event.clientY)
      ?.closest('.permission-rule-list');
    const targetKind = target?.dataset.permissionKind;
    if (dragging && targetKind && targetKind !== kind) {
      void movePermissionRule(kind, targetKind, rule);
    }
    origin = null;
    dragging = false;
    row.classList.remove('permission-rule-dragging');
    document.querySelectorAll('.permission-rule-drop-target')
      .forEach(element => element.classList.remove('permission-rule-drop-target'));
  };
  row.onpointerup = finish;
  row.onpointercancel = finish;
}

function openPermissionRuleEditor(kind) {
  // This settings UI has no approved prototype yet; late callbacks must stay inert.
  return;
  closePermissionRuleEditor();
  pendingPermissionRuleKind = kind;
  const host = document.getElementById(`permission${kind[0].toUpperCase()}${kind.slice(1)}List`);
  const template = document.getElementById('permissionRuleEditorTemplate');
  if (!host || !template) return;
  host.appendChild(template.content.cloneNode(true));
  const tool = document.getElementById('permissionRuleTool');
  if (!tool) return;
  setupPermissionToolCombobox(capabilitySettings?.tools || []);
  const regexp = document.getElementById('permissionRuleRegexp');
  setSettingSwitch(regexp, false);
  regexp.onclick = () => {
    setSettingSwitch(regexp, !isSettingSwitchOn(regexp));
    highlightPermissionRulePattern();
    updatePermissionRuleHint();
  };
  const input = document.getElementById('permissionRuleTarget');
  input.oninput = highlightPermissionRulePattern;
  input.onkeydown = event => {
    if (event.key === 'Enter') event.preventDefault();
  };
  input.onpaste = event => {
    event.preventDefault();
    document.execCommand('insertText', false, event.clipboardData.getData('text/plain').replace(/[\r\n]+/g, ' '));
  };
  updatePermissionRuleHint();
  tool.focus();
}

function closePermissionRuleEditor() {
  const editor = document.getElementById('permissionRuleEditor');
  if (editor) editor.remove();
}

function isPathPermissionTool(tool) {
  return ['read', 'write', 'edit'].includes(String(tool || '').toLowerCase());
}

function setupPermissionToolCombobox(tools) {
  // This settings UI has no approved prototype yet; late callbacks must stay inert.
  return;
  const input = document.getElementById('permissionRuleTool');
  const button = document.getElementById('permissionRuleToolButton');
  const combobox = input?.closest('.permission-tool-combobox');
  if (!input || !button || !combobox) return;
  permissionToolOptions = tools.map(item => ({ name: item.name, label: item.label }));
  permissionToolActiveIndex = -1;
  input.oninput = () => {
    renderPermissionToolOptions(true);
    updatePermissionRuleHint();
  };
  input.onfocus = () => renderPermissionToolOptions(true, true);
  input.onclick = () => renderPermissionToolOptions(true);
  input.onkeydown = event => {
    const matches = matchingPermissionTools();
    if (event.key === 'Tab' && input.value && matches.length) {
      event.preventDefault();
      selectPermissionTool(matches[0]);
      document.getElementById('permissionRuleTarget')?.focus();
      return;
    }
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault();
      if (!matches.length) return;
      const direction = event.key === 'ArrowDown' ? 1 : -1;
      permissionToolActiveIndex =
        (permissionToolActiveIndex + direction + matches.length) % matches.length;
      renderPermissionToolOptions(true);
      return;
    }
    if (event.key === 'Enter' && permissionToolActiveIndex >= 0 && matches[permissionToolActiveIndex]) {
      event.preventDefault();
      selectPermissionTool(matches[permissionToolActiveIndex]);
      document.getElementById('permissionRuleTarget')?.focus();
      return;
    }
    if (event.key === 'Escape') closePermissionToolOptions();
  };
  button.onclick = () => {
    const list = document.getElementById('permissionRuleToolList');
    if (list?.hidden) {
      renderPermissionToolOptions(true, true);
    } else {
      closePermissionToolOptions();
    }
  };
  combobox.onfocusout = event => {
    if (!combobox.contains(event.relatedTarget)) {
      setTimeout(closePermissionToolOptions, 100);
    }
  };
  renderPermissionToolOptions(false, true);
}

function matchingPermissionTools(showAll = false) {
  const query = document.getElementById('permissionRuleTool')?.value.trim().toLowerCase() || '';
  if (showAll || !query) return permissionToolOptions;
  return permissionToolOptions.filter(item =>
    item.name.toLowerCase().startsWith(query) || item.label.toLowerCase().startsWith(query)
  );
}

function renderPermissionToolOptions(open, showAll = false) {
  // This settings UI has no approved prototype yet; late callbacks must stay inert.
  return;
  const input = document.getElementById('permissionRuleTool');
  const list = document.getElementById('permissionRuleToolList');
  if (!input || !list) return;
  const matches = matchingPermissionTools(showAll);
  if (permissionToolActiveIndex >= matches.length) permissionToolActiveIndex = -1;
  list.replaceChildren();
  matches.forEach((item, index) => {
    const option = document.createElement('div');
    option.className = 'permission-tool-option';
    option.classList.toggle('active', index === permissionToolActiveIndex);
    option.setAttribute('role', 'option');
    option.textContent = item.label;
    option.onmousedown = event => {
      event.preventDefault();
      selectPermissionTool(item);
    };
    list.appendChild(option);
  });
  list.hidden = !open || matches.length === 0;
  input.setAttribute('aria-expanded', String(!list.hidden));
}

function closePermissionToolOptions() {
  const input = document.getElementById('permissionRuleTool');
  const list = document.getElementById('permissionRuleToolList');
  if (list) list.hidden = true;
  if (input) input.setAttribute('aria-expanded', 'false');
  permissionToolActiveIndex = -1;
}

function selectPermissionTool(item) {
  // This settings UI has no approved prototype yet; late callbacks must stay inert.
  return;
  const input = document.getElementById('permissionRuleTool');
  if (!input) return;
  input.value = item.label;
  closePermissionToolOptions();
  updatePermissionRuleHint();
}

function updatePermissionRuleHint() {
  const tool = document.getElementById('permissionRuleTool')?.value || '';
  const input = document.getElementById('permissionRuleTarget');
  const hint = document.getElementById('permissionRuleHint');
  const prefix = document.getElementById('permissionRulePathPrefix');
  const regexp = isSettingSwitchOn(document.getElementById('permissionRuleRegexp'));
  if (!input || !hint) return;
  if (prefix) prefix.hidden = permissionScope !== 'global' || !isPathPermissionTool(tool);
  if (isPathPermissionTool(tool)) {
    input.dataset.placeholder = permissionScope === 'global'
      ? 'path/**/*.md'
      : 'docs/**/*.md';
    hint.textContent = permissionScope === 'global'
      ? `$HOME/ is fixed. ${regexp ? 'The regular expression applies only to the Home-relative suffix and must match it fully.' : '* matches one path segment; ** matches recursively.'}`
      : `${regexp ? 'The regular expression must fully match the normalized project-relative path.' : '* matches one path segment; ** matches recursively.'}`;
  } else {
    input.dataset.placeholder = regexp ? '^cargo (test|check)( .*)?$' : '*';
    hint.textContent = regexp
      ? 'The regular expression must fully match each command segment or tool target.'
      : 'Use * for every invocation, or include * inside a command pattern.';
  }
}

function highlightPermissionRulePattern() {
  const input = document.getElementById('permissionRuleTarget');
  if (!input) return;
  const text = getInputText(input);
  const regexp = isSettingSwitchOn(document.getElementById('permissionRuleRegexp'));
  const special = regexp ? /[\\^$.*+?()[\]{}|]/g : /\*/g;
  const ranges = [];
  for (const match of text.matchAll(special)) {
    ranges.push({ start: match.index, end: match.index + match[0].length });
  }
  renderRichInputHighlights(input, ranges);
}

async function savePermissionRule() {
  // This settings UI has no approved prototype yet; late callbacks must stay inert.
  return;
  const toolInput = document.getElementById('permissionRuleTool')?.value.trim() || '';
  const matchedTool = permissionToolOptions.find(item =>
    item.name.toLowerCase() === toolInput.toLowerCase()
      || item.label.toLowerCase() === toolInput.toLowerCase()
  );
  const tool = matchedTool?.name;
  const input = document.getElementById('permissionRuleTarget');
  const rawTarget = input ? getInputText(input).trim() : '';
  const regexp = isSettingSwitchOn(document.getElementById('permissionRuleRegexp'));
  if (!tool) {
    setPermissionSettingsError('Choose a tool from the suggestions before adding the rule.');
    return;
  }
  let target = rawTarget || '*';
  if (!regexp && permissionScope === 'global' && isPathPermissionTool(tool)) {
    target = `$HOME/${target.replace(/^\/+/, '')}`;
  }
  if (regexp) {
    if (!rawTarget) {
      setPermissionSettingsError('Enter a regular expression before adding the rule.');
      return;
    }
    if (permissionScope === 'global' && isPathPermissionTool(tool)) {
      target = `$HOME/regex:${rawTarget.replace(/^\/+/, '')}`;
    } else {
      target = `regex:${rawTarget}`;
    }
  }
  const rule = `${tool}(${target})`;
  if (rule === '*(*)') {
    setPermissionSettingsError('*(*) is not allowed. Use yolo mode to allow every tool.');
    return;
  }
  const local = displayedPermissionRules(pendingPermissionRuleKind);
  if (local.includes(rule)) {
    closePermissionRuleEditor();
    return;
  }
  await replacePermissionRules(pendingPermissionRuleKind, [...local, rule]);
  closePermissionRuleEditor();
}

async function removePermissionRule(kind, rule) {
  // This settings UI has no approved prototype yet; late callbacks must stay inert.
  return;
  const local = displayedPermissionRules(kind);
  await replacePermissionRules(kind, local.filter(candidate => candidate !== rule));
}

async function resetPermissionRules(kind) {
  // This settings UI has no approved prototype yet; late callbacks must stay inert.
  return;
  await replacePermissionRules(kind, null);
}

async function replacePermissionRules(kind, rules) {
  // This settings UI has no approved prototype yet; late callbacks must stay inert.
  return;
  try {
    permissionSettings = await invoke('update_permission_rules', {
      scope: permissionScope, kind, rules,
    });
    setPermissionSettingsError('');
    renderPermissionSettings();
  } catch (error) {
    setPermissionSettingsError(`Failed to update permission rules: ${String(error)}`);
  }
}

async function movePermissionRule(fromKind, toKind, rule) {
  // This settings UI has no approved prototype yet; late callbacks must stay inert.
  return;
  if (fromKind === toKind) return;
  const ruleSet = Object.fromEntries(
    ['deny', 'ask', 'allow'].map(kind => [kind, displayedPermissionRules(kind)])
  );
  ruleSet[fromKind] = ruleSet[fromKind].filter(candidate => candidate !== rule);
  if (!ruleSet[toKind].includes(rule)) ruleSet[toKind].push(rule);
  try {
    permissionSettings = await invoke('update_permission_rule_set', {
      scope: permissionScope,
      deny: ruleSet.deny,
      ask: ruleSet.ask,
      allow: ruleSet.allow,
    });
    setPermissionSettingsError('');
    renderPermissionSettings();
  } catch (error) {
    setPermissionSettingsError(`Failed to move permission rule: ${String(error)}`);
  }
}

