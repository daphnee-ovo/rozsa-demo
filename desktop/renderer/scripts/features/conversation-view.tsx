import React, { Fragment, memo, type ReactNode } from "react";
import { createRoot, type Root } from "react-dom/client";

export type ConversationRole = "user" | "assistant" | "system" | "toolResult";
export interface ConversationBlock { type: string; text?: string; id?: string; name?: string; [key: string]: unknown }
export interface ConversationMessage {
  role: ConversationRole;
  content?: ConversationBlock[];
  errorMessage?: string | null;
  error?: string | null;
  toolName?: string;
  toolCallId?: string;
  isError?: boolean;
  [key: string]: unknown;
}
export interface ConversationEntry {
  id: string;
  kind: "standard" | "custom";
  message: ConversationMessage | Record<string, unknown>;
  messageIndex: number;
  continuation?: boolean;
  showIdentity?: boolean;
  activeStream?: boolean;
  streamTarget?: "text" | "thinking";
  thinkingExpanded?: boolean;
  thinkingDuration?: string;
  turnActivityMarkup?: string;
  bodyMarkup?: string;
}
export interface ConversationViewProps {
  entries: ConversationEntry[];
  markdown(text: string): string;
  empty: boolean;
}

interface MessageRowProps { entry: ConversationEntry; markdown: (text: string) => string }

function textBlocks(content: ConversationBlock[], type: "text" | "thinking"): string {
  return content.filter(block => block.type === type && typeof block.text === "string").map(block => block.text!).join("\n");
}

function ImageBlock({ block }: { block: ConversationBlock }) {
  const mimeType = typeof block.mimeType === "string" ? block.mimeType.toLowerCase() : "";
  const data = typeof block.data === "string" ? block.data : "";
  const allowedType = ["image/png", "image/jpeg", "image/gif", "image/webp", "image/avif"].includes(mimeType);
  if (!allowedType || !data || !/^[A-Za-z0-9+/]*={0,2}$/.test(data)) {
    return <span className="msg-image-blocked" role="img" aria-label="Image unavailable">Image unavailable</span>;
  }
  return <img className="md-image" alt="Image attachment" src={`data:${mimeType};base64,${data}`} />;
}

function Markdown({ value, render, className = "msg-content markdown-body", cursor = false }: {
  value: string; render: (text: string) => string; className?: string; cursor?: boolean;
}) {
  if (!value) return null;
  return <div className={className} data-stream-cursor-target={cursor ? "text" : undefined}
    dangerouslySetInnerHTML={{ __html: render(value) + (cursor ? '<span class="stream-cursor">▌</span>' : '') }} />;
}

function Frame({ role, index, continuation, children }: {
  role: ConversationRole; index: number; continuation: boolean; children: ReactNode;
}) {
  const hiddenIdentity = continuation && role === "assistant";
  return <div className={`msg msg-${role}${continuation ? " msg-assistant-continuation" : ""}`} data-od-id={`message-${index}`}>
    {hiddenIdentity ? null : <div className="msg-avatar">{role === "user" ? "U" : role === "system" ? "S" : "R"}</div>}
    <div className="msg-body">{hiddenIdentity ? null : <div className="msg-role">{role === "user" ? "You" : role === "system" ? "System" : "Rozsa"}</div>}{children}</div>
  </div>;
}

function ThinkingBlock({ text, entry, render }: { text: string; entry: ConversationEntry; render: (text: string) => string }) {
  const expanded = entry.thinkingExpanded !== false;
  const active = entry.activeStream === true && entry.streamTarget === "thinking";
  return <div className={`thinking-block${expanded ? " expanded" : ""}${active ? " active" : ""}`}>
    <div className="thinking-header" role="button" tabIndex={0} aria-expanded={expanded}
      onClick={event => window.toggleThinking?.(event.currentTarget)}
      onKeyDown={event => {
        if (event.key !== "Enter" && event.key !== " ") return;
        event.preventDefault();
        window.toggleThinking?.(event.currentTarget);
      }}>
      <svg className="lucide thinking-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
        <path d="M9 18h6"/><path d="M10 22h4"/><path d="M15.09 14c.18-.7.66-1.22 1.18-1.75A6 6 0 1 0 7.73 12.25c.52.52 1 1.04 1.18 1.75Z"/>
      </svg>
      <span className="thinking-label">{active ? "THINKING" : "THINKED"}</span>
      {entry.thinkingDuration ? <span className="thinking-duration">{entry.thinkingDuration}</span> : null}
      <span className="thinking-chevron"><svg className="lucide" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="m9 18 6-6-6-6"/></svg></span>
    </div>
    <div className="thinking-content" data-stream-cursor-target={active ? "thinking" : undefined}>
      <Markdown value={text} render={render} className="thinking-markdown markdown-body" />
    </div>
  </div>;
}

const MessageRow = memo(function MessageRow({ entry, markdown }: MessageRowProps) {
  const message = entry.message as ConversationMessage;
  const content = Array.isArray(message.content) ? message.content : [];
  if (entry.kind === "custom") {
    const custom = entry.message as Record<string, unknown>;
    const body = typeof custom.payload === "string" ? custom.payload : JSON.stringify(custom.payload, null, 2);
    const label = String(custom.message_type || custom.messageType || "system");
    return <div className="msg msg-assistant" data-od-id={`message-${entry.messageIndex}`}>
      <div className="msg-avatar">R</div><div className="msg-body"><div className="msg-role">{label}</div>
        <div className="msg-content"><pre style={{ fontFamily: "var(--font-mono)", fontSize: 12, whiteSpace: "pre-wrap" }}>{body}</pre></div>
      </div>
    </div>;
  }

  if (message.role === "user" || message.role === "system") {
    const text = textBlocks(content, "text");
    return <Frame role={message.role} index={entry.messageIndex} continuation={false}>
      <Markdown value={text} render={markdown} />
      {content.filter(block => block.type === "image").map((block, index) => <ImageBlock key={`${entry.id}:image:${index}`} block={block} />)}
      {entry.bodyMarkup ? <div dangerouslySetInnerHTML={{ __html: entry.bodyMarkup }} /> : null}
    </Frame>;
  }

  if (message.role === "toolResult") {
    return <Frame role="assistant" index={entry.messageIndex} continuation={true}>
      {entry.bodyMarkup ? <div dangerouslySetInnerHTML={{ __html: entry.bodyMarkup }} /> :
        <div className={`msg-content${message.isError ? " msg-error" : ""}`}><pre>{textBlocks(content, "text")}</pre></div>}
    </Frame>;
  }

  const error = typeof message.errorMessage === "string" ? message.errorMessage : message.error;
  const thinking = textBlocks(content, "thinking");
  const text = textBlocks(content, "text");
  return <Frame role="assistant" index={entry.messageIndex} continuation={entry.continuation === true}>
    {error ? <div className="msg-content msg-error"><pre>{error}</pre></div> : null}
    {thinking ? <ThinkingBlock text={thinking} entry={entry} render={markdown} /> : null}
    {entry.bodyMarkup ? <div dangerouslySetInnerHTML={{ __html: entry.bodyMarkup }} /> : null}
    <Markdown value={text} render={markdown} cursor={entry.activeStream === true && entry.streamTarget === "text"} />
    {content.filter(block => block.type === "image").map((block, index) => <ImageBlock key={`${entry.id}:image:${index}`} block={block} />)}
    {entry.turnActivityMarkup ? <div dangerouslySetInnerHTML={{ __html: entry.turnActivityMarkup }} /> : null}
    {entry.activeStream && entry.streamTarget && !text && !thinking ? <span className="stream-cursor">▌</span> : null}
  </Frame>;
}, (previous, next) => previous.entry === next.entry && previous.markdown === next.markdown);

function EmptyConversation() {
  return <div className="chat-empty" data-od-id="chat-empty">
    <div className="chat-empty-icon">R</div>
    <div className="chat-empty-title">Start a new conversation</div>
    <div className="chat-empty-hint">Describe your coding task to Rózsa
      <div className="chat-empty-kbd"><kbd>Enter</kbd> Send <kbd>⇧ Enter</kbd> New line</div>
    </div>
  </div>;
}

export function ConversationView({ entries, markdown, empty }: ConversationViewProps) {
  if (empty) return <EmptyConversation />;
  return <Fragment>{entries.map(entry => <MessageRow key={entry.id} entry={entry} markdown={markdown} />)}</Fragment>;
}

export function createConversationRenderer(container: Element) {
  if (!(container instanceof Element)) throw new TypeError("Conversation mount must be an Element");
  const root: Root = createRoot(container);
  let disposed = false;
  return Object.freeze({
    render(props: ConversationViewProps) {
      if (disposed) throw new Error("Conversation renderer is disposed");
      root.render(<ConversationView {...props} />);
    },
    dispose() {
      if (disposed) return;
      disposed = true;
      root.unmount();
    },
  });
}

declare global {
  interface Window { toggleThinking?: (header: HTMLElement) => void }
}
