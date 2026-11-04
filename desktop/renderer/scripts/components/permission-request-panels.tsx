import * as React from "react";
import { useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { ArrowLeft, ArrowRight, Check, X } from "lucide-react";
import type {
  ApprovalRequest,
  UserQuestion,
  UserQuestionRequest,
} from "../../../main/session-contract.js";
import { HighlightedCode } from "./highlighted-code.js";

export const HINT_PREFIX = "Deny, ";
export const OTHER_VALUE = "__rozsa_other__";
export type PermissionPage = "main" | "trust" | "hint";

export function PermissionPanel(props: {
  request: ApprovalRequest;
  pendingCount: number;
  page: PermissionPage;
  trustGroup?: ApprovalRequest["trustGroups"][number];
  commandExpanded: boolean;
  descriptionExpanded: boolean;
  hint: string;
  busy: boolean;
  commandRef: React.RefObject<HTMLDivElement | null>;
  onFocus(index: number): void;
  onAllow(): void;
  onDeny(): void;
  onTrust(): void;
  onTrustChoice(key: string): void;
  onHint(): void;
  onHintChange(value: string): void;
  onBack(): void;
  onCancelHint(): void;
  onSubmitHint(): void;
  onToggleCommand(): void;
  onToggleDescription(): void;
}) {
  const { request, page, trustGroup, commandExpanded, descriptionExpanded, busy } = props;
  const command = request.command || request.summary || "—";
  const disabled = busy;
  const isBash = request.tool.toLowerCase() === "bash";
  const commandCopy: ReactNode = isBash ? (
    <HighlightedCode code={command} language="bash" />
  ) : (
    command
  );
  const commandPrefix: ReactNode = isBash ? <span className="perm-syn-prompt">$ </span> : null;
  const description = permissionDescription(request);
  return (
    <div
      className="perm-panel visible"
      id="permPanel"
      data-request-id={request.requestId}
      data-od-id="permission-panel"
      data-interruption-ui="shared"
      role="region"
      aria-label="Permission Request"
    >
      <div className="perm-panel-context" id="permPanelContext">
        <div className="perm-panel-tool-row">
          <span className="perm-panel-tool" id="permTool">
            {request.tool || "—"}
          </span>
          {description && (
            <FoldContent
              className="perm-panel-desc"
              text={description}
              kind="clamp"
              expanded={descriptionExpanded}
              onToggle={props.onToggleDescription}
            />
          )}
        </div>
        <FoldContent
          className="perm-panel-cmd"
          text={command}
          prefix={commandPrefix}
          expanded={commandExpanded}
          onToggle={props.onToggleCommand}
          contentRef={props.commandRef}
        >
          {commandCopy}
        </FoldContent>
      </div>
      {page === "main" && (
        <div className="perm-panel-page" id="permPanelMain">
          <ScrollViewport className="perm-panel-actions" id="permPanelActions">
            <ActionButton
              index={0}
              disabled={disabled}
              action="allow"
              keyLabel="Y"
              label="Allow Once"
              onFocus={props.onFocus}
              onClick={props.onAllow}
            />
            <ActionButton
              index={1}
              disabled={
                disabled ||
                (!request.trustLevels.length && !request.trustGroups.length && !request.trustKey)
              }
              action="trust"
              keyLabel="T"
              label="Trust in project"
              onFocus={props.onFocus}
              onClick={props.onTrust}
            />
            <ActionButton
              index={2}
              disabled={disabled}
              action="deny"
              keyLabel="N"
              label="Deny Execute"
              onFocus={props.onFocus}
              onClick={props.onDeny}
            />
            <ActionButton
              index={3}
              disabled={disabled}
              action="hint"
              keyLabel="H"
              label="Deny and hints"
              onFocus={props.onFocus}
              onClick={props.onHint}
            />
          </ScrollViewport>
        </div>
      )}
      {page === "trust" && (
        <div className="perm-panel-page" id="permPanelTrust">
          {trustGroup && (
            <>
              <ScrollViewport
                className="perm-panel-actions trust-options"
                id="permTrustActions"
                tabIndex={0}
                ariaLabel={trustGroup.target}
              >
                {trustGroup.levels.map((level, index) => (
                  <ActionButton
                    key={level.key}
                    index={index}
                    disabled={disabled}
                    action="trust-level"
                    keyLabel={String(index + 1)}
                    label={`Trust ${level.label || level.key}`}
                    onFocus={props.onFocus}
                    onClick={() => props.onTrustChoice(level.key)}
                  />
                ))}
                <RoundIconButton
                  className="perm-panel-back"
                  label="Back"
                  disabled={disabled}
                  action="back"
                  onClick={props.onBack}
                >
                  <ArrowLeft size={20} strokeWidth={1.7} aria-hidden="true" />
                </RoundIconButton>
              </ScrollViewport>
            </>
          )}
        </div>
      )}
      {page === "hint" && (
        <div className="perm-panel-page perm-panel-hint-page" id="permPanelHint">
          <input
            className="perm-panel-hint-input"
            data-permission-hint
            data-interruption-focus="3"
            id="permHintInput"
            type="text"
            value={`${HINT_PREFIX}${props.hint}`}
            autoComplete="off"
            spellCheck={false}
            aria-label="Deny reason"
            disabled={disabled}
            onFocus={() => props.onFocus(3)}
            onChange={(event) => {
              const input = event.currentTarget;
              const raw = input.value;
              const prefix = /^Deny,\s*/i.exec(raw)?.[0];
              const next = prefix ? raw.slice(prefix.length) : raw;
              const cursor = input.selectionStart || 0;
              props.onHintChange(next);
              if (!prefix || cursor < HINT_PREFIX.length)
                requestAnimationFrame(() =>
                  input.setSelectionRange(HINT_PREFIX.length, HINT_PREFIX.length),
                );
            }}
          />
          <div className="perm-panel-hint-actions">
            <button
              className="perm-panel-hint-cancel"
              data-permission-action="cancel-hint"
              type="button"
              disabled={disabled}
              onClick={props.onCancelHint}
            >
              Cancel
            </button>
            <button
              className="perm-panel-hint-submit"
              data-permission-action="submit-hint"
              type="button"
              disabled={disabled}
              onClick={props.onSubmitHint}
            >
              Deny
            </button>
          </div>
        </div>
      )}
      {props.pendingCount > 1 && (
        <div className="approval-dots" aria-label={`${props.pendingCount} pending approvals`}>
          {Array.from({ length: props.pendingCount }, (_, index) => (
            <span className="approval-dot" key={index} />
          ))}
        </div>
      )}
    </div>
  );
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
  className: string;
  text: string;
  prefix?: ReactNode;
  kind?: "lines" | "clamp";
  expanded: boolean;
  onToggle(): void;
  contentRef?: React.RefObject<HTMLDivElement | null>;
  children?: ReactNode;
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
      setOverflow(
        kind === "clamp"
          ? full.scrollHeight > lineHeight * 3 + 1
          : full.scrollHeight > lineHeight * 2 + 1,
      );
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => observer.disconnect();
  }, [kind, props.text]);
  const title = overflow
    ? `Click or press Z to ${props.expanded ? "collapse" : "expand"}`
    : undefined;
  return (
    <div
      ref={(element) => {
        ownRef.current = element;
        if (props.contentRef) props.contentRef.current = element;
      }}
      className={`${props.className} fold${props.expanded ? " expanded" : ""}`}
      data-kind={kind}
      data-overflow={String(overflow)}
      data-expanded={props.expanded ? "true" : undefined}
      title={title}
      onClick={overflow ? props.onToggle : undefined}
    >
      <span className="fold-full">
        {props.prefix}
        {props.children || props.text}
      </span>
      {kind === "lines" && (
        <span className="fold-collapsed">
          <span className="fold-head">
            {props.prefix}
            {props.text}
          </span>
          <span className="fold-tail">
            <span>{props.text}</span>
          </span>
        </span>
      )}
    </div>
  );
}

function ScrollViewport(props: {
  className: string;
  id: string;
  tabIndex?: number;
  ariaLabel?: string;
  children: ReactNode;
}) {
  const optionsRef = useRef<HTMLDivElement>(null);
  const [metrics, setMetrics] = useState({ visible: false, height: 0, offset: 0 });
  const measure = () => {
    const options = optionsRef.current;
    if (!options) return;
    const visible = options.scrollHeight > options.clientHeight + 1;
    const height = visible
      ? Math.max(28, (options.clientHeight * options.clientHeight) / options.scrollHeight)
      : 0;
    const travel = options.clientHeight - height;
    const offset =
      visible && options.scrollHeight > options.clientHeight
        ? (options.scrollTop / (options.scrollHeight - options.clientHeight)) * travel
        : 0;
    setMetrics((current) =>
      current.visible === visible && current.height === height && current.offset === offset
        ? current
        : { visible, height, offset },
    );
  };
  useLayoutEffect(() => {
    measure();
    const options = optionsRef.current;
    if (!options) return;
    const observer = new ResizeObserver(measure);
    observer.observe(options);
    return () => observer.disconnect();
  }, []);
  return (
    <div className="perm-option-viewport">
      <div
        ref={optionsRef}
        className={`${props.className} scroll-options`}
        id={props.id}
        tabIndex={props.tabIndex}
        aria-label={props.ariaLabel}
        onScroll={measure}
      >
        {props.children}
      </div>
      <div
        className="trust-scrollbar"
        aria-hidden="true"
        hidden={!metrics.visible}
        onPointerDown={(event) => {
          const options = optionsRef.current;
          if (!options) return;
          const rect = event.currentTarget.getBoundingClientRect();
          const position = Math.max(0, Math.min(1, (event.clientY - rect.top) / rect.height));
          options.scrollTop = Math.max(
            0,
            position * options.scrollHeight - options.clientHeight / 2,
          );
        }}
      >
        <span
          className="trust-scrollbar-thumb"
          style={{ height: metrics.height, transform: `translateY(${metrics.offset}px)` }}
        />
      </div>
    </div>
  );
}

function ActionButton(props: {
  index: number;
  disabled: boolean;
  action: string;
  keyLabel: string;
  label: string;
  onFocus(index: number): void;
  onClick(): void;
}) {
  return (
    <button
      type="button"
      className="perm-panel-opt"
      data-permission-action={props.action}
      data-interruption-focus={props.index}
      disabled={props.disabled}
      onFocus={() => props.onFocus(props.index)}
      onClick={props.onClick}
    >
      <span className="perm-panel-opt-key">{props.keyLabel}</span>
      <span className="perm-panel-opt-label">{props.label}</span>
    </button>
  );
}

function RoundIconButton(props: {
  className: string;
  label: string;
  disabled: boolean;
  action?: string;
  onClick(): void;
  children: ReactNode;
}) {
  return (
    <button
      className={props.className}
      type="button"
      data-permission-action={props.action}
      aria-label={props.label}
      title={props.label}
      disabled={props.disabled}
      onClick={props.onClick}
    >
      {props.children}
    </button>
  );
}

export function QuestionPanel(props: {
  request: UserQuestionRequest;
  index: number;
  selectedByHeader: Record<string, string[]>;
  otherByHeader: Record<string, string>;
  busy: boolean;
  onFocus(index: number): void;
  onSelect(question: UserQuestion, value: string): void;
  onOtherChange(header: string, value: string): void;
  onAdvance(): void;
  onBack(): void;
  onCancel(): void;
}) {
  const question = props.request.questions[props.index];
  if (!question) return null;
  const selected = props.selectedByHeader[question.header] || [];
  const last = props.index + 1 === props.request.questions.length;
  const multiple = props.request.questions.length > 1;
  const otherSelected = selected.includes(OTHER_VALUE);
  const canAdvance =
    selected.length > 0 &&
    (!otherSelected || Boolean((props.otherByHeader[question.header] || "").trim()));
  return (
    <div
      className="question-panel visible"
      id="questionPanel"
      data-request-id={props.request.requestId}
      data-od-id="question-panel"
      data-interruption-ui="shared"
      role="region"
      aria-labelledby="questionPanelTitle"
    >
      <div className="question-panel-head">
        <div
          className="question-panel-title"
          id="questionPanelTitle"
          data-od-id="question-title"
          role="heading"
          aria-level={2}
        >
          {question.question}
        </div>
        {multiple && (
          <RoundIconButton
            className="question-panel-cancel"
            label="Cancel"
            disabled={props.busy}
            onClick={props.onCancel}
          >
            <X size={20} strokeWidth={1.7} aria-hidden="true" />
          </RoundIconButton>
        )}
      </div>
      <ScrollViewport className="question-panel-options" id="questionPanelOptions">
        {[
          ...question.options,
          { label: "Other", description: "Write a custom answer", other: true },
        ].map((option, index) => {
          const value = "other" in option ? OTHER_VALUE : option.label;
          const checked = selected.includes(value);
          return (
            <label key={value} className="question-panel-option">
              <span className="question-panel-option-key">{index + 1}</span>
              <input
                type={question.multiSelect ? "checkbox" : "radio"}
                name={`question-${props.request.requestId}-${props.index}`}
                value={value}
                data-option-number={index + 1}
                data-other={value === OTHER_VALUE}
                data-interruption-focus={index}
                checked={checked}
                disabled={props.busy}
                onFocus={() => props.onFocus(index)}
                onChange={() => props.onSelect(question, value)}
              />
              <span className="question-panel-option-copy">
                <span className="question-panel-option-label">{option.label}</span>
                {value === OTHER_VALUE && checked ? (
                  <input
                    className="question-panel-other-input"
                    data-question-other
                    type="text"
                    autoComplete="off"
                    spellCheck={false}
                    placeholder="Type your own answer…"
                    value={props.otherByHeader[question.header] || ""}
                    disabled={props.busy}
                    onChange={(event) =>
                      props.onOtherChange(question.header, event.currentTarget.value)
                    }
                  />
                ) : (
                  <span className="question-panel-option-description">{option.description}</span>
                )}
              </span>
            </label>
          );
        })}
      </ScrollViewport>
      <div className="question-panel-answer-row">
        {multiple && props.index > 0 ? (
          <RoundIconButton
            className="question-panel-back"
            label="Back"
            disabled={props.busy}
            onClick={props.onBack}
          >
            <ArrowLeft size={20} strokeWidth={1.7} aria-hidden="true" />
          </RoundIconButton>
        ) : !multiple ? (
          <RoundIconButton
            className="question-panel-cancel-inline"
            label="Cancel"
            disabled={props.busy}
            onClick={props.onCancel}
          >
            <X size={20} strokeWidth={1.7} aria-hidden="true" />
          </RoundIconButton>
        ) : (
          <span className="question-panel-slot" />
        )}
        {multiple && (
          <div
            className="question-panel-progress"
            aria-label={`Question ${props.index + 1} of ${props.request.questions.length}`}
          >
            {props.request.questions.map((item, index) => (
              <span key={item.header} className={index < props.index ? "answered" : ""} />
            ))}
          </div>
        )}
        {!multiple && <div className="question-panel-spacer" />}
        <div className="question-panel-actions">
          <button
            className="question-panel-submit"
            id="questionPanelSubmit"
            data-od-id="question-submit"
            data-permission-action="submit-question"
            data-interruption-focus={question.options.length + 1}
            type="button"
            aria-label={last ? "Done (D)" : "Next question (N)"}
            title={last ? "Done" : "Next question"}
            disabled={props.busy || !canAdvance}
            onFocus={() => props.onFocus(question.options.length + 1)}
            onClick={props.onAdvance}
          >
            {last ? (
              <Check className="lucide" size={20} strokeWidth={1.7} aria-hidden="true" />
            ) : (
              <ArrowRight className="lucide" size={20} strokeWidth={1.7} aria-hidden="true" />
            )}
          </button>
        </div>
      </div>
    </div>
  );
}
