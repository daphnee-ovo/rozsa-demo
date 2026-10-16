import { init as bindSettings } from '../../../desktop/renderer/scripts/features/settings.js';

export function init(api) {
  bindSettings({ ...api,
    previewFontSize: (value, control) => api.setFontSize(value, false, control.id === 'settingsFontSizeRange'),
    commitFontSize: api.setFontSize,
  });
}
