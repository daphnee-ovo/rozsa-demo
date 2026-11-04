import { useEffect, useLayoutEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { createRoot, type Root } from "react-dom/client";
import type {
  InterruptionState,
  PermissionResponse,
  ProductBridge,
  QuestionAnswers,
  UserQuestion,
  UserQuestionRequest,
} from "../../../main/session-contract.js";
import { reportError, resolveError } from "../../error-reporting.js";
import { ProductClient } from "../../product-client.js";
import {
  HINT_PREFIX,
  OTHER_VALUE,
  PermissionPanel,
  QuestionPanel,
  type PermissionPage,
} from "./permission-request-panels.js";

const EMPTY_STATE: InterruptionState = { approvals: [] };
const POLL_ERROR_ID = (sessionId: string): string => `interruption-poll:${sessionId}`;
const ACTION_ERROR_ID = (kind: string, sessionId: string, requestId: string): string =>
  `interruption-action:${kind}:${sessionId}:${requestId}`;
type PanelLocalState = {
  approvalId?: string;
  permissionPage: PermissionPage;
  trustGroupIndex: number;
  trustKeys: string[];
  commandExpanded: boolean;
  descriptionExpanded: boolean;
  hint: string;
  questionId?: string;
  questionIndex: number;
  selectedByHeader: Record<string, string[]>;
  otherByHeader: Record<string, string>;
  answersByHeader: QuestionAnswers;
  focusIndex: number;
  busyId?: string;
};

function emptyLocalState(): PanelLocalState {
  return {
    permissionPage: "main",
    trustGroupIndex: 0,
    trustKeys: [],
    commandExpanded: false,
    descriptionExpanded: false,
    hint: "",
    questionIndex: 0,
    selectedByHeader: {},
    otherByHeader: {},
    answersByHeader: {},
    focusIndex: 0,
  };
}

export interface InterruptionPanelHandle {
  setSessionId(sessionId: string): void;
  dispose(): void;
}

export function mountInterruptionPanel(
  target: Element,
  bridge: ProductBridge,
): InterruptionPanelHandle {
  if (!(target instanceof Element))
    throw new TypeError("Interruption panel mount must be an Element");
  const root: Root = createRoot(target);
  let sessionId = "";
  let disposed = false;
  const render = (): void => {
    if (!disposed) root.render(<InterruptionPanel bridge={bridge} sessionId={sessionId} />);
  };
  render();
  return {
    setSessionId(next) {
      if (disposed || next === sessionId) return;
      sessionId = next;
      render();
    },
    dispose() {
      if (disposed) return;
      disposed = true;
      root.unmount();
    },
  };
}

function InterruptionPanel({ bridge, sessionId }: { bridge: ProductBridge; sessionId: string }) {
  const client = useMemo(() => new ProductClient(bridge), [bridge]);
  const panelRef = useRef<HTMLDivElement>(null);
  const commandRef = useRef<HTMLDivElement>(null);
  const dismissedApprovals = useRef(new Set<string>());
  const dismissedQuestions = useRef(new Set<string>());
  const [states, setStates] = useState<Record<string, InterruptionState>>({});
  const [localBySession, setLocalBySession] = useState<Record<string, PanelLocalState>>({});
  const state = sessionId ? states[sessionId] || EMPTY_STATE : EMPTY_STATE;
  const approval = state.approvals.find(
    (item) =>
      !dismissedApprovals.current.has(item.requestId) &&
      (item.status === "pending" || item.status === "responding" || item.status === "failed"),
  );
  const question =
    state.question && !dismissedQuestions.current.has(state.question.requestId)
      ? state.question
      : undefined;
  const interruptionKind = approval ? "permission" : question ? "question" : undefined;
  const storedLocal = sessionId
    ? localBySession[sessionId] || emptyLocalState()
    : emptyLocalState();
  const local: PanelLocalState = {
    ...storedLocal,
    ...(storedLocal.approvalId !== approval?.requestId
      ? {
          approvalId: approval?.requestId,
          permissionPage: "main" as const,
          trustGroupIndex: 0,
          trustKeys: [],
          commandExpanded: false,
          descriptionExpanded: false,
          hint: "",
          focusIndex: 0,
        }
      : {}),
    ...(storedLocal.questionId !== question?.requestId
      ? {
          questionId: question?.requestId,
          questionIndex: 0,
          selectedByHeader: {},
          otherByHeader: {},
          answersByHeader: {},
          focusIndex: 0,
        }
      : {}),
  };
  const setLocal = (
    update: Partial<PanelLocalState> | ((current: PanelLocalState) => Partial<PanelLocalState>),
  ) => {
    if (!sessionId) return;
    setLocalBySession((previous) => {
      const stored = previous[sessionId] || emptyLocalState();
      const current = {
        ...stored,
        ...(stored.approvalId !== approval?.requestId
          ? {
              approvalId: approval?.requestId,
              permissionPage: "main" as const,
              trustGroupIndex: 0,
              trustKeys: [],
              commandExpanded: false,
              descriptionExpanded: false,
              hint: "",
              focusIndex: 0,
            }
          : {}),
        ...(stored.questionId !== question?.requestId
          ? {
              questionId: question?.requestId,
              questionIndex: 0,
              selectedByHeader: {},
              otherByHeader: {},
              answersByHeader: {},
              focusIndex: 0,
            }
          : {}),
      };
      return {
        ...previous,
        [sessionId]: { ...current, ...(typeof update === "function" ? update(current) : update) },
      };
    });
  };

  useEffect(() => {
    if (!sessionId) return;
    let active = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const controller = new AbortController();
    const poll = async (): Promise<void> => {
      if (!active) return;
      let delay = 700;
      try {
        const next = await client.getInterruptionState(sessionId, { signal: controller.signal });
        if (!active) return;
        const visible = {
          approvals: next.approvals.filter(
            (item) => !dismissedApprovals.current.has(item.requestId),
          ),
          ...(next.question && !dismissedQuestions.current.has(next.question.requestId)
            ? { question: next.question }
            : {}),
        } satisfies InterruptionState;
        setStates((previous) => ({ ...previous, [sessionId]: visible }));
        resolveError(POLL_ERROR_ID(sessionId));
      } catch (error) {
        if (!active || controller.signal.aborted) return;
        delay = 1800;
        reportError(error, {
          id: POLL_ERROR_ID(sessionId),
          title: "Could not refresh pending requests",
        });
      }
      if (active)
        timer = setTimeout(() => {
          void poll();
        }, delay);
    };
    void poll();
    return () => {
      active = false;
      controller.abort();
      if (timer) clearTimeout(timer);
    };
  }, [client, sessionId]);

  useLayoutEffect(() => {
    const command = commandRef.current;
    if (!command || !approval || local.commandExpanded) return;
    const measure = (): void => {
      const button = panelRef.current?.querySelector<HTMLButtonElement>("[data-command-toggle]");
      if (button) button.hidden = command.scrollHeight <= command.clientHeight + 1;
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(command);
    return () => observer.disconnect();
  }, [approval?.requestId, approval?.summary, local.commandExpanded]);

  useEffect(() => {
    if (!sessionId || (!approval && !question)) return;
    const focusIndex = Math.max(0, local.focusIndex);
    requestAnimationFrame(() => {
      const focusTarget = panelRef.current?.querySelector<HTMLElement>(
        `[data-interruption-focus="${focusIndex}"]`,
      );
      focusTarget?.focus();
    });
  }, [sessionId, approval?.requestId, question?.requestId]);

  useEffect(() => {
    if (!interruptionKind) return;
    document.dispatchEvent(
      new CustomEvent("rozsa:interruption-open", { detail: { kind: interruptionKind } }),
    );
    return () => {
      document.dispatchEvent(
        new CustomEvent("rozsa:interruption-close", { detail: { kind: interruptionKind } }),
      );
    };
  }, [interruptionKind]);

  async function refreshAfterSuccess(
    kind: "approval" | "question",
    requestId: string,
  ): Promise<void> {
    if (kind === "approval") dismissedApprovals.current.add(requestId);
    else dismissedQuestions.current.add(requestId);
    setStates((previous) => {
      const current = previous[sessionId] || EMPTY_STATE;
      return {
        ...previous,
        [sessionId]: {
          approvals: current.approvals.filter((item) => item.requestId !== requestId),
          ...(kind === "question" ? {} : current.question ? { question: current.question } : {}),
        },
      };
    });
    setLocal({ busyId: undefined });
  }

  async function sendPermission(response: PermissionResponse): Promise<void> {
    if (!approval || !sessionId) return;
    const requestId = approval.requestId;
    setLocal({ busyId: requestId });
    try {
      await client.respondPermission(response);
      resolveError(ACTION_ERROR_ID("permission", sessionId, requestId));
      await refreshAfterSuccess("approval", requestId);
    } catch (error) {
      setLocal({ busyId: undefined });
      reportError(error, {
        id: ACTION_ERROR_ID("permission", sessionId, requestId),
        title: "Permission response failed",
      });
      void refreshCurrent();
    }
  }

  async function sendQuestion(answers: QuestionAnswers): Promise<void> {
    if (!question || !sessionId) return;
    const requestId = question.requestId;
    setLocal({ busyId: requestId });
    try {
      await client.respondUserQuestion(requestId, answers);
      resolveError(ACTION_ERROR_ID("question", sessionId, requestId));
      await refreshAfterSuccess("question", requestId);
    } catch (error) {
      setLocal({ busyId: undefined });
      reportError(error, {
        id: ACTION_ERROR_ID("question", sessionId, requestId),
        title: "Question response failed",
      });
      void refreshCurrent();
    }
  }

  async function cancelQuestion(): Promise<void> {
    if (!question || !sessionId) return;
    const requestId = question.requestId;
    setLocal({ busyId: requestId });
    try {
      await client.cancelUserQuestion(requestId);
      resolveError(ACTION_ERROR_ID("question-cancel", sessionId, requestId));
      await refreshAfterSuccess("question", requestId);
    } catch (error) {
      setLocal({ busyId: undefined });
      reportError(error, {
        id: ACTION_ERROR_ID("question-cancel", sessionId, requestId),
        title: "Question cancellation failed",
      });
      void refreshCurrent();
    }
  }

  async function refreshCurrent(): Promise<void> {
    if (!sessionId) return;
    try {
      const next = await client.getInterruptionState(sessionId);
      setStates((previous) => ({
        ...previous,
        [sessionId]: {
          approvals: next.approvals.filter(
            (item) => !dismissedApprovals.current.has(item.requestId),
          ),
          ...(next.question && !dismissedQuestions.current.has(next.question.requestId)
            ? { question: next.question }
            : {}),
        },
      }));
      resolveError(POLL_ERROR_ID(sessionId));
    } catch (error) {
      reportError(error, {
        id: POLL_ERROR_ID(sessionId),
        title: "Could not refresh pending requests",
      });
    }
  }

  const trustGroups = approval
    ? approval.trustGroups.length
      ? approval.trustGroups
      : approval.trustLevels.length
        ? [{ target: approval.summary || approval.tool, levels: approval.trustLevels }]
        : approval.trustKey
          ? [
              {
                target: approval.summary || approval.tool,
                levels: [{ key: approval.trustKey, label: "This request" }],
              },
            ]
          : []
    : [];
  const permissionBusy =
    Boolean(approval && local.busyId === approval.requestId) || approval?.status === "responding";
  const questionBusy = Boolean(question && local.busyId === question.requestId);

  function respondTrustChoice(trustKey: string): void {
    if (!approval) return;
    const trustKeys = [...new Set([...local.trustKeys, trustKey])];
    if (local.trustGroupIndex + 1 < trustGroups.length) {
      setLocal({ trustKeys, trustGroupIndex: local.trustGroupIndex + 1, focusIndex: 0 });
      requestAnimationFrame(() =>
        panelRef.current
          ?.querySelector<HTMLElement>('[data-permission-action="trust-level"]')
          ?.focus(),
      );
      return;
    }
    void sendPermission({
      sessionId: approval.sessionId,
      requestId: approval.requestId,
      choice: "allow-project",
      trustKeys,
    });
  }

  function enterTrustPage(): void {
    if (approval && !["read", "write", "edit", "bash"].includes(approval.tool.toLowerCase())) {
      const trustKey = approval.trustKey || approval.trustLevels[0]?.key;
      if (trustKey)
        void sendPermission({
          sessionId: approval.sessionId,
          requestId: approval.requestId,
          choice: "allow-project",
          trustKey,
        });
      return;
    }
    setLocal({ permissionPage: "trust", trustGroupIndex: 0, trustKeys: [], focusIndex: 0 });
    requestAnimationFrame(() =>
      panelRef.current
        ?.querySelector<HTMLElement>('[data-permission-action="trust-level"]')
        ?.focus(),
    );
  }

  function questionAnswer(current: UserQuestion): string | string[] | null {
    const selected = local.selectedByHeader[current.header] || [];
    const other = (local.otherByHeader[current.header] || "").trim();
    if (current.multiSelect) {
      const values = selected.filter((value) => value !== OTHER_VALUE);
      if (selected.includes(OTHER_VALUE)) {
        if (!other) {
          focusOther();
          return null;
        }
        values.push(other);
      }
      if (!values.length) return null;
      return values;
    }
    const choice = selected[0];
    if (!choice) return null;
    if (choice === OTHER_VALUE) {
      if (!other) {
        focusOther();
        return null;
      }
      return other;
    }
    return choice;
  }

  function selectOption(current: UserQuestion, value: string): void {
    const selected = local.selectedByHeader[current.header] || [];
    const next = current.multiSelect
      ? selected.includes(value)
        ? selected.filter((item) => item !== value)
        : [...selected, value]
      : [value];
    setLocal({ selectedByHeader: { ...local.selectedByHeader, [current.header]: next } });
    if (value === OTHER_VALUE && !selected.includes(value)) focusOther();
  }

  function advanceQuestion(): void {
    if (!question) return;
    const current = question.questions[local.questionIndex];
    if (!current) return;
    const answer = questionAnswer(current);
    if (answer === null) return;
    const answers = { ...local.answersByHeader, [current.header]: answer };
    if (local.questionIndex + 1 < question.questions.length) {
      setLocal({ answersByHeader: answers, questionIndex: local.questionIndex + 1, focusIndex: 0 });
      requestAnimationFrame(() =>
        panelRef.current?.querySelector<HTMLElement>('[data-interruption-focus="0"]')?.focus(),
      );
      return;
    }
    void sendQuestion(answers);
  }

  function focusOther(): void {
    requestAnimationFrame(() =>
      panelRef.current?.querySelector<HTMLInputElement>("[data-question-other]")?.focus(),
    );
  }

  function handleKeyDown(event: KeyboardEvent<HTMLDivElement>): void {
    if (event.key === "Escape") {
      event.preventDefault();
      if (approval && local.permissionPage === "hint") setLocal({ permissionPage: "main" });
      else if (question) void cancelQuestion();
      else if (approval)
        void sendPermission({
          sessionId: approval.sessionId,
          requestId: approval.requestId,
          choice: "deny",
        });
      return;
    }
    const target = event.target as HTMLElement;
    const isInput = target instanceof HTMLInputElement;
    const isTextInput = isInput && (target as HTMLInputElement).type === "text";
    if (approval && !permissionBusy) {
      if (local.permissionPage === "hint") {
        if (isInput && (event.key === "Backspace" || event.key === "Delete")) {
          const input = target as HTMLInputElement;
          if (
            (input.selectionStart || 0) <= HINT_PREFIX.length &&
            (input.selectionEnd || 0) <= HINT_PREFIX.length
          ) {
            event.preventDefault();
            input.setSelectionRange(HINT_PREFIX.length, HINT_PREFIX.length);
          }
        }
        if (isInput && event.key === "Enter" && !event.shiftKey) {
          event.preventDefault();
          void sendPermission({
            sessionId: approval.sessionId,
            requestId: approval.requestId,
            choice: "deny-hint",
            hint: local.hint.trim(),
          });
        }
        return;
      }
      if (local.permissionPage === "main") {
        const choice = event.key.toLowerCase();
        if (choice === "y") {
          event.preventDefault();
          void sendPermission({
            sessionId: approval.sessionId,
            requestId: approval.requestId,
            choice: "allow",
          });
        } else if (choice === "t") {
          event.preventDefault();
          enterTrustPage();
        } else if (choice === "n") {
          event.preventDefault();
          void sendPermission({
            sessionId: approval.sessionId,
            requestId: approval.requestId,
            choice: "deny",
          });
        } else if (choice === "h") {
          event.preventDefault();
          setLocal({ permissionPage: "hint", hint: "" });
          requestAnimationFrame(() =>
            panelRef.current?.querySelector<HTMLInputElement>("[data-permission-hint]")?.focus(),
          );
        }
      } else if (local.permissionPage === "trust" && /^[1-9]$/.test(event.key)) {
        const index = Number(event.key) - 1;
        const group = trustGroups[local.trustGroupIndex];
        if (group && index < group.levels.length) {
          event.preventDefault();
          respondTrustChoice(group.levels[index].key);
        }
      } else if (local.permissionPage === "trust" && ["j", "k"].includes(event.key.toLowerCase())) {
        const options = panelRef.current?.querySelector<HTMLElement>(".trust-options");
        if (options && options.scrollHeight > options.clientHeight) {
          event.preventDefault();
          const first = options.querySelector<HTMLElement>(".perm-panel-opt");
          const step =
            (first?.getBoundingClientRect().height || 44) +
            (Number.parseFloat(getComputedStyle(options).rowGap) || 0);
          options.scrollBy({
            top: event.key.toLowerCase() === "j" ? step : -step,
            behavior: "auto",
          });
        }
      }
      if (!isTextInput && event.key.toLowerCase() === "z") {
        event.preventDefault();
        setLocal({
          commandExpanded: !local.commandExpanded,
          descriptionExpanded: !local.descriptionExpanded,
        });
      }
    }
    if (question && !questionBusy && !isTextInput) {
      if (/^[1-9]$/.test(event.key)) {
        const current = question.questions[local.questionIndex];
        const optionCount = (current?.options.length || 0) + 1;
        const index = Number(event.key) - 1;
        if (current && index < optionCount) {
          event.preventDefault();
          selectOption(
            current,
            index === current.options.length ? OTHER_VALUE : current.options[index].label,
          );
        }
      } else if (event.key.toLowerCase() === "n") {
        event.preventDefault();
        advanceQuestion();
      } else if (event.key.toLowerCase() === "d" || event.key === "Enter") {
        event.preventDefault();
        advanceQuestion();
      }
    } else if (
      question &&
      !questionBusy &&
      isTextInput &&
      event.key === "Enter" &&
      !event.shiftKey
    ) {
      event.preventDefault();
      advanceQuestion();
    }
  }

  if (!sessionId) return null;
  if (!approval && !question) return null;
  return (
    <div ref={panelRef} className="interruption-panel-root" onKeyDown={handleKeyDown}>
      {approval && (
        <PermissionPanel
          request={approval}
          pendingCount={state.approvals.filter((item) => item.status !== "cancelled").length}
          page={local.permissionPage}
          trustGroup={trustGroups[local.trustGroupIndex]}
          commandExpanded={local.commandExpanded}
          descriptionExpanded={local.descriptionExpanded}
          hint={local.hint}
          busy={permissionBusy}
          commandRef={commandRef}
          onFocus={(index) => setLocal({ focusIndex: index })}
          onAllow={() =>
            void sendPermission({
              sessionId: approval.sessionId,
              requestId: approval.requestId,
              choice: "allow",
            })
          }
          onDeny={() =>
            void sendPermission({
              sessionId: approval.sessionId,
              requestId: approval.requestId,
              choice: "deny",
            })
          }
          onTrust={enterTrustPage}
          onTrustChoice={respondTrustChoice}
          onBack={() =>
            setLocal({ permissionPage: "main", trustGroupIndex: 0, trustKeys: [], focusIndex: 0 })
          }
          onHint={() => {
            setLocal({ permissionPage: "hint", hint: "" });
            requestAnimationFrame(() =>
              panelRef.current?.querySelector<HTMLInputElement>("[data-permission-hint]")?.focus(),
            );
          }}
          onHintChange={(hint) => setLocal({ hint })}
          onCancelHint={() => setLocal({ permissionPage: "main" })}
          onSubmitHint={() =>
            void sendPermission({
              sessionId: approval.sessionId,
              requestId: approval.requestId,
              choice: "deny-hint",
              hint: local.hint.trim(),
            })
          }
          onToggleCommand={() => setLocal({ commandExpanded: !local.commandExpanded })}
          onToggleDescription={() => setLocal({ descriptionExpanded: !local.descriptionExpanded })}
        />
      )}
      {!approval && question && (
        <QuestionPanel
          request={question}
          index={local.questionIndex}
          selectedByHeader={local.selectedByHeader}
          otherByHeader={local.otherByHeader}
          busy={questionBusy}
          onFocus={(index) => setLocal({ focusIndex: index })}
          onSelect={selectOption}
          onOtherChange={(header, value) =>
            setLocal({ otherByHeader: { ...local.otherByHeader, [header]: value } })
          }
          onAdvance={advanceQuestion}
          onBack={() =>
            setLocal({ questionIndex: Math.max(0, local.questionIndex - 1), focusIndex: 0 })
          }
          onCancel={() => void cancelQuestion()}
        />
      )}
    </div>
  );
}
