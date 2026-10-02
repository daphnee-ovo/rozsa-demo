export function init(api) {
  if (api.boundModules.has("tool-call")) return;
  api.boundModules.add("tool-call");

  const chat = api.byId("chatMessages");
  chat?.addEventListener("click", event => {
    const diff = event.target.closest(".change-name");
    if (diff) {
      api.toggleTurnDiff(diff);
      return;
    }
    const card = event.target.closest(".tool-call");
    if (card) api.toggleToolCall(card);
  });
  chat?.addEventListener("keydown", event => {
    if (event.key !== "Enter" && event.key !== " ") return;
    if (event.target.closest(".change-name")) return;
    const card = event.target.closest(".tool-call");
    if (!card) return;
    event.preventDefault();
    api.toggleToolCall(card);
  });
}
