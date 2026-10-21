export interface ErrorReportOptions {
  id?: string;
  title?: string;
}

interface ErrorNotification {
  id: string;
  severity: "error";
  title: string;
  message: string;
  timeoutMs: number;
}

type NotificationWindow = Window & {
  upsertNotification: (notification: ErrorNotification) => void;
  resolveNotification: (id: string) => void;
};

const REPORT_CACHE_LIMIT = 256;
const DUPLICATE_WINDOW_MS = 6000;
const reports = new Map<string, { message: string; reportedAt: number }>();
let nextErrorId = 0;

function messageFor(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function remember(id: string, message: string, reportedAt: number): void {
  reports.delete(id);
  reports.set(id, { message, reportedAt });
  while (reports.size > REPORT_CACHE_LIMIT) reports.delete(reports.keys().next().value!);
}

export function reportError(error: unknown, options: ErrorReportOptions = {}): string {
  const message = messageFor(error);
  window.console.error(error);

  const id = options.id || `runtime-error-${++nextErrorId}`;
  const previous = reports.get(id);
  const now = Date.now();
  if (previous?.message === message && now - previous.reportedAt < DUPLICATE_WINDOW_MS) return id;

  const notificationWindow = window as NotificationWindow;
  if (typeof notificationWindow.upsertNotification !== "function") return id;

  try {
    notificationWindow.upsertNotification({
      id,
      severity: "error",
      title: options.title || "Error",
      message,
      timeoutMs: DUPLICATE_WINDOW_MS,
    });
    remember(id, message, now);
  } catch (deliveryError) {
    window.console.error("Could not deliver runtime error notification:", deliveryError);
  }
  return id;
}

export function resolveError(id: string): void {
  if (!reports.delete(id)) return;

  const notificationWindow = window as NotificationWindow;
  if (typeof notificationWindow.resolveNotification !== "function") return;

  try {
    notificationWindow.resolveNotification(id);
  } catch (deliveryError) {
    window.console.error("Could not resolve runtime error notification:", deliveryError);
  }
}
