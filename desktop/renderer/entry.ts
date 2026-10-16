import "./styles/foundations/tokens.css";
import "./styles/features/sidebar.css";
import type { ProductBridge } from "../main/session-contract.js";
import { ProductClient } from "./product-client.js";
import { SessionList } from "./features/session-list.js";
import { SessionHistory } from "./features/session-history.js";

declare global { interface Window { rozsa: ProductBridge } }
const list = document.querySelector<HTMLElement>("#sessions");
const status = document.querySelector<HTMLElement>("#status");
const refresh = document.querySelector<HTMLButtonElement>("#refresh");
if (!list || !status || !refresh) throw new Error("Session list elements missing");
const container = document.querySelector<HTMLElement>("#history");
const title = document.querySelector<HTMLElement>("#history-title");
const historyStatus = document.querySelector<HTMLElement>("#history-status");
const retry = document.querySelector<HTMLButtonElement>("#retry-history");
if (!container || !title || !historyStatus || !retry) throw new Error("Session history elements missing");
const client = new ProductClient(window.rozsa);
const history = new SessionHistory(client, container, title, historyStatus, retry, path => feature.setActive(path));
const feature = new SessionList(client, list, status, refresh, entry => { void history.switchTo(entry); });
const reception = new AbortController();
window.addEventListener("pagehide", () => reception.abort("Session page closed"), { once: true });
void client.syncSessions(active => history.restore(active), (error, resynchronizing) => history.syncFailed(error, resynchronizing), { signal: reception.signal });
void feature.load();
