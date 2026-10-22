import * as React from "react";
import { useEffect, useLayoutEffect, useMemo, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { createRoot, type Root } from "react-dom/client";
import { ArrowLeft, ArrowRight, Check, X } from "lucide-react";
import type { ApprovalRequest, InterruptionState, PermissionResponse, ProductBridge, QuestionAnswers, UserQuestion, UserQuestionRequest } from "../../../main/session-contract.js";
import { reportError, resolveError } from "../../error-reporting.js";
import { ProductClient } from "../../product-client.js";

const HINT_PREFIX = "Deny, ";
const OTHER_VALUE = "__rozsa_other__";
const EMPTY_STATE: InterruptionState = { approvals: [] };
const POLL_ERROR_ID = (sessionId: string): string => `interruption-poll:${sessionId}`;
const ACTION_ERROR_ID = (kind: string, sessionId: string, requestId: string): string =>
  `interruption-action:${kind}:${sessionId}:${requestId}`;
type PermissionPage = "main" | "trust" | "hint";
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
    permissionPage: "main", trustGroupIndex: 0, trustKeys: [], commandExpanded: false, descriptionExpanded: false,
    hint: "", questionIndex: 0, selectedByHeader: {}, otherByHeader: {}, answersByHeader: {},
    focusIndex: 0,
  };
}

export interface InterruptionPanelHandle {
  setSessionId(sessionId: string): void;
  dispose(): void;
}

export function mountInterruptionPanel(target: Element, bridge: ProductBridge): InterruptionPanelHandle {
  if (!(target instanceof Element)) throw new TypeError("Interruption panel mount must be an Element");
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
  const approval = state.approvals.find(item => !dismissedApprovals.current.has(item.requestId)
    && (item.status === "pending" || item.status === "responding" || item.status === "failed"));
  const question = state.question && !dismissedQuestions.current.has(state.question.requestId) ? state.question : undefined;
  const interruptionKind = approval ? "permission" : question ? "question" : undefined;
  const storedLocal = sessionId ? localBySession[sessionId] || emptyLocalState() : emptyLocalState();
  const local: PanelLocalState = {
    ...storedLocal,
    ...(storedLocal.approvalId !== approval?.requestId ? { approvalId: approval?.requestId, permissionPage: "main" as const, trustGroupIndex: 0, trustKeys: [], commandExpanded: false, descriptionExpanded: false, hint: "", focusIndex: 0 } : {}),
    ...(storedLocal.questionId !== question?.requestId ? { questionId: question?.requestId, questionIndex: 0, selectedByHeader: {}, otherByHeader: {}, answersByHeader: {}, focusIndex: 0 } : {}),
  };
  const setLocal = (update: Partial<PanelLocalState> | ((current: PanelLocalState) => Partial<PanelLocalState>)) => {
    if (!sessionId) return;
    setLocalBySession(previous => {
      const stored = previous[sessionId] || emptyLocalState();
      const current = {
        ...stored,
        ...(stored.approvalId !== approval?.requestId ? { approvalId: approval?.requestId, permissionPage: "main" as const, trustGroupIndex: 0, trustKeys: [], commandExpanded: false, descriptionExpanded: false, hint: "", focusIndex: 0 } : {}),
        ...(stored.questionId !== question?.requestId ? { questionId: question?.requestId, questionIndex: 0, selectedByHeader: {}, otherByHeader: {}, answersByHeader: {}, focusIndex: 0 } : {}),
      };
      return { ...previous, [sessionId]: { ...current, ...(typeof update === "function" ? update(current) : update) } };
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
          approvals: next.approvals.filter(item => !dismissedApprovals.current.has(item.requestId)),
          ...(next.question && !dismissedQuestions.current.has(next.question.requestId) ? { question: next.question } : {}),
        } satisfies InterruptionState;
        setStates(previous => ({ ...previous, [sessionId]: visible }));
        resolveError(POLL_ERROR_ID(sessionId));
      } catch (error) {
        if (!active || controller.signal.aborted) return;
        delay = 1800;
        reportError(error, { id: POLL_ERROR_ID(sessionId), title: "Could not refresh pending requests" });
      }
      if (active) timer = setTimeout(() => { void poll(); }, delay);
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
      const focusTarget = panelRef.current?.querySelector<HTMLElement>(`[data-interruption-focus="${focusIndex}"]`);
      focusTarget?.focus();
    });
  }, [sessionId, approval?.requestId, question?.requestId]);

  useEffect(() => {
    if (!interruptionKind) return;
    document.dispatchEvent(new CustomEvent("rozsa:interruption-open", { detail: { kind: interruptionKind } }));
    return () => { document.dispatchEvent(new CustomEvent("rozsa:interruption-close", { detail: { kind: interruptionKind } })); };
  }, [interruptionKind]);

  async function refreshAfterSuccess(kind: "approval" | "question", requestId: string): Promise<void> {
    if (kind === "approval") dismissedApprovals.current.add(requestId);
    else dismissedQuestions.current.add(requestId);
    setStates(previous => {
      const current = previous[sessionId] || EMPTY_STATE;
      return { ...previous, [sessionId]: {
        approvals: current.approvals.filter(item => item.requestId !== requestId),
        ...(kind === "question" ? {} : current.question ? { question: current.question } : {}),
      } };
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
      reportError(error, { id: ACTION_ERROR_ID("permission", sessionId, requestId), title: "Permission response failed" });
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
      reportError(error, { id: ACTION_ERROR_ID("question", sessionId, requestId), title: "Question response failed" });
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
      reportError(error, { id: ACTION_ERROR_ID("question-cancel", sessionId, requestId), title: "Question cancellation failed" });
      void refreshCurrent();
    }
  }

  async function refreshCurrent(): Promise<void> {
    if (!sessionId) return;
    try {
      const next = await client.getInterruptionState(sessionId);
      setStates(previous => ({ ...previous, [sessionId]: {
        approvals: next.approvals.filter(item => !dismissedApprovals.current.has(item.requestId)),
        ...(next.question && !dismissedQuestions.current.has(next.question.requestId) ? { question: next.question } : {}),
      } }));
      resolveError(POLL_ERROR_ID(sessionId));
    } catch (error) {
      reportError(error, { id: POLL_ERROR_ID(sessionId), title: "Could not refresh pending requests" });
    }
  }

  const trustGroups = approval ? approval.trustGroups.length ? approval.trustGroups
    : approval.trustLevels.length ? [{ target: approval.summary || approval.tool, levels: approval.trustLevels }]
      : approval.trustKey ? [{ target: approval.summary || approval.tool, levels: [{ key: approval.trustKey, label: "This request" }] }]
        : [] : [];
  const permissionBusy = Boolean(approval && local.busyId === approval.requestId) || approval?.status === "responding";
  const questionBusy = Boolean(question && local.busyId === question.requestId);

  function respondTrustChoice(trustKey: string): void {
    if (!approval) return;
    const trustKeys = [...new Set([...local.trustKeys, trustKey])];
    if (local.trustGroupIndex + 1 < trustGroups.length) {
      setLocal({ trustKeys, trustGroupIndex: local.trustGroupIndex + 1, focusIndex: 0 });
      requestAnimationFrame(() => panelRef.current?.querySelector<HTMLElement>('[data-permission-action="trust-level"]')?.focus());
      return;
    }
    void sendPermission({ sessionId: approval.sessionId, requestId: approval.requestId, choice: "allow-project", trustKeys });
  }

  function enterTrustPage(): void {
    if (approval && !["read", "write", "edit", "bash"].includes(approval.tool.toLowerCase())) {
      const trustKey = approval.trustKey || approval.trustLevels[0]?.key;
      if (trustKey) void sendPermission({ sessionId: approval.sessionId, requestId: approval.requestId, choice: "allow-project", trustKey });
      return;
    }
    setLocal({ permissionPage: "trust", trustGroupIndex: 0, trustKeys: [], focusIndex: 0 });
    requestAnimationFrame(() => panelRef.current?.querySelector<HTMLElement>('[data-permission-action="trust-level"]')?.focus());
  }

  function questionAnswer(current: UserQuestion): string | string[] | null {
    const selected = local.selectedByHeader[current.header] || [];
    const other = (local.otherByHeader[current.header] || "").trim();
    if (current.multiSelect) {
      const values = selected.filter(value => value !== OTHER_VALUE);
      if (selected.includes(OTHER_VALUE)) {
        if (!other) { focusOther(); return null; }
        values.push(other);
      }
      if (!values.length) return null;
      return values;
    }
    const choice = selected[0];
    if (!choice) return null;
    if (choice === OTHER_VALUE) {
      if (!other) { focusOther(); return null; }
      return other;
    }
    return choice;
  }

  function selectOption(current: UserQuestion, value: string): void {
    const selected = local.selectedByHeader[current.header] || [];
    const next = current.multiSelect
      ? selected.includes(value) ? selected.filter(item => item !== value) : [...selected, value]
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
      requestAnimationFrame(() => panelRef.current?.querySelector<HTMLElement>("[data-interruption-focus=\"0\"]")?.focus());
      return;
    }
    void sendQuestion(answers);
  }

  function focusOther(): void {
    requestAnimationFrame(() => panelRef.current?.querySelector<HTMLInputElement>("[data-question-other]")?.focus());
  }

  function handleKeyDown(event: KeyboardEvent<HTMLDivElement>): void {
    if (event.key === "Escape") {
      event.preventDefault();
      if (approval && local.permissionPage === "hint") setLocal({ permissionPage: "main" });
      else if (question) void cancelQuestion();
      else if (approval) void sendPermission({ sessionId: approval.sessionId, requestId: approval.requestId, choice: "deny" });
      return;
    }
    const target = event.target as HTMLElement;
    const isInput = target instanceof HTMLInputElement;
    const isTextInput = isInput && (target as HTMLInputElement).type === "text";
    if (approval && !permissionBusy) {
      if (local.permissionPage === "hint") {
        if (isInput && (event.key === "Backspace" || event.key === "Delete")) {
          const input = target as HTMLInputElement;
          if ((input.selectionStart || 0) <= HINT_PREFIX.length && (input.selectionEnd || 0) <= HINT_PREFIX.length) {
            event.preventDefault();
            input.setSelectionRange(HINT_PREFIX.length, HINT_PREFIX.length);
          }
        }
        if (isInput && event.key === "Enter" && !event.shiftKey) {
          event.preventDefault();
          void sendPermission({ sessionId: approval.sessionId, requestId: approval.requestId, choice: "deny-hint", hint: local.hint.trim() });
        }
        return;
      }
      if (local.permissionPage === "main") {
        const choice = event.key.toLowerCase();
        if (choice === "y") { event.preventDefault(); void sendPermission({ sessionId: approval.sessionId, requestId: approval.requestId, choice: "allow" }); }
        else if (choice === "t") { event.preventDefault(); enterTrustPage(); }
        else if (choice === "n") { event.preventDefault(); void sendPermission({ sessionId: approval.sessionId, requestId: approval.requestId, choice: "deny" }); }
        else if (choice === "h") { event.preventDefault(); setLocal({ permissionPage: "hint", hint: "" }); requestAnimationFrame(() => panelRef.current?.querySelector<HTMLInputElement>("[data-permission-hint]")?.focus()); }
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
          const step = (first?.getBoundingClientRect().height || 44) + (Number.parseFloat(getComputedStyle(options).rowGap) || 0);
          options.scrollBy({ top: event.key.toLowerCase() === "j" ? step : -step, behavior: "auto" });
        }
      }
      if (!isTextInput && event.key.toLowerCase() === "z") {
        event.preventDefault();
        setLocal({ commandExpanded: !local.commandExpanded, descriptionExpanded: !local.descriptionExpanded });
      }
    }
    if (question && !questionBusy && !isTextInput) {
      if (/^[1-9]$/.test(event.key)) {
        const current = question.questions[local.questionIndex];
        const optionCount = (current?.options.length || 0) + 1;
        const index = Number(event.key) - 1;
        if (current && index < optionCount) {
          event.preventDefault();
          selectOption(current, index === current.options.length ? OTHER_VALUE : current.options[index].label);
        }
      } else if (event.key.toLowerCase() === "n") {
        event.preventDefault();
        advanceQuestion();
      } else if (event.key.toLowerCase() === "d" || event.key === "Enter") {
        event.preventDefault();
        advanceQuestion();
      }
    } else if (question && !questionBusy && isTextInput && event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      advanceQuestion();
    }
  }

  if (!sessionId) return null;
  if (!approval && !question) return null;
  return <div ref={panelRef} className="interruption-panel-root" onKeyDown={handleKeyDown}>
    {approval && <PermissionPanel
      request={approval}
      pendingCount={state.approvals.filter(item => item.status !== "cancelled").length}
      page={local.permissionPage}
      trustGroup={trustGroups[local.trustGroupIndex]}
      commandExpanded={local.commandExpanded}
      descriptionExpanded={local.descriptionExpanded}
      hint={local.hint}
      busy={permissionBusy}
      commandRef={commandRef}
      onFocus={index => setLocal({ focusIndex: index })}
      onAllow={() => void sendPermission({ sessionId: approval.sessionId, requestId: approval.requestId, choice: "allow" })}
      onDeny={() => void sendPermission({ sessionId: approval.sessionId, requestId: approval.requestId, choice: "deny" })}
      onTrust={enterTrustPage}
      onTrustChoice={respondTrustChoice}
      onBack={() => setLocal({ permissionPage: "main", trustGroupIndex: 0, trustKeys: [], focusIndex: 0 })}
      onHint={() => { setLocal({ permissionPage: "hint", hint: "" }); requestAnimationFrame(() => panelRef.current?.querySelector<HTMLInputElement>("[data-permission-hint]")?.focus()); }}
      onHintChange={hint => setLocal({ hint })}
      onCancelHint={() => setLocal({ permissionPage: "main" })}
      onSubmitHint={() => void sendPermission({ sessionId: approval.sessionId, requestId: approval.requestId, choice: "deny-hint", hint: local.hint.trim() })}
      onToggleCommand={() => setLocal({ commandExpanded: !local.commandExpanded })}
      onToggleDescription={() => setLocal({ descriptionExpanded: !local.descriptionExpanded })}
    />}
    {!approval && question && <QuestionPanel
      request={question}
      index={local.questionIndex}
      selectedByHeader={local.selectedByHeader}
      otherByHeader={local.otherByHeader}
      busy={questionBusy}
      onFocus={index => setLocal({ focusIndex: index })}
      onSelect={selectOption}
      onOtherChange={(header, value) => setLocal({ otherByHeader: { ...local.otherByHeader, [header]: value } })}
      onAdvance={advanceQuestion}
      onBack={() => setLocal({ questionIndex: Math.max(0, local.questionIndex - 1), focusIndex: 0 })}
      onCancel={() => void cancelQuestion()}
    />}
  </div>;
}

function PermissionPanel(props: {
  request: ApprovalRequest; pendingCount: number; page: PermissionPage; trustGroup?: ApprovalRequest["trustGroups"][number];
  commandExpanded: boolean; descriptionExpanded: boolean;
  hint: string; busy: boolean; commandRef: React.RefObject<HTMLDivElement | null>;
  onFocus(index: number): void; onAllow(): void; onDeny(): void; onTrust(): void;
  onTrustChoice(key: string): void; onHint(): void; onHintChange(value: string): void;
  onBack(): void; onCancelHint(): void; onSubmitHint(): void; onToggleCommand(): void; onToggleDescription(): void;
}) {
  const { request, page, trustGroup, commandExpanded, descriptionExpanded, busy } = props;
  const command = request.command || request.summary || "—";
  const disabled = busy;
  const commandCopy: ReactNode = request.tool.toLowerCase() === "bash" ? <><span className="perm-syn-prompt">$ </span>{command}</> : command;
  const description = permissionDescription(request);
  return <div className="perm-panel visible" id="permPanel" data-request-id={request.requestId} data-od-id="permission-panel" data-interruption-ui="shared" role="region" aria-label="Permission Request">
    <div className="perm-panel-context" id="permPanelContext">
      <div className="perm-panel-tool-row"><span className="perm-panel-tool" id="permTool">{request.tool || "—"}</span>{description && <FoldContent className="perm-panel-desc" text={description} kind="clamp" expanded={descriptionExpanded} onToggle={props.onToggleDescription} />}</div>
      <FoldContent className="perm-panel-cmd" text={command} prefix={request.tool.toLowerCase() === "bash" ? "$ " : ""} expanded={commandExpanded} onToggle={props.onToggleCommand} contentRef={props.commandRef}>{commandCopy}</FoldContent>
    </div>
    {page === "main" && <div className="perm-panel-page" id="permPanelMain">
      <ScrollViewport className="perm-panel-actions" id="permPanelActions">
        <ActionButton index={0} disabled={disabled} action="allow" keyLabel="Y" label="Allow Once" onFocus={props.onFocus} onClick={props.onAllow} />
        <ActionButton index={1} disabled={disabled || !request.trustLevels.length && !request.trustGroups.length && !request.trustKey} action="trust" keyLabel="T" label="Trust in project" onFocus={props.onFocus} onClick={props.onTrust} />
        <ActionButton index={2} disabled={disabled} action="deny" keyLabel="N" label="Deny Execute" onFocus={props.onFocus} onClick={props.onDeny} />
        <ActionButton index={3} disabled={disabled} action="hint" keyLabel="H" label="Deny and hints" onFocus={props.onFocus} onClick={props.onHint} />
      </ScrollViewport>
    </div>}
    {page === "trust" && <div className="perm-panel-page" id="permPanelTrust">
      {trustGroup && <>
        <ScrollViewport className="perm-panel-actions trust-options" id="permTrustActions" tabIndex={0} ariaLabel={trustGroup.target}>
          {trustGroup.levels.map((level, index) => <ActionButton key={level.key} index={index} disabled={disabled} action="trust-level" keyLabel={String(index + 1)} label={`Trust ${level.label || level.key}`} onFocus={props.onFocus} onClick={() => props.onTrustChoice(level.key)} />)}
          <RoundIconButton className="perm-panel-back" label="Back" disabled={disabled} action="back" onClick={props.onBack}><ArrowLeft size={20} strokeWidth={1.7} aria-hidden="true" /></RoundIconButton>
        </ScrollViewport>
      </>}
    </div>}
    {page === "hint" && <div className="perm-panel-page perm-panel-hint-page" id="permPanelHint">
      <input className="perm-panel-hint-input" data-permission-hint data-interruption-focus="3" id="permHintInput" type="text" value={`${HINT_PREFIX}${props.hint}`} autoComplete="off" spellCheck={false} aria-label="Deny reason" disabled={disabled}
        onFocus={() => props.onFocus(3)}
        onChange={event => {
          const input = event.currentTarget;
          const raw = input.value;
          const prefix = /^Deny,\s*/i.exec(raw)?.[0];
          const next = prefix ? raw.slice(prefix.length) : raw;
          const cursor = input.selectionStart || 0;
          props.onHintChange(next);
          if (!prefix || cursor < HINT_PREFIX.length) requestAnimationFrame(() => input.setSelectionRange(HINT_PREFIX.length, HINT_PREFIX.length));
        }} />
      <div className="perm-panel-hint-actions">
        <button className="perm-panel-hint-cancel" data-permission-action="cancel-hint" type="button" disabled={disabled} onClick={props.onCancelHint}>Cancel</button>
        <button className="perm-panel-hint-submit" data-permission-action="submit-hint" type="button" disabled={disabled} onClick={props.onSubmitHint}>Deny</button>
      </div>
    </div>}
    {props.pendingCount > 1 && <div className="approval-dots" aria-label={`${props.pendingCount} pending approvals`}>{Array.from({ length: props.pendingCount }, (_, index) => <span className="approval-dot" key={index} />)}</div>}
  </div>;
}

function permissionDescription(request: ApprovalRequest): string {
  if (["read", "write", "edit", "bash"].includes(request.tool.toLowerCase())) return "";
  try {
    const value = JSON.parse(request.summary) as Record<string, unknown>;
    for (const key of ["description", "system_prompt", "prompt", "task"]) {
      if (typeof value[key] === "string" && value[key].trim()) return value[key].trim();
    }
  } catch (error) {
    console.debug("Permission summary is not structured tool arguments:", error);
  }
  return "";
}

function FoldContent(props: {
  className: string; text: string; prefix?: string; kind?: "lines" | "clamp"; expanded: boolean;
  onToggle(): void; contentRef?: React.RefObject<HTMLDivElement | null>; children?: ReactNode;
}) {
  const ownRef = useRef<HTMLDivElement>(null);
  const [overflow, setOverflow] = useState(false);
  const kind = props.kind || "lines";
  useLayoutEffect(() => {
    const element = ownRef.current;
    if (!element) return;
    const measure = () => {
      const full = element.querySelector<HTMLElement>(".fold-full");
      if (!full) return;
      const view = element.ownerDocument.defaultView;
      const lineHeight = Number.parseFloat(view?.getComputedStyle(full).lineHeight || "") || 18;
      setOverflow(kind === "clamp" ? full.scrollHeight > lineHeight * 3 + 1 : full.scrollHeight > lineHeight * 2 + 1);
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => observer.disconnect();
  }, [kind, props.text]);
  const title = overflow ? `Click or press Z to ${props.expanded ? "collapse" : "expand"}` : undefined;
  return <div ref={element => { ownRef.current = element; if (props.contentRef) props.contentRef.current = element; }}
    className={`${props.className} fold${props.expanded ? " expanded" : ""}`} data-kind={kind} data-overflow={String(overflow)}
    data-expanded={props.expanded ? "true" : undefined} title={title} onClick={overflow ? props.onToggle : undefined}>
    <span className="fold-full">{props.prefix}{props.children || props.text}</span>
    {kind === "lines" && <span className="fold-collapsed"><span className="fold-head">{props.prefix}{props.text}</span><span className="fold-tail"><span>{props.text}</span></span></span>}
  </div>;
}

function ScrollViewport(props: { className: string; id: string; tabIndex?: number; ariaLabel?: string; children: ReactNode }) {
  const optionsRef = useRef<HTMLDivElement>(null);
  const [metrics, setMetrics] = useState({ visible: false, height: 0, offset: 0 });
  const measure = () => {
    const options = optionsRef.current;
    if (!options) return;
    const visible = options.scrollHeight > options.clientHeight + 1;
    const height = visible ? Math.max(28, options.clientHeight * options.clientHeight / options.scrollHeight) : 0;
    const travel = options.clientHeight - height;
    const offset = visible && options.scrollHeight > options.clientHeight ? options.scrollTop / (options.scrollHeight - options.clientHeight) * travel : 0;
    setMetrics(current => current.visible === visible && current.height === height && current.offset === offset
      ? current : { visible, height, offset });
  };
  useLayoutEffect(() => {
    measure();
    const options = optionsRef.current;
    if (!options) return;
    const observer = new ResizeObserver(measure);
    observer.observe(options);
    return () => observer.disconnect();
  }, []);
  return <div className="perm-option-viewport">
    <div ref={optionsRef} className={`${props.className} scroll-options`} id={props.id} tabIndex={props.tabIndex} aria-label={props.ariaLabel} onScroll={measure}>{props.children}</div>
    <div className="trust-scrollbar" aria-hidden="true" hidden={!metrics.visible} onPointerDown={event => {
      const options = optionsRef.current;
      if (!options) return;
      const rect = event.currentTarget.getBoundingClientRect();
      const position = Math.max(0, Math.min(1, (event.clientY - rect.top) / rect.height));
      options.scrollTop = Math.max(0, position * options.scrollHeight - options.clientHeight / 2);
    }}><span className="trust-scrollbar-thumb" style={{ height: metrics.height, transform: `translateY(${metrics.offset}px)` }} /></div>
  </div>;
}

function ActionButton(props: {
  index: number; disabled: boolean; action: string; keyLabel: string; label: string;
  onFocus(index: number): void; onClick(): void;
}) {
  return <button type="button" className="perm-panel-opt" data-permission-action={props.action}
    data-interruption-focus={props.index} disabled={props.disabled} onFocus={() => props.onFocus(props.index)} onClick={props.onClick}>
    <span className="perm-panel-opt-key">{props.keyLabel}</span><span className="perm-panel-opt-label">{props.label}</span>
  </button>;
}

function RoundIconButton(props: {
  className: string; label: string; disabled: boolean; action?: string; onClick(): void; children: ReactNode;
}) {
  return <button className={props.className} type="button" data-permission-action={props.action}
    aria-label={props.label} title={props.label} disabled={props.disabled} onClick={props.onClick}>{props.children}</button>;
}

function QuestionPanel(props: {
  request: UserQuestionRequest; index: number; selectedByHeader: Record<string, string[]>; otherByHeader: Record<string, string>;
  busy: boolean; onFocus(index: number): void;
  onSelect(question: UserQuestion, value: string): void; onOtherChange(header: string, value: string): void;
  onAdvance(): void; onBack(): void; onCancel(): void;
}) {
  const question = props.request.questions[props.index];
  if (!question) return null;
  const selected = props.selectedByHeader[question.header] || [];
  const last = props.index + 1 === props.request.questions.length;
  const multiple = props.request.questions.length > 1;
  const otherSelected = selected.includes(OTHER_VALUE);
  const canAdvance = selected.length > 0 && (!otherSelected || Boolean((props.otherByHeader[question.header] || "").trim()));
  return <div className="question-panel visible" id="questionPanel" data-request-id={props.request.requestId} data-od-id="question-panel" data-interruption-ui="shared" role="region" aria-labelledby="questionPanelTitle">
    <div className="question-panel-head">
      <div className="question-panel-title" id="questionPanelTitle" data-od-id="question-title" role="heading" aria-level={2}>{question.question}</div>
      {multiple && <RoundIconButton className="question-panel-cancel" label="Cancel" disabled={props.busy} onClick={props.onCancel}><X size={20} strokeWidth={1.7} aria-hidden="true" /></RoundIconButton>}
    </div>
    <ScrollViewport className="question-panel-options" id="questionPanelOptions">
      {[...question.options, { label: "Other", description: "Write a custom answer", other: true }].map((option, index) => {
        const value = "other" in option ? OTHER_VALUE : option.label;
        const checked = selected.includes(value);
        return <label key={value} className="question-panel-option">
          <span className="question-panel-option-key">{index + 1}</span>
          <input type={question.multiSelect ? "checkbox" : "radio"} name={`question-${props.request.requestId}-${props.index}`} value={value}
            data-option-number={index + 1} data-other={value === OTHER_VALUE} data-interruption-focus={index} checked={checked} disabled={props.busy}
            onFocus={() => props.onFocus(index)} onChange={() => props.onSelect(question, value)} />
          <span className="question-panel-option-copy"><span className="question-panel-option-label">{option.label}</span>{value === OTHER_VALUE && checked
            ? <input className="question-panel-other-input" data-question-other type="text" autoComplete="off" spellCheck={false} placeholder="Type your own answer…" value={props.otherByHeader[question.header] || ""} disabled={props.busy} onChange={event => props.onOtherChange(question.header, event.currentTarget.value)} />
            : <span className="question-panel-option-description">{option.description}</span>}</span>
        </label>;
      })}
    </ScrollViewport>
    <div className="question-panel-answer-row">
      {multiple && props.index > 0
        ? <RoundIconButton className="question-panel-back" label="Back" disabled={props.busy} onClick={props.onBack}><ArrowLeft size={20} strokeWidth={1.7} aria-hidden="true" /></RoundIconButton>
        : !multiple
          ? <RoundIconButton className="question-panel-cancel-inline" label="Cancel" disabled={props.busy} onClick={props.onCancel}><X size={20} strokeWidth={1.7} aria-hidden="true" /></RoundIconButton>
          : <span className="question-panel-slot" />}
      {multiple && <div className="question-panel-progress" aria-label={`Question ${props.index + 1} of ${props.request.questions.length}`}>{props.request.questions.map((item, index) => <span key={item.header} className={index < props.index ? "answered" : ""} />)}</div>}
      {!multiple && <div className="question-panel-spacer" />}
      <div className="question-panel-actions">
        <button className="question-panel-submit" id="questionPanelSubmit" data-od-id="question-submit" data-permission-action="submit-question"
          data-interruption-focus={question.options.length + 1} type="button" aria-label={last ? "Done (D)" : "Next question (N)"} title={last ? "Done" : "Next question"} disabled={props.busy || !canAdvance}
          onFocus={() => props.onFocus(question.options.length + 1)} onClick={props.onAdvance}>
          {last ? <Check className="lucide" size={20} strokeWidth={1.7} aria-hidden="true" /> : <ArrowRight className="lucide" size={20} strokeWidth={1.7} aria-hidden="true" />}
        </button>
      </div>
    </div>
  </div>;
}

