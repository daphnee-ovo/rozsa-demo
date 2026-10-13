import { RozsaBridge } from "./bridge.js";
import { contextBridge, ipcRenderer } from "electron";
contextBridge.exposeInMainWorld("rozsa", RozsaBridge.create((channel, ...args) => ipcRenderer.invoke(channel, ...args)));
