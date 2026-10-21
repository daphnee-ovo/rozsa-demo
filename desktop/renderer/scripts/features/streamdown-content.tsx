import React, { createContext, useContext, useEffect, useLayoutEffect, useMemo, useRef, useState, type ComponentProps, type ReactElement, type ReactNode } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { createPortal } from "react-dom";
import { CodeXml, Copy, Download, Maximize, Minimize, RotateCcw, Workflow, ZoomIn, ZoomOut } from "lucide-react";
import { Streamdown, defaultComponents } from "streamdown";
import type { Mounted } from "pele";
import { completeMermaidFences, incompleteCodeFence } from "./markdown-renderer.tsx";
import { HighlightedCode } from "../components/highlighted-code.tsx";
import { reportError, resolveError } from "../../error-reporting.ts";

function recordMarkdownStage(stage: "renderCommit" | "peleMount" | "peleAdapt", duration: number): void {
  const perf = window.__ROZSA_PERF__;
  if (!perf) return;
  perf.markdownStages ??= { renderCommit: [], peleMount: [], peleAdapt: [] };
  perf.markdownStages[stage].push(duration);
}

function normalizedCode(source: string): string { return source.replace(/\r\n/g, "\n").replace(/\n+$/, ""); }

function mermaidSourceId(source: string): string {
  let hash = 2166136261;
  for (let index = 0; index < source.length; index += 1) {
    hash = Math.imul(hash ^ source.charCodeAt(index), 16777619);
  }
  return (hash >>> 0).toString(36);
}

function iconMarkup(Icon: typeof CodeXml) {
  return renderToStaticMarkup(<Icon className="lucide" size={16} strokeWidth={1.7} aria-hidden="true" />);
}
function codeText(node: ReactNode): string {
  if (Array.isArray(node)) return node.map(codeText).join("");
  if (typeof node === "string" || typeof node === "number") return String(node);
  if (React.isValidElement<{ children?: ReactNode }>(node)) return codeText(node.props.children);
  return "";
}

async function saveDiagramImage(element: HTMLElement): Promise<void> {
  const svg = element.querySelector("svg.pele");
  if (!(svg instanceof SVGSVGElement)) throw new Error("Diagram is not ready to save");
  const clone = svg.cloneNode(true) as SVGSVGElement;
  const originals = [svg, ...svg.querySelectorAll("*")];
  const copies = [clone, ...clone.querySelectorAll("*")];
  const properties = ["fill", "stroke", "stroke-width", "stroke-linecap", "stroke-linejoin", "opacity", "font-family", "font-size", "font-weight", "letter-spacing", "text-anchor", "rx"];
  originals.forEach((original, index) => {
    const style = getComputedStyle(original);
    copies[index].removeAttribute("style");
    for (const property of properties) (copies[index] as SVGElement).style.setProperty(property, style.getPropertyValue(property));
  });
  const width = Number(svg.getAttribute("width")), height = Number(svg.getAttribute("height"));
  if (!(width > 0 && height > 0)) throw new Error("Diagram has invalid dimensions");
  // Export the complete diagram, independent of the current pan and zoom.
  clone.setAttribute("viewBox", `0 0 ${width} ${height}`);
  clone.setAttribute("width", String(width)); clone.setAttribute("height", String(height));
  const image = new Image();
  image.src = `data:image/svg+xml;charset=utf-8,${encodeURIComponent(new XMLSerializer().serializeToString(clone))}`;
  await image.decode();
  const canvas = document.createElement("canvas");
  const scale = Math.min(2, 8192 / Math.max(width, height));
  canvas.width = Math.ceil(width * scale); canvas.height = Math.ceil(height * scale);
  const context = canvas.getContext("2d");
  if (!context) throw new Error("Image export is unavailable");
  context.drawImage(image, 0, 0, canvas.width, canvas.height);
  const link = document.createElement("a");
  link.download = "diagram.png"; link.href = canvas.toDataURL("image/png");
  document.body.append(link);
  try { link.click(); } finally { link.remove(); }
}

function CopyButton({ source, staticMarkup = false }: { source: string; staticMarkup?: boolean }) {
  return <button className="md-copy" type="button" aria-label="Copy code" title="Copy code"
    data-static-copy={staticMarkup ? "true" : undefined}
    onClick={event => { const button = event.currentTarget; void navigator.clipboard.writeText(source).then(() => {
      button.classList.add("copied");
      window.setTimeout(() => button.classList.remove("copied"), 1200);
    }).catch(error => window.showError?.(`Could not copy code: ${String(error)}`)); }}>
    <Copy className="lucide" size={16} strokeWidth={1.7} aria-hidden="true" />
  </button>;
}

function MermaidDiagram({ source }: { source: string }) {
  const wrap = useRef<HTMLDivElement>(null);
  const target = useRef<HTMLDivElement>(null);
  const actions = useRef<HTMLDivElement>(null);
  const mounted = useRef<Mounted | undefined>(undefined);
  const [error, setError] = useState<string>();
  const [showSource, setShowSource] = useState(false);
  const [fullscreen, setFullscreen] = useState(false);
  useEffect(() => {
    const clearMermaidFocus = () => {
      const active = document.activeElement;
      if (active instanceof HTMLElement && wrap.current?.contains(active)) active.blur();
    };
    window.addEventListener("blur", clearMermaidFocus);
    return () => window.removeEventListener("blur", clearMermaidFocus);
  }, []);
  useEffect(() => {
    if (!fullscreen) return;
    const previousFocus = document.activeElement;
    const siblings = Array.from(document.body.children).filter(element => element !== wrap.current) as HTMLElement[];
    const inert = siblings.map(element => element.inert);
    siblings.forEach(element => { element.inert = true; });
    wrap.current?.querySelector<HTMLButtonElement>('[aria-label="Exit fullscreen"]')?.focus();
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") { event.preventDefault(); setFullscreen(false); }
      if (event.key !== "Tab") return;
      const buttons = Array.from(wrap.current?.querySelectorAll<HTMLButtonElement>('button:not(:disabled)') || []).filter(button => getComputedStyle(button).display !== "none");
      const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
      event.preventDefault(); buttons[(index + (event.shiftKey ? -1 : 1) + buttons.length) % buttons.length]?.focus();
    };
    document.addEventListener("keydown", onKey);
    return () => {
      siblings.forEach((element, index) => { element.inert = inert[index]; });
      document.removeEventListener("keydown", onKey);
      if (previousFocus instanceof HTMLElement && previousFocus.isConnected) previousFocus.focus();
      else queueMicrotask(() => wrap.current?.querySelector<HTMLButtonElement>('[aria-label="Enter fullscreen"]')?.focus({ preventScroll: true }));
    };
  }, [fullscreen]);
  useLayoutEffect(() => {
    const element = target.current;
    if (!element) return;
    const messageIdentity = wrap.current?.closest<HTMLElement>(".msg")?.dataset.renderErrorId;
    const errorOptions = {
      id: messageIdentity ? `mermaid-render:${messageIdentity}:${mermaidSourceId(source)}` : undefined,
      title: "Diagram error",
    };
    const blockDoubleClickZoom = (event: MouseEvent) => {
      event.preventDefault();
      event.stopImmediatePropagation();
    };
    element.addEventListener("dblclick", blockDoubleClickZoom, true);
    const styleZoomControls = () => {
      const controls = element.querySelector<HTMLElement>(".pele-zoom");
      if (!controls) return;
      if (actions.current && controls.parentElement !== actions.current) actions.current.append(controls);
      for (const [label, Icon] of [["Zoom in", ZoomIn], ["Zoom out", ZoomOut], ["Reset zoom", RotateCcw]] as const) {
        const button = controls.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`);
        if (button && !button.querySelector(".lucide")) button.innerHTML = iconMarkup(Icon);
      }
    };
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
    const mutationObserver = typeof MutationObserver === "undefined" ? undefined : new MutationObserver(() => { recordAdaptation(); styleZoomControls(); });
    mutationObserver?.observe(element, { attributes: true, childList: true, characterData: true, subtree: true });
    const started = performance.now();
    void import("pele").then(({ mount }) => {
      if (!active) return;
      try {
        mounted.current = mount(element, source, {
          fontSize: parseFloat(getComputedStyle(element).fontSize) || 16,
          zoom: { always: true, controls: true },
          onRender: () => {
            recordAdaptation();
            styleZoomControls();
            element.dataset.peleReady = "true";
          },
          onError: value => { if (active) { reportError(value, errorOptions); setError(value instanceof Error ? value.message : String(value)); } },
        });
        styleZoomControls();
        if (window.__ROZSA_PERF__) {
          window.__ROZSA_PERF__.activePeleMounts = Number(window.__ROZSA_PERF__.activePeleMounts || 0) + 1; mountCounted = true;
        }
        recordMarkdownStage("peleMount", performance.now() - started);
        if (mounted.current.error) { reportError(mounted.current.error, errorOptions); setError(String(mounted.current.error)); }
        else {
          if (errorOptions.id) resolveError(errorOptions.id);
          setError(undefined);
        }
      } catch (value) { if (active) { reportError(value, errorOptions); setError(value instanceof Error ? value.message : String(value)); } }
    }).catch(value => { if (active) { reportError(value, errorOptions); setError(value instanceof Error ? value.message : String(value)); } });
    return () => {
      active = false; resizeObserver?.disconnect();
      mutationObserver?.disconnect();
      element.removeEventListener("dblclick", blockDoubleClickZoom, true);
      if (mountCounted && window.__ROZSA_PERF__) window.__ROZSA_PERF__.activePeleMounts = Math.max(0, Number(window.__ROZSA_PERF__.activePeleMounts || 0) - 1);
      mounted.current?.destroy(); mounted.current = undefined;
      actions.current?.replaceChildren();
      element.replaceChildren();
    };
  }, [source, showSource, fullscreen]);
  const returnToDiagram = <button className="md-copy md-mermaid-return" type="button" aria-label="Show diagram" title="Show diagram" onClick={() => setShowSource(false)}><Workflow className="lucide" size={16} strokeWidth={1.7} aria-hidden="true" /></button>;
  if (showSource) return <section className="md-code-block md-mermaid-source">
    <div className="md-code-head"><span className="md-code-lang">mermaid</span>{returnToDiagram}<CopyButton source={source} /></div>
    <pre><code className="language-mermaid">{source}</code></pre>
  </section>;
  const diagram = <div ref={wrap} className={`md-mermaid-wrap${fullscreen ? " md-mermaid-fullscreen" : ""}`} data-mermaid-source="complete"
    role={fullscreen ? "dialog" : undefined} aria-modal={fullscreen ? true : undefined} aria-label={fullscreen ? "Diagram viewer" : undefined}>
    <div ref={target} className="md-mermaid" aria-label="Mermaid diagram" />
    <div className="md-mermaid-actions">
      {!fullscreen && <button className="md-mermaid-tool" type="button" aria-label="Show source" title="Show source" onClick={() => setShowSource(true)}><CodeXml className="lucide" size={16} strokeWidth={1.7} aria-hidden="true" /></button>}
      <div ref={actions} className="md-mermaid-pele-actions" />
      {fullscreen && <button className="md-mermaid-tool" type="button" aria-label="Save image" title="Save image" onClick={() => {
        if (target.current) void saveDiagramImage(target.current).catch(value => window.showError?.(`Could not save diagram: ${String(value)}`));
      }}><Download className="lucide" size={16} strokeWidth={1.7} aria-hidden="true" /></button>}
      <button className="md-mermaid-tool" type="button" aria-label={fullscreen ? "Exit fullscreen" : "Enter fullscreen"} title={fullscreen ? "Exit fullscreen" : "Enter fullscreen"}
        onClick={() => setFullscreen(value => !value)}>
        {fullscreen ? <Minimize className="lucide" size={16} strokeWidth={1.7} aria-hidden="true" /> : <Maximize className="lucide" size={16} strokeWidth={1.7} aria-hidden="true" />}
      </button>
    </div>
    {error ? <div className="md-mermaid-error" role="alert"><strong>Diagram could not be rendered.</strong><pre><code>{source}</code></pre></div> : null}
  </div>;
  return fullscreen ? createPortal(diagram, document.body) : diagram;
}

type StreamdownProps = ComponentProps<typeof Streamdown>;
const CompleteMermaidContext = createContext<ReadonlySet<string>>(new Set());
const StaticMarkupContext = createContext(false);
const IncompleteCodeContext = createContext<{ language: string; source: string } | undefined>(undefined);

function MarkdownCodeBlock({ children, ...props }: ComponentProps<"pre">) {
  const completeMermaid = useContext(CompleteMermaidContext);
  const staticMarkup = useContext(StaticMarkupContext);
  const incompleteCode = useContext(IncompleteCodeContext);
  const child = React.Children.toArray(children)[0] as ReactElement<{ className?: string; children?: ReactNode }> | undefined;
  const source = child ? codeText(child.props.children) : codeText(children);
  const language = /(?:^|\s)language-([\w-]+)/.exec(child?.props.className || "")?.[1]?.toLowerCase() || "text";
  const isIncomplete = incompleteCode?.language === language && incompleteCode.source === normalizedCode(source);
  if (language === "mermaid" && completeMermaid.has(normalizedCode(source))) return <MermaidDiagram source={source} />;
  return <div className="md-code-block"><div className="md-code-head"><span className="md-code-lang">{language}</span><CopyButton source={source} staticMarkup={staticMarkup} /></div>
    <pre {...props}><HighlightedCode code={source} language={language} isIncomplete={isIncomplete} /></pre></div>;
}

const markdownComponents: StreamdownProps["components"] = (() => {
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
    pre: MarkdownCodeBlock,
  } as unknown as StreamdownProps["components"];
})();

export function StreamdownMessage({ value, active, cursor }: { value: string; active: boolean; cursor: boolean }) {
  const renderStartedAt = performance.now();
  const completeMermaid = useMemo(() => completeMermaidFences(value), [value]);
  const incompleteCode = useMemo(() => incompleteCodeFence(value), [value]);
  useLayoutEffect(() => {
    recordMarkdownStage("renderCommit", Math.max(0, performance.now() - renderStartedAt));
  }, [value]);
  return <CompleteMermaidContext.Provider value={completeMermaid}><IncompleteCodeContext.Provider value={active ? incompleteCode : undefined}>
    <Streamdown className="markdown-streamdown" components={markdownComponents} parseIncompleteMarkdown isAnimating={active} controls={false} skipHtml>{value}</Streamdown>
    {cursor ? <span className="stream-cursor">▌</span> : null}
  </IncompleteCodeContext.Provider></CompleteMermaidContext.Provider>;
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
  const element = <StaticMarkupContext.Provider value><Streamdown className="markdown-streamdown" components={markdownComponents} mode="static" parseIncompleteMarkdown={false} controls={false} skipHtml>{source}</Streamdown></StaticMarkupContext.Provider>;
  return renderToStaticMarkup(element);
}

const staticCopyDocuments = new WeakSet<Document>();

declare global {
  interface Window {
    __ROZSA_PERF__?: { markdownStages?: Record<"renderCommit" | "peleMount" | "peleAdapt", number[]>; activePeleMounts?: number; [key: string]: unknown };
    showError?: (message: string) => void;
  }
}
