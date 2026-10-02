// Each persistent WebView retains its existing bridge and product state source.
export function createTauriAdapter(getApi = () => window.__TAURI__) {
  function api() {
    const value = getApi();
    if (!value?.core?.invoke || !value?.event?.listen) throw new Error("Tauri API not loaded");
    return value;
  }
  return Object.freeze({
    request(command, args, { signal } = {}) {
      signal?.throwIfAborted();
      return api().core.invoke(command, args);
    },
    subscribe(event, receive) {
      return api().event.listen(event, message => receive(message.payload));
    },
    emit(event, payload) {
      return api().event.emit(event, payload);
    },
  });
}
