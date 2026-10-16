export function bindToast(element, id, api) {
  element.querySelector('.notification-close').addEventListener('click', () => api.dismissToast(id));
  element.addEventListener('pointerenter', () => api.pauseToastTimer?.(id));
  element.addEventListener('pointerleave', () => api.resumeToastTimer?.(id));
}

export function bindErrorTray(api) {
  const tray = api.byId('notificationErrorTray');
  const button = api.byId('notificationErrorTrayButton');
  let pinned = false;
  const close = () => { pinned = false; api.closeNotificationErrorList(); };
  button.addEventListener('click', () => {
    if (pinned) close();
    else { api.openNotificationErrorList(); pinned = true; }
  });
  tray.addEventListener('pointerenter', () => { if (api.hasErrors()) api.openNotificationErrorList(); });
  tray.addEventListener('pointerleave', () => { if (!pinned) close(); });
  button.addEventListener('focusin', () => { if (api.hasErrors()) api.openNotificationErrorList(); });
  button.addEventListener('focusout', () => { if (!pinned) close(); });
  return { close, resetPin: () => { pinned = false; } };
}

export function init(api) {
  if (api.boundModules.has('notification')) return;
  api.boundModules.add('notification');
  const center = api.byId('notificationCenter');
  center.addEventListener('click', event => {
    const button = event.target.closest('.notification-close');
    if (button) api.dismissToast(button.closest('.notification-toast').dataset.notificationId);
  });
  const list = api.byId('notificationErrorList');
  const button = api.byId('notificationErrorTrayButton');
  const tray = bindErrorTray({ ...api,
    hasErrors: () => Number(api.byId('notificationErrorCount').textContent) > 0,
    isNotificationErrorListOpen: () => !list.hidden,
    openNotificationErrorList: () => { list.hidden = false; button.setAttribute('aria-expanded', 'true'); },
    closeNotificationErrorList: () => { list.hidden = true; button.setAttribute('aria-expanded', 'false'); },
  });
  document.addEventListener('keydown', event => {
    if (event.key === 'Escape' && !list.hidden) { event.stopImmediatePropagation(); tray.close(); }
  });
}
