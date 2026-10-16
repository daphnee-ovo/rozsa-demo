// Product handlers retain session drafts, selection, IME, send/abort and IPC ownership.
export function createConversationAdapter(handlers) {
  return Object.freeze({
    attachFile: handlers.attachFileReference,
    attachDirectory: handlers.attachDirectoryReference,
    slashCommand: handlers.insertSlashCommandPrefix,
    chooseModel: handlers.showModelPicker,
    submit: handlers.submit,
    refreshSendState: handlers.updateAbortButton,
  });
}
