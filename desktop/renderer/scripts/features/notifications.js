const escapeText = value => String(value).replace(/[&<>'"]/g, char => ({'&':'&amp;', '<':'&lt;', '>':'&gt;', "'":'&#39;', '"':'&quot;'}[char]));
const paths = {
  'circle-check': '<circle cx="12" cy="12" r="10"/><path d="m9 12 2 2 4-4"/>',
  'circle-x': '<circle cx="12" cy="12" r="10"/><path d="m15 9-6 6"/><path d="m9 9 6 6"/>',
  'circle-alert': '<circle cx="12" cy="12" r="10"/><line x1="12" x2="12" y1="8" y2="12"/><line x1="12" x2="12.01" y1="16" y2="16"/>',
  'triangle-alert': '<path d="m21.73 18-8-14a2 2 0 0 0-3.46 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3"/><path d="M12 9v4"/><path d="M12 17h.01"/>',
  info: '<circle cx="12" cy="12" r="10"/><path d="M12 16v-4"/><path d="M12 8h.01"/>',
  x: '<path d="M18 6 6 18"/><path d="m6 6 12 12"/>',
};
export function notificationIcon(severity) {
  const name = ({success:'circle-check', error:'circle-x', warning:'triangle-alert'})[severity] || 'info';
  return icon(name);
}
function icon(name) {
  return '<svg class="lucide" data-lucide="' + name + '" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" focusable="false">' + paths[name] + '</svg>';
}
export function notificationMarkup(item) {
  const severity = item.severity || 'info';
  return '<div class="notification-toast notification-' + escapeText(severity) + '" role="' + (severity === 'error' ? 'alert' : 'status') + '" data-notification-id="' + escapeText(item.id) + '">' +
    '<div class="notification-icon" aria-hidden="true">' + notificationIcon(severity) + '</div><div class="notification-body"><div class="notification-title">' +
    escapeText(item.title) + '</div><div class="notification-message">' + escapeText(item.message) +
    '</div></div><button class="notification-close" type="button" aria-label="Dismiss notification">' + icon('x') + '</button></div>';
}
export function errorItemMarkup(entry) {
  return '<div class="notification-error-item" role="listitem"><span class="notification-error-item-icon" aria-hidden="true">' + icon('circle-alert') + '</span><div><div class="notification-error-item-title">' +
    escapeText(entry.title) + '</div><div class="notification-error-item-message">' + escapeText(entry.message) + '</div></div></div>';
}
export function applyNotificationPresentation(toast, severity, title, message) {
  toast.element.className = 'notification-toast notification-' + severity;
  toast.element.setAttribute('role', severity === 'error' ? 'alert' : 'status');
  toast.element.querySelector('.notification-icon').innerHTML = notificationIcon(severity);
  toast.element.querySelector('.notification-title').textContent = title;
  toast.element.querySelector('.notification-message').textContent = message;
  Object.assign(toast, { severity, title, message });
}
export function createNotificationElement(item, doc = document) {
  const template = doc.createElement('template');
  template.innerHTML = notificationMarkup(item);
  return template.content.firstElementChild;
}
