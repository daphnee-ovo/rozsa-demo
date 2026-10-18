import React, { Suspense, lazy, type ComponentType } from "react";

function normalizedCode(source: string): string { return source.replace(/\r\n/g, "\n").replace(/\n+$/, ""); }

export function completeMermaidFences(markdown: string): Set<string> {
  const sources = new Set<string>();
  let fence: { marker: "`" | "~"; length: number; mermaid: boolean; lines: string[] } | undefined;
  for (const line of markdown.replace(/\r\n/g, "\n").split("\n")) {
    const trimmed = line.replace(/^ {0,3}/, "");
    if (!fence) {
      const open = /^(\`{3,}|~{3,})(.*)$/.exec(trimmed);
      if (!open) continue;
      fence = { marker: open[1]![0] as "`" | "~", length: open[1]!.length,
        mermaid: open[2]!.trim().split(/[\t ]+/, 1)[0]!.toLowerCase() === "mermaid", lines: [] };
      continue;
    }
    if (new RegExp(`^${fence.marker}{${fence.length},}[\\t ]*$`).test(trimmed)) {
      if (fence.mermaid) sources.add(normalizedCode(fence.lines.join("\n")));
      fence = undefined;
    } else fence.lines.push(line);
  }
  return sources;
}

const StreamdownMessage = lazy(async () => {
  const module = await import("./streamdown-content.tsx");
  return { default: module.StreamdownMessage as ComponentType<{ value: string; active: boolean; cursor: boolean }> };
});

export function MarkdownContent({ value, active = false, cursor = false }: { value: string; active?: boolean; cursor?: boolean }) {
  if (!value) return null;
  return <Suspense fallback={<div className="markdown-loading" aria-busy="true">{value}</div>}>
    <StreamdownMessage value={value} active={active} cursor={cursor} />
  </Suspense>;
}

export async function renderMarkdownStatic(source: string): Promise<string> {
  if (!source) return "";
  return (await import("./streamdown-content.tsx")).renderMarkdownStaticContent(source);
}

declare global {
  interface Window {
    __ROZSA_PERF__?: { markdownStages?: Record<"renderCommit" | "peleMount" | "peleAdapt", number[]>; activePeleMounts?: number; [key: string]: unknown };
    showError?: (message: string) => void;
  }
}
