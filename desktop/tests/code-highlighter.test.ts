import test from "node:test";
import assert from "node:assert/strict";
import React, { act } from "react";
import { createRoot } from "react-dom/client";
import { createDOM, withDOM } from "./helpers/browser.js";
import { HighlightedCode } from "../renderer/scripts/components/highlighted-code.tsx";
import { highlightCode, loadCodeHighlighter } from "../renderer/scripts/features/code-highlighter.ts";
import { incompleteCodeFence } from "../renderer/scripts/features/markdown-renderer.tsx";
import type { HighlightResult } from "@streamdown/code";

const fakeResult = (content: string) => ({ tokens: [[{ content, offset: 0, htmlStyle: { color: "#eee8de" } }]], fg: "#eee8de", bg: "#322d28", themeName: "test" }) as unknown as HighlightResult;

test("open code fences stream their latest source and become closed on completion", () => {
  assert.deepEqual(incompleteCodeFence("```ts\nconst answer ="), { language: "ts", source: "const answer =" });
  assert.equal(incompleteCodeFence("```ts\nconst answer = 42\n```"), undefined);
  assert.deepEqual(incompleteCodeFence("~~~unknown\nplain <b>text</b>"), { language: "unknown", source: "plain <b>text</b>" });
});

test("shared highlighter uses supported languages and degrades unknown languages to readable text", async () => {
  const highlighter = await loadCodeHighlighter();
  assert.equal(highlighter.supportsLanguage("typescript"), true);
  assert.equal(highlighter.supportsLanguage("rozsa-unknown" as never), false);
  const output = await new Promise<HighlightResult>((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error("Highlight callback timed out")), 5000);
    const result = highlightCode({ code: "plain source", language: "rozsa-unknown", isIncomplete: true }, value => { clearTimeout(timer); resolve(value); });
    if (result) { clearTimeout(timer); resolve(result); }
  });
  assert.equal(output.tokens.flat().map(token => token.content).join(""), "plain source");
});

test("late and post-unmount highlights cannot replace current source", async () => {
  const dom = createDOM('<!doctype html><html><body><div id="root"></div></body></html>');
  await withDOM(dom, async () => {
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
    const rootElement = dom.window.document.getElementById("root")!;
    const root = createRoot(rootElement);
    const callbacks: Array<(result: HighlightResult) => void> = [];
    const requestHighlight = (_request: unknown, callback?: (result: HighlightResult) => void) => {
      callbacks.push(callback!);
      return null;
    };
    const render = async (code: string) => act(async () => root.render(React.createElement(HighlightedCode, { code, language: "typescript", isIncomplete: true, requestHighlight: requestHighlight as never })));
    await render("first");
    assert.equal(rootElement.querySelector("code")?.textContent, "first");
    await render("second");
    await act(async () => callbacks[0]!(fakeResult("stale")));
    assert.equal(rootElement.querySelector("code")?.textContent, "second");
    await act(async () => callbacks[1]!(fakeResult("second")));
    assert.equal(rootElement.querySelector("code span")?.textContent, "second");
    await render("<img src=x onerror=alert(1)>");
    assert.equal(rootElement.querySelector("img"), null);
    assert.equal(rootElement.querySelector("code")?.textContent, "<img src=x onerror=alert(1)>");
    const latest = callbacks.at(-1)!;
    await act(async () => root.unmount());
    await act(async () => latest(fakeResult("after unmount")));
    assert.equal(rootElement.textContent, "");
    dom.window.close();
  });
});
