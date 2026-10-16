// Runtime events retain their backend identities and resolution semantics.
export async function subscribeNotifications(port, api) {
  await port.subscribe('app-notification', payload => {
    if (payload?.type === 'upsert' && typeof payload.id === 'string' && payload.id.length > 0 &&
        ['info', 'success', 'warning', 'error'].includes(payload.severity) &&
        typeof payload.title === 'string' && typeof payload.message === 'string') api.upsertNotification(payload);
    else if (payload?.type === 'resolve' && typeof payload.id === 'string' && payload.id.length > 0) api.resolveNotification(payload.id);
    else api.showError('Invalid app-notification event: ' + JSON.stringify(payload));
  });
  await port.subscribe('notification', payload => api.showNotification(typeof payload === 'string' ? payload : JSON.stringify(payload)));
}

export function showRuntimeError(message, api) {
  api.upsertNotification({ id: api.nextErrorId(), severity: 'error', title: 'Error', message: String(message), timeoutMs: 6000 });
}

// Sidebar feedback is displayed by the single global center in the main WebView.
export function createSidebarFeedback(adapter) {
  return message => adapter.emit('error', String(message));
}
