import React, { useLayoutEffect, useMemo, useRef, useState, type ComponentProps, type ReactElement, type ReactNode } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { Streamdown, defaultComponents } from "streamdown";
import type { Mounted } from "pele";
import { completeMermaidFences } from "./markdown-renderer.tsx";

function recordMarkdownStage(stage: "renderCommit" | "peleMount" | "peleAdapt", duration: number): void {
  const perf = window.__ROZSA_PERF__;
  if (!perf) return;
  perf.markdownStages ??= { renderCommit: [], peleMount: [], peleAdapt: [] };
  perf.markdownStages[stage].push(duration);
}

function normalizedCode(source: string): string { return source.replace(/\r\n/g, "\n").replace(/\n+$/, ""); }
function codeText(node: ReactNode): string {
  if (Array.isArray(node)) return node.map(codeText).join("");
  if (typeof node === "string" || typeof node === "number") return String(node);
  if (React.isValidElement<{ children?: ReactNode }>(node)) return codeText(node.props.children);
  return "";
}

function CopyButton({ source, staticMarkup = false }: { source: string; staticMarkup?: boolean }) {
  return <button className="md-copy" type="button" aria-label="Copy code" title="Copy code"
    data-static-copy={staticMarkup ? "true" : undefined}
    onClick={event => { void navigator.clipboard.writeText(source).then(() => {
      const button = event.currentTarget; button.classList.add("copied");
      window.setTimeout(() => button.classList.remove("copied"), 1200);
    }).catch(error => window.showError?.(`Could not copy code: ${String(error)}`)); }}>
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <rect x="5" y="4" width="8" height="9" rx="1.5"/><path d="M3 10.5V3.5A1.5 1.5 0 0 1 4.5 2h6"/>
    </svg>
  </button>;
}

function MermaidDiagram({ source }: { source: string }) {
  const target = useRef<HTMLDivElement>(null);
  const mounted = useRef<Mounted | undefined>(undefined);
  const [error, setError] = useState<string>();
  useLayoutEffect(() => {
    const element = target.current;
    if (!element) return;
    let active = true, mountCounted = false;
    let resizeStartedAt: number | undefined;
    let lastWidth = element.getBoundingClientRect().width;
    const recordAdaptation = () => {
      if (resizeStartedAt === undefined) return;
      recordMarkdownStage("peleAdapt", Math.max(0, performance.now() - resizeStartedAt));
      resizeStartedAt = undefined;
    };
    const resizeObserver = typeof ResizeObserver === "undefined" ? undefined : new ResizeObserver(entries => {
      const width = entries.at(-1)?.contentRect.width ?? element.getBoundingClientRect().width;
      if (Math.abs(width - lastWidth) < 1) return;
      lastWidth = width;
      resizeStartedAt = performance.now();
    });
    resizeObserver?.observe(element);
    const mutationObserver = typeof MutationObserver === "undefined" ? undefined : new MutationObserver(() => recordAdaptation());
    mutationObserver?.observe(element, { attributes: true, childList: true, characterData: true, subtree: true });
    const started = performance.now();
    void import("pele").then(({ mount }) => {
      if (!active) return;
      try {
        mounted.current = mount(element, source, {
          zoom: "auto",
          onRender: () => {
            recordAdaptation();
            element.dataset.peleReady = "true";
          },
          onError: value => { if (active) setError(value instanceof Error ? value.message : String(value)); },
        });
        if (window.__ROZSA_PERF__) {
          window.__ROZSA_PERF__.activePeleMounts = Number(window.__ROZSA_PERF__.activePeleMounts || 0) + 1; mountCounted = true;
        }
        recordMarkdownStage("peleMount", performance.now() - started);
        if (mounted.current.error) setError(String(mounted.current.error));
      } catch (value) { if (active) setError(value instanceof Error ? value.message : String(value)); }
    }).catch(value => { if (active) setError(value instanceof Error ? value.message : String(value)); });
    return () => {
      active = false; resizeObserver?.disconnect();
      mutationObserver?.disconnect();
      if (mountCounted && window.__ROZSA_PERF__) window.__ROZSA_PERF__.activePeleMounts = Math.max(0, Number(window.__ROZSA_PERF__.activePeleMounts || 0) - 1);
      mounted.current?.destroy(); mounted.current = undefined;
    };
  }, [source]);
  return <div className="md-mermaid-wrap" data-mermaid-source="complete">
    <div ref={target} className="md-mermaid" aria-label="Mermaid diagram" />
    {error ? <div className="md-mermaid-error" role="alert"><strong>Diagram could not be rendered.</strong> {error}<pre><code>{source}</code></pre></div> : null}
  </div>;
}

type StreamdownProps = ComponentProps<typeof Streamdown>;
function createComponents(completeMermaid: ReadonlySet<string>, staticMarkup = false): StreamdownProps["components"] {
  const Pre = ({ children, ...props }: ComponentProps<"pre">) => {
    const child = React.Children.toArray(children)[0] as ReactElement<{ className?: string; children?: ReactNode }> | undefined;
    const source = child ? codeText(child.props.children) : codeText(children);
    const language = /(?:^|\s)language-([\w-]+)/.exec(child?.props.className || "")?.[1]?.toLowerCase() || "text";
    if (language === "mermaid" && completeMermaid.has(normalizedCode(source))) return <MermaidDiagram source={source} />;
    return <div className="md-code-block"><div className="md-code-head"><span className="md-code-lang">{language}</span><CopyButton source={source} staticMarkup={staticMarkup} /></div>
      <pre {...props}><code className={child?.props.className}>{source}</code></pre></div>;
  };
  const plain = (tag: "a" | "blockquote" | "code" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "hr" | "img" | "li" | "ol" | "p" | "section" | "strong" | "sub" | "sup" | "tbody" | "td" | "th" | "thead" | "tr" | "ul") => {
    const Element = tag;
    return function PlainMarkdownElement(props: { children?: ReactNode; node?: unknown; [key: string]: unknown }) {
      const { node: _node, children, ...attributes } = props; return React.createElement(Element, attributes as never, children);
    };
  };
  return {
    ...defaultComponents,
    a: ({ children, ...props }: ComponentProps<"a">) => <a href={props.href} title={props.title} target="_blank" rel="noreferrer">{children}</a>,
    blockquote: plain("blockquote"), code: plain("code"),
    h1: plain("h1"), h2: plain("h2"), h3: plain("h3"), h4: plain("h4"), h5: plain("h5"), h6: plain("h6"),
    hr: plain("hr"), img: plain("img"), li: plain("li"), ol: plain("ol"), p: plain("p"), section: plain("section"),
    strong: plain("strong"), sub: plain("sub"), sup: plain("sup"), tbody: plain("tbody"), td: plain("td"), th: plain("th"),
    thead: plain("thead"), tr: plain("tr"), ul: plain("ul"),
    table: ({ children, ...props }: ComponentProps<"table">) => <div className="md-table-wrap"><table className="md-table" {...props}>{children}</table></div>,
    pre: Pre,
  } as unknown as StreamdownProps["components"];
}

export function StreamdownMessage({ value, active, cursor }: { value: string; active: boolean; cursor: boolean }) {
  const renderStartedAt = performance.now();
  const completeMermaid = useMemo(() => completeMermaidFences(value), [value]);
  const components = useMemo(() => createComponents(completeMermaid), [completeMermaid]);
  useLayoutEffect(() => {
    recordMarkdownStage("renderCommit", Math.max(0, performance.now() - renderStartedAt));
  }, [value]);
  return <>
    <Streamdown className="markdown-streamdown" components={components} parseIncompleteMarkdown isAnimating={active} controls={false} skipHtml>{value}</Streamdown>
    {cursor ? <span className="stream-cursor">▌</span> : null}
  </>;
}

export function renderMarkdownStaticContent(source: string): string {
  if (!staticCopyDocuments.has(document)) {
    staticCopyDocuments.add(document);
    document.addEventListener("click", event => {
      const target = event.target;
      if (!(target instanceof Element)) return;
      const button = target.closest<HTMLButtonElement>(".md-copy[data-static-copy='true']");
      const code = button?.closest(".md-code-block")?.querySelector("code");
      if (!button || !code) return;
      void navigator.clipboard.writeText(code.textContent || "").then(() => {
        button.classList.add("copied"); window.setTimeout(() => button.classList.remove("copied"), 1200);
      }).catch(error => window.showError?.(`Could not copy code: ${String(error)}`));
    });
  }
  const element = <Streamdown className="markdown-streamdown" components={createComponents(new Set(), true)} mode="static" parseIncompleteMarkdown={false} controls={false} skipHtml>{source}</Streamdown>;
  return renderToStaticMarkup(element);
}

const staticCopyDocuments = new WeakSet<Document>();

declare global {
  interface Window {
    __ROZSA_PERF__?: { markdownStages?: Record<"renderCommit" | "peleMount" | "peleAdapt", number[]>; activePeleMounts?: number; [key: string]: unknown };
    showError?: (message: string) => void;
  }
}
