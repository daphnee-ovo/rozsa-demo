const escapeText = value => String(value ?? '').replace(/[&<>'"]/g, char => ({'&':'&amp;', '<':'&lt;', '>':'&gt;', "'":'&#39;', '"':'&quot;'}[char]));
export function sessionsMarkup(sessions) {
  return sessions.map((session, index) => '<div class="session-item' + (session.active ? ' active' : '') + '" data-session-index="' + index + '" data-od-id="session-' + index + '" tabindex="0" role="button"><span class="session-status ' + escapeText(session.activity) + '"></span><div class="session-name">' + escapeText(session.name) + '</div><div class="session-meta"><span>' + escapeText(session.date) + '</span></div></div>').join('');
}
export function init(api) {
  if (api.boundModules.has('sidebar')) return;
  api.boundModules.add('sidebar');
  const root = api.byId('sidebar');
  root.addEventListener('click', event => {
    const session = event.target.closest('.session-item');
    if (session) api.activateSession(Number(session.dataset.sessionIndex));
    const action = event.target.closest('[data-action]')?.dataset.action;
    if (action === 'new-session') api.newSession();
    if (action === 'toggle-settings') api.toggleSettings();
    if (action === 'dev-flow-dashboard') api.openDevFlowDashboard();
  });
  root.addEventListener('keydown', event => {
    const session = event.target.closest('.session-item');
    if (session && (event.key === 'Enter' || event.key === ' ')) {
      event.preventDefault(); api.activateSession(Number(session.dataset.sessionIndex));
    }
  });
}
export function suspendQuotaTooltip(root = document) {
  const tooltip = root.getElementById('quotaTooltip');
  if (tooltip) { tooltip.replaceChildren(); tooltip.hidden = true; tooltip.dataset.uiSuspended = 'quota-tooltip'; }
}
