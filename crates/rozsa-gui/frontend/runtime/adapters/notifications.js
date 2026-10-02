import { createNotificationElement, errorItemMarkup } from '../../ui/scripts/features/notifications.js';
import { bindToast, bindErrorTray } from '../../ui/scripts/components/notification.js';

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

// Sidebar command failures remain local and unresolved after visual dismissal.
// Backend-owned structured notifications continue to target the main WebView.
export function createSidebarFeedback(root = document) {
  const byId = id => root.getElementById(id);
  const errors = new Map();
  let counter = 0;
  const list = byId('notificationErrorList');
  const button = byId('notificationErrorTrayButton');
  const closeNotificationErrorList = () => { list.hidden = true; button.setAttribute('aria-expanded', 'false'); };
  const controller = bindErrorTray({ byId, hasErrors: () => errors.size > 0,
    isNotificationErrorListOpen: () => !list.hidden,
    openNotificationErrorList: () => {
      list.innerHTML = [...errors.values()].map(errorItemMarkup).join('');
      list.hidden = false;
      button.setAttribute('aria-expanded', 'true');
    }, closeNotificationErrorList,
  });
  root.addEventListener('keydown', event => {
    if (event.key === 'Escape' && !list.hidden) { event.preventDefault(); controller.close(); }
  });
  return message => {
    const item = { id: 'sidebar-error-' + (++counter), severity: 'error', title: 'Error', message: String(message) };
    const element = createNotificationElement(item, root);
    bindToast(element, item.id, { dismissToast: () => {
      element.remove();
      errors.set(item.id, item);
      byId('notificationErrorCount').textContent = String(errors.size);
      byId('notificationErrorTray').hidden = false;
    } });
    byId('notificationStack').appendChild(element);
  };
}
