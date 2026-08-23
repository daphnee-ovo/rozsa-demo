export function init(api) {
  if (api.boundModules.has("notification")) return;
  api.boundModules.add("notification");

  const center = api.byId("notificationCenter");
  center?.addEventListener("click", event => {
    const close = event.target.closest(".notification-close");
    if (close) {
      close.closest(".notification-toast")?.remove();
      return;
    }
    if (event.target.closest("#notificationErrorTrayButton")) {
      const list = api.byId("notificationErrorList");
      const button = api.byId("notificationErrorTrayButton");
      if (!list || !button) return;
      list.hidden = !list.hidden;
      button.setAttribute("aria-expanded", String(!list.hidden));
    }
  });
}

