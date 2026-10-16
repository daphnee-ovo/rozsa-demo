// This adapter classifies explicit runtime states; it never invents results or
// maps a failed/cancelled tool to the prototype's successful fixture.
export function toolView(toolCall, result) {
  const name = String(toolCall.name || '').toLowerCase();
  const details = result?.details || {};
  if (result?.isError || details.success === false || (name === 'bash' && Number.isFinite(details.exit_code) && details.exit_code !== 0)) {
    return { disposition: 'blocked', coverage: name === 'read' || name === 'edit' || name === 'bash' ? 'UI-TOOL-001' : 'UI-TOOL-002' };
  }
  if ((name === 'askuserquestion' || name === 'ask_user_question') && !result) {
    return { disposition: 'blocked', coverage: 'UI-TOOL-002' };
  }
  const coverage = ['read', 'edit', 'bash'].includes(name) ? 'UI-TOOL-001' : 'UI-TOOL-002';
  if (!result || details.truncated || !['read', 'edit', 'bash'].includes(name)) return { disposition: 'suspended', coverage };
  if (name === 'edit') {
    const deltas = Array.isArray(details.file_deltas) ? details.file_deltas : [];
    if (deltas.length !== 1 || !deltas[0].patch || deltas[0].truncated) return { disposition: 'suspended', coverage };
    return { disposition: 'covered', kind: 'diff', content: deltas[0].patch, status: 's-success' };
  }
  const output = String(result.output ?? '');
  if (!output) return { disposition: 'suspended', coverage };
  return { disposition: 'covered', kind: 'output', label: 'Output', output, status: 's-success' };
}

export function activityView(activity) {
  const files = activity?.fileChanges;
  const verification = activity?.verification;
  if (activity?.captureComplete === false || !Array.isArray(files) || !files.length || files.some(file =>
    !['added', 'modified'].includes(file.status) || typeof file.patch !== 'string' || !Number.isFinite(file.added) || !Number.isFinite(file.deleted)
  )) return { disposition: 'blocked' };
  if (verification && (verification.success !== true || verification.timedOut || verification.truncated || !Number.isFinite(verification.exitCode) || !Number.isFinite(verification.durationMs))) {
    return { disposition: 'blocked' };
  }
  return { disposition: 'covered', files, verification: verification ? {
    command: verification.command, exitCode: verification.exitCode, duration: verification.durationMs + 'ms',
  } : null };
}
