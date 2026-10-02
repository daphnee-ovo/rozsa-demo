import { init as bindSettings } from '../../../crates/rozsa-gui/frontend/ui/scripts/features/settings.js';

export function init(api) {
  bindSettings({ ...api,
    previewFontSize: (value, control) => api.setFontSize(value, false, control.id === 'settingsFontSizeRange'),
    commitFontSize: api.setFontSize,
  });
}
