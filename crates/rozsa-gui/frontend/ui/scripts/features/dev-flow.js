const escape = value => String(value ?? '').replace(/[&<>"']/g, char => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[char]);

export function claimedMarkup(item) {
  return '<button class="dev-flow-claimed-row" type="button" title="' + escape(item.title) + '" data-dev-flow-trigger="true"><span class="dev-flow-claimed-dot" aria-hidden="true"></span><span class="dev-flow-claimed-id">' + escape(item.id) + '</span><span class="dev-flow-claimed-title">' + escape(item.title) + '</span></button>';
}

function disable(control, disabled) {
  if (!control) return;
  control.disabled = disabled;
  control.setAttribute('aria-disabled', String(disabled));
  control.closest('.setting-item')?.classList.toggle('is-disabled', disabled);
}

export function renderSettings(view, root = document) {
  if (!view) return;
  const byId = id => root.getElementById(id);
  const text = (id, value) => { const element = byId(id); if (element) element.textContent = value; };
  for (const [id, enabled] of [['devFlowEnabled', view.enabled], ['devFlowSidebarStatus', view.sidebarEnabled], ['devFlowDashboardButton', view.dashboardEnabled]]) {
    const control = byId(id);
    control?.classList.toggle('on', enabled);
    control?.setAttribute('aria-checked', String(enabled));
  }
  text('devFlowDashboardAvailability', view.availability);
  text('devFlowDashboardAddressText', view.address);
  text('devFlowVersion', view.version);
  text('devFlowMemoryAmount', view.memory.amount);
  text('devFlowMemoryUnit', view.memory.unit);
  byId('devFlowDashboardStatus')?.classList.toggle('is-ready', view.availability.toLowerCase() === 'ready');
  const address = byId('devFlowDashboardAddress');
  if (address) { address.disabled = !view.dashboardAvailable; address.title = view.dashboardAvailable ? 'Open Dev Flow dashboard' : 'Dashboard unavailable'; }
  const path = byId('devFlowExecutablePath');
  if (path) path.value = view.executablePath;
  disable(byId('devFlowEnabled'), false);
  disable(byId('devFlowSidebarStatus'), view.dependentDisabled);
  disable(byId('devFlowDashboardButton'), view.dependentDisabled);
  disable(byId('devFlowPickExecutable'), false);
  disable(path, false);
  const missing = byId('devFlowMissing');
  if (missing) missing.hidden = !view.missing;
}

function filesMarkup(files) {
  if (!files.length) return '';
  return '<details class="dev-flow-detail-disclosure"><summary>Files (' + files.length + ')</summary><div class="dev-flow-detail-disclosure-body">' + files.map(file => '<div>' + escape(file) + '</div>').join('') + '</div></details>';
}

export function renderDetail(view, root = document) {
  for (const [id, value] of [['devFlowDetailRevision', view.revision], ['devFlowDetailProject', view.project], ['devFlowDetailSummary', view.summary]]) {
    const element = root.getElementById(id);
    if (element) element.textContent = value;
  }
  const list = root.getElementById('devFlowDetailList');
  if (!list) return;
  list.innerHTML = view.items.map(item => '<div class="dev-flow-detail-item' + (item.focus ? ' focus' : '') + '" role="listitem" tabindex="0" aria-label="' + escape(item.label || item.id + ' ' + item.title) + '"><div class="dev-flow-detail-item-head"><span class="dev-flow-detail-item-id">' + escape(item.id) + '</span><span class="dev-flow-detail-item-title">' + escape(item.title) + '</span><span class="dev-flow-detail-item-status' + (item.status === 'in-progress' ? ' in-progress' : '') + '">' + escape(item.status) + '</span></div>' + (item.meta ? '<div class="dev-flow-detail-item-meta">' + escape(item.meta) + '</div>' : '') + (item.description ? '<div class="dev-flow-detail-item-desc">' + escape(item.description) + '</div>' : '') + filesMarkup(item.files) + '</div>').join('');
  list.querySelector('.focus')?.scrollIntoView({ block: 'nearest' });
}

export function init(api) {
  if (api.boundModules.has('dev-flow')) return;
  api.boundModules.add('dev-flow');
  api.byId('devFlowDetailClose')?.addEventListener('click', api.closeDevFlowDetail);
  document.addEventListener('pointerdown', event => {
    if (!api.byId('devFlowDetail')?.hidden && !event.target.closest('#devFlowDetail')) api.closeDevFlowDetail();
  });
  document.addEventListener('keydown', event => {
    if (api.byId('devFlowDetail')?.hidden) return;
    if (event.key === 'Escape') { event.preventDefault(); api.closeDevFlowDetail(); return; }
    if (event.key !== 'ArrowDown' && event.key !== 'ArrowUp') return;
    const items = Array.from(document.querySelectorAll('#devFlowDetail .dev-flow-detail-item'));
    if (!items.length) return;
    const index = items.indexOf(document.activeElement);
    const next = event.key === 'ArrowDown' ? (index + 1) % items.length : (index - 1 + items.length) % items.length;
    event.preventDefault();
    items[next].focus();
  });
  document.addEventListener('click', event => {
    if (event.target.closest('#sidebarDevFlowDashboard')) api.openDevFlowDashboard?.();
    else if (event.target.closest('#devFlowSidebarSummary, #devFlowSidebarMore, [data-dev-flow-trigger]')) api.openDevFlowDetail?.();
  });
}
