export function init(api) {
  if (api.boundModules.has("conversation")) return;
  api.boundModules.add("conversation");
  api.renderMessages();
}

