import type { createUIHost } from '../scripts/core/renderer.ts';
import type { TauriAPI } from './adapters/tauri.ts';

// These bindings are owned by the unchanged classic app.js/sidebar.js scripts.
// The entries consume their browser-global API without importing/reinitializing it.
declare global {
  interface Window {
    __TAURI__: TauriAPI;
    RozsaUIHost: ReturnType<typeof createUIHost>;
    RozsaUIReady: Promise<void>;
    RozsaSidebarEmit: (event: string, payload?: unknown) => Promise<void>;
    RozsaSidebar: typeof import('../scripts/features/sidebar.js') & Partial<typeof import('../scripts/features/dev-flow.js')>;
    RozsaNotifications: typeof import('../scripts/features/notifications.js') & Omit<typeof import('../scripts/components/notification.js'), 'init'> & Pick<typeof import('./adapters/notifications.js'), 'subscribeNotifications' | 'showRuntimeError'>;
    RozsaSettings: Partial<typeof import('../scripts/features/settings.js')> & Partial<typeof import('./adapters/settings.js')>;
    RozsaDevFlow: typeof import('../scripts/features/dev-flow.js') & typeof import('./adapters/sidebar-dev-flow.js');
    RozsaTools: typeof import('../scripts/features/tools.js') & typeof import('./adapters/tools.js');
    RozsaPermissions: typeof import('../scripts/features/permissions.js');
    RozsaConversation: Pick<typeof import('../scripts/features/conversation.ts'), 'emptyConversationMarkup' | 'messageFrame' | 'createMessageFrame' | 'runningPanelMarkup' | 'thinkingMarkup'>;
    RozsaComposer: Pick<typeof import('../scripts/components/composer.js'), 'renderSendState'>;
    RozsaPopover: ReturnType<typeof import('../scripts/components/popover.js')['init']>;
    RozsaSidebarFeedback: ReturnType<typeof import('./adapters/notifications.js')['createSidebarFeedback']>;
  }
  const isStreaming: boolean;
  const sidebarSessions: { path: string }[];
  function switchSidebarSession(path: string): Promise<void>;
  function newSidebarSession(): Promise<void>;
  function openSidebarSettings(): void;
  function openDevFlowDashboard(): Promise<void>;
  function selectSidebarSettingsPane(pane: string): void;
  function closeSidebarSettings(): void;
  function closeDevFlowDetail(): void;
  function switchSettingsTab(tabId: string, button?: HTMLElement): void;
  function closeSettings(): void;
  function selectThemeModeCard(mode: string): void;
  function previewFontSize(value: string | number, control?: HTMLElement | null): void;
  function commitFontSize(value: string | number): void;
  function toggleToolCall(element: HTMLElement): void;
  function toggleTurnDiff(button: HTMLElement): void;
  function respondPermission(choice: string, hint?: string | null): Promise<void>;
  function enterPermissionTrust(): void;
  function enterPermissionHint(): void;
  function togglePermissionCommand(): void;
  function showPermissionMainPage(): void;
  function submitPermissionHint(): Promise<void>;
  function submitUserQuestion(): Promise<void>;
  function normalizePermissionHint(input: HTMLElement): void;
  function handlePermissionHintKeydown(event: KeyboardEvent): void;
  function configureInterruptionSuspensions(): void;
  function configureConversationSuspensions(): void;
  function toggleThinking(element: HTMLElement): void;
  function attachFileReference(): Promise<void>;
  function attachDirectoryReference(): Promise<void>;
  function insertSlashCommandPrefix(): void;
  function showModelPicker(): void;
  function updateAbortButton(): void;
  function abortAgent(): Promise<void>;
  function sendMessage(): Promise<void>;
  function handleInput(input: HTMLElement, event: InputEvent): void;
  function handleCompositionStart(event: CompositionEvent): void;
  function handleCompositionUpdate(event: CompositionEvent): void;
  function handleCompositionEnd(event: CompositionEvent): void;
  function dismissTransientPopups(): void;
  function renderThinkingEffortPicker(): void;
  function previewThinkingEffort(index: number): void;
  function selectThinkingEffort(index: number): Promise<void>;
}
