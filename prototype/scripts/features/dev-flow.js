import { init as bindDevFlow } from '../../../crates/rozsa-gui/frontend/ui/scripts/features/dev-flow.js';

export function init(api) {
  bindDevFlow({ ...api, openDevFlowDashboard: api.openDevFlowDashboardDemo, openDevFlowDetail: api.openDevFlowDetailDemo });
}
