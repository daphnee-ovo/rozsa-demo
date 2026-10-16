import { init as bindSidebar, suspendQuotaTooltip } from '../../../desktop/renderer/scripts/features/sidebar.js';
export function init(api) {
  suspendQuotaTooltip();
  bindSidebar({ ...api, openDevFlowDashboard: api.openDevFlowDashboardDemo,
    activateSession: index => { api.state.activeSession = index; api.renderSessionList(); api.renderMessages(); api.saveState(); },
  });
}
