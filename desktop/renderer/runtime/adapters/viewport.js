import { bindViewport } from '../../scripts/core/viewport.ts';

export async function readWindowWidth(api) {
  const nativeWindow = api.window.getCurrentWindow();
  // macOS FullSizeContentView spans the window frame. innerSize follows the
  // detached original content view after native split installation.
  const [size, scale] = await Promise.all([nativeWindow.outerSize(), nativeWindow.scaleFactor()]);
  if (!Number.isFinite(size.width) || size.width <= 0 || !Number.isFinite(scale) || scale <= 0) {
    throw new Error('Invalid native window geometry');
  }
  return size.width / scale;
}

export function bindNativeViewport(view = window) {
  return view.RozsaGuiShared.isNativeSplitPlatform()
    ? bindViewport(view, () => readWindowWidth(view.__TAURI__))
    : bindViewport(view);
}
