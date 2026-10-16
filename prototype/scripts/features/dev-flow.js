import { init as bindDevFlow } from '../../../desktop/renderer/scripts/features/dev-flow.js';

export function init(api) {
  bindDevFlow({ ...api, openDevFlowDashboard: api.openDevFlowDashboardDemo, openDevFlowDetail: api.openDevFlowDetailDemo });
}
