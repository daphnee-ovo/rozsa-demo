const widthBreakpoints = [560, 760, 780, 900, 1100];
type ViewportWindow = Window & typeof globalThis;

// Both hosts supply the complete window width, never a split pane's width.
export async function bindViewport(view: ViewportWindow, readWidth: () => number | Promise<number> = () => view.innerWidth): Promise<() => void> {
  let revision = 0;
  let disposed = false;
  const root = view.document.documentElement;
  async function refresh(): Promise<void> {
    const current = ++revision;
    const width = await readWidth();
    if (!Number.isFinite(width) || width <= 0) throw new Error('Invalid UI viewport width: ' + width);
    if (disposed || current !== revision) return;
    root.style.setProperty('--ui-vw', `${width / 100}px`);
    root.setAttribute('data-ui-max-width', widthBreakpoints.filter(limit => width <= limit).join(' '));
  }
  const resize = () => { void refresh().catch((error: unknown) => view.dispatchEvent(new view.ErrorEvent('error', { error, message: error instanceof Error ? error.message : String(error) }))); };
  await refresh();
  view.addEventListener('resize', resize);
  const dispose = () => {
    disposed = true;
    revision++;
    view.removeEventListener('resize', resize);
    view.removeEventListener('pagehide', dispose);
  };
  view.addEventListener('pagehide', dispose, { once: true });
  return dispose;
}
