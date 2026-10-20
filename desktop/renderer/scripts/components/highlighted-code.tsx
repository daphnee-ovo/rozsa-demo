import React, { useEffect, useState, type CSSProperties } from "react";
import type { HighlightResult } from "@streamdown/code";
import { highlightCode, type CodeHighlightRequest } from "../features/code-highlighter.ts";

type HighlightRequest = (request: CodeHighlightRequest, callback?: (result: HighlightResult) => void) => HighlightResult | null;
type ReadyHighlight = { code: string; language: string; result: HighlightResult };

const syntaxTokens: Record<string, string> = {
  "#eee8de": "--code-syntax-base", "#e6dfd3": "--code-syntax-base",
  "#b3a99a": "--code-syntax-comment", "#9e9588": "--code-syntax-comment",
  "#d9a7ae": "--code-syntax-keyword", "#d49aa2": "--code-syntax-keyword",
  "#c5c49a": "--code-syntax-string", "#b9b98f": "--code-syntax-string",
  "#c3ad91": "--code-syntax-number", "#c5a98a": "--code-syntax-number",
  "#d3b6a0": "--code-syntax-function", "#cda991": "--code-syntax-function",
  "#c4b9a8": "--code-syntax-type", "#b9b0a3": "--code-syntax-type",
};

function tokenStyle(style: Record<string, string | number> | undefined): CSSProperties | undefined {
  if (!style) return undefined;
  const token = syntaxTokens[String(style.color || "").toLowerCase()] || syntaxTokens[String(style["--shiki-dark"] || "").toLowerCase()];
  return token ? { color: `var(${token})` } as CSSProperties : undefined;
}

function HighlightedTokens({ result }: { result: HighlightResult }) {
  return <>{result.tokens.map((line, lineIndex) => <React.Fragment key={lineIndex}>
    {line.map((token, tokenIndex) => <span key={`${token.offset}-${tokenIndex}`} style={tokenStyle(token.htmlStyle as Record<string, string | number> | undefined)}>{token.content}</span>)}
    {lineIndex < result.tokens.length - 1 ? "\n" : null}
  </React.Fragment>)}</>;
}

export function HighlightedCode({ code, language, isIncomplete = false, requestHighlight = highlightCode }:
  CodeHighlightRequest & { requestHighlight?: HighlightRequest }) {
  const [ready, setReady] = useState<ReadyHighlight>();
  useEffect(() => {
    let current = true;
    const request = { code, language, isIncomplete };
    const receive = (result: HighlightResult) => { if (current) setReady({ code, language, result }); };
    const immediate = requestHighlight(request, receive);
    if (immediate) receive(immediate);
    return () => { current = false; };
  }, [code, language, isIncomplete, requestHighlight]);

  const result = ready?.code === code && ready.language === language ? ready.result : undefined;
  return <code className={`md-highlighted-code language-${language}`}>{result ? <HighlightedTokens result={result} /> : code}</code>;
}

