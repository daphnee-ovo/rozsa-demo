export function createInterruptionAdapter(handlers) {
  return {
    respondPermission: handlers.respondPermission,
    enterPermissionTrust: handlers.enterPermissionTrust,
    enterPermissionHint: handlers.enterPermissionHint,
    togglePermissionCommand: handlers.togglePermissionCommand,
    showPermissionMainPage: handlers.showPermissionMainPage,
    submitPermissionHint: handlers.submitPermissionHint,
    submitQuestionDemo: handlers.submitUserQuestion,
    normalizePermissionHint: handlers.normalizePermissionHint,
    handlePermissionHintKeydown: handlers.handlePermissionHintKeydown,
  };
}
