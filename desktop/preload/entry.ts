import { RozsaBridge } from "./bridge.js";
import { contextBridge, ipcRenderer } from "electron";
contextBridge.exposeInMainWorld("rozsa", RozsaBridge.create((channel, ...args) => ipcRenderer.invoke(channel, ...args), (channel, receive) => {
  const listener = (_event: Electron.IpcRendererEvent, value: unknown) => receive(value);
  ipcRenderer.on(channel, listener);
  return () => { ipcRenderer.removeListener(channel, listener); };
}));
