import { isSettingsPaneAvailable } from '../../ui/scripts/features/settings.js';

const switchHandlers = new WeakMap();

export function bindSettingSwitch(control, onChange, read, render) {
  control.onclick = null;
  switchHandlers.set(control, () => {
    const enabled = !read(control);
    render(control, enabled);
    return onChange(enabled);
  });
}

export function createSettingsAdapter(actions) {
  return Object.freeze({
    ...actions,
    toggleSetting: control => switchHandlers.get(control)?.(),
    switchSettingsTab: (pane, button) => {
      if (isSettingsPaneAvailable(pane)) actions.switchSettingsTab(pane, button);
    },
  });
}
