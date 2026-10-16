import type { Disposer, RequestOptions } from '../../scripts/core/ports.ts';

export interface TauriAPI {
  core: { invoke(command: string, args?: unknown): Promise<unknown> };
  event: {
    listen(event: string, receive: (message: { payload: unknown }) => void): Promise<Disposer>;
    emit(event: string, payload?: unknown): Promise<void>;
  };
}

// Each persistent WebView retains its existing bridge and product state source.
export function createTauriAdapter(getApi: () => TauriAPI | undefined = () => window.__TAURI__) {
  function api(): TauriAPI {
    const value = getApi();
    if (!value?.core?.invoke || !value?.event?.listen) throw new Error("Tauri API not loaded");
    return value;
  }
  return Object.freeze({
    request(command: string, args?: unknown, { signal }: RequestOptions = {}) {
      signal?.throwIfAborted();
      return api().core.invoke(command, args);
    },
    subscribe(event: string, receive: (payload: unknown) => void) {
      return api().event.listen(event, message => receive(message.payload));
    },
    emit(event: string, payload?: unknown) {
      return api().event.emit(event, payload);
    },
  });
}
