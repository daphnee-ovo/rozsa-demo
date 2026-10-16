export function devFlowSettingsView(settings) {
  if (!settings) return null;
  const bytes = settings.project?.memoryUseBytes;
  const memory = Number.isFinite(bytes) ? { amount: (bytes / (1024 * 1024)).toFixed(bytes < 10 * 1024 * 1024 ? 1 : 0), unit: 'MiB' } : { amount: 'Unavailable', unit: '' };
  const url = settings.project?.dashboardUrl || '';
  const availability = !settings.cli.available ? 'Unavailable' : settings.project ? settings.project.availability.charAt(0).toUpperCase() + settings.project.availability.slice(1) : 'No active project';
  return { enabled: settings.enabled, sidebarEnabled: settings.showSidebarStatus, dashboardEnabled: settings.showDashboardButton, availability,
    address: url.replace(/^https?:\/\//, '') || 'Unavailable', dashboardAvailable: Boolean(url), version: settings.cli.available ? (settings.cli.version || 'unknown') : 'unknown',
    memory, executablePath: settings.executablePath || settings.cli.executable || 'Not detected', dependentDisabled: !settings.enabled || !settings.cli.available, missing: !settings.cli.available };
}

const countLabel = (count, noun) => count === 1 ? '1 ' + noun : count + ' ' + noun + 's';

export function devFlowDetailView(payload) {
  return { revision: payload.revision ? '#' + payload.revision : '', project: (payload.project.root || '') + ' · ' + (payload.project.revision || ''),
    summary: countLabel(payload.openTasks, 'Task') + ' · ' + countLabel(payload.openIssues, 'Issue') + (payload.stale ? ' · stale' : ''),
    items: payload.items.map(item => ({ id: item.shortId || item.id, title: item.title || '', status: item.status || '', focus: payload.focusId === item.id,
      label: (item.kind === 'issue' ? 'Issue ' : 'Task ') + (item.shortId || item.id) + ' ' + item.title,
      meta: [item.priority, item.complexity, item.taskType, item.severity, item.dependsOn?.length ? 'depends: ' + item.dependsOn.join(', ') : '', item.refs].filter(Boolean).join(' · '),
      description: item.description, files: [...(item.filesCreate || []).map(file => 'CREATE: ' + file), ...(item.filesModify || []).map(file => 'MODIFY: ' + file), ...(item.filesTest || []).map(file => 'TEST: ' + file)] })) };
}
