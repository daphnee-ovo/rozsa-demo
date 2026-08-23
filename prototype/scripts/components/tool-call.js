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
    const thinking = event.target.closest(".thinking-header");
    if (thinking) {
      api.toggleThinking(thinking);
      return;
    }
    const card = event.target.closest(".tool-call");
    if (card) api.toggleToolCall(card);
  });
  chat?.addEventListener("keydown", event => {
    if (event.key !== "Enter" && event.key !== " ") return;
    if (event.target.closest(".change-name")) return;
    const thinking = event.target.closest(".thinking-header");
    const card = event.target.closest(".tool-call");
    if (!thinking && !card) return;
    event.preventDefault();
    if (thinking) api.toggleThinking(thinking);
    else api.toggleToolCall(card);
  });
}

