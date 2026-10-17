import test from "node:test";
import assert from "node:assert/strict";
import { act } from "react";
import { createConversationRenderer } from "../renderer/scripts/features/conversation-view.tsx";
import * as conversation from "../renderer/scripts/features/conversation.ts";
import { createDOM, loadApp, readFrontend, withDOM } from "./helpers/browser.js";

const previousActFlag = globalThis.IS_REACT_ACT_ENVIRONMENT;
globalThis.IS_REACT_ACT_ENVIRONMENT = true;
test.after(() => {
  if (previousActFlag === undefined) delete globalThis.IS_REACT_ACT_ENVIRONMENT;
  else globalThis.IS_REACT_ACT_ENVIRONMENT = previousActFlag;
});

async function withMessageApp(action, html = '<div id="chatMessages"></div><div id="msgInput" contenteditable="true">draft text</div>') {
  const app = loadApp({ html });
  return withDOM(app.dom, async () => {
    const renderer = createConversationRenderer(app.document.getElementById("chatMessages"));
    app.window.RozsaConversation = { ...conversation, renderView: renderer.render };
    app.window.RozsaTools = { messageTextMarkup: String };
    app.evaluate("renderToolChips = () => {}; updateInputHighlight = () => {}; autoResize = () => {}; updateAbortButton = () => {};");
    try { await action(app); }
    finally {
      await act(async () => renderer.dispose());
      app.close();
    }
  });
}

async function render(app, script) {
  await act(async () => { app.evaluate(script); });
}

test("React messages preserve user, assistant and system roles plus rich Markdown", async () => {
  await withMessageApp(async app => {
    const markdown = [
      "代码示例：", "```ts", "const 答案 = 42;", "```",
      "| 名称 | 状态 |", "| --- | --- |", "| Rózsa | 完成 |", "",
      "- 第一项", "- 第二项", "", "[文档](https://example.test/docs)", "",
      "![架构图](https://images.example.test/diagram.png)", "", "超长内容：" + "界".repeat(300),
    ].join("\n");
    app.window.__messages = [
      { kind: "standard", entryId: "user-1", message: { role: "user", content: [{ type: "text", text: "用户你好" }] } },
      { kind: "standard", entryId: "assistant-1", message: { role: "assistant", content: [{ type: "text", text: markdown }] } },
      { kind: "standard", entryId: "system-1", message: { role: "system", content: [{ type: "text", text: "系统提示" }] } },
    ];
    await render(app, 'renderMessages(window.__messages, false, "session-markdown")');
    const container = app.document.getElementById("chatMessages");
    assert.equal(container.children.length, 3);
    assert.match(container.children[0].className, /msg-user/);
    assert.equal(container.children[0].querySelector(".markdown-body")?.textContent, "用户你好");
    assert.match(container.children[1].querySelector(".msg-role")?.textContent || "", /Rozsa/);
    assert.ok(container.children[1].querySelector(".md-code-block code")?.textContent.includes("const 答案 = 42;"));
    assert.ok(container.children[1].querySelector(".md-table th")?.textContent.includes("名称"));
    assert.equal(container.children[1].querySelectorAll(".markdown-body li").length, 2);
    const link = container.children[1].querySelector('a[href="https://example.test/docs"]');
    assert.equal(link?.getAttribute("rel"), "noreferrer");
    assert.equal(link?.target, "_blank");
    const image = container.children[1].querySelector("img");
    assert.equal(image?.getAttribute("alt"), "架构图");
    assert.equal(image?.getAttribute("src"), "https://images.example.test/diagram.png");
    assert.ok(container.children[1].textContent?.includes("界".repeat(300)));
    assert.match(container.children[2].className, /msg-system/);
    assert.equal(container.children[2].querySelector(".msg-role")?.textContent, "System");
    assert.equal(container.children[2].querySelector(".markdown-body")?.textContent, "系统提示");

    const policy = readFrontend("index.html").match(/Content-Security-Policy" content="([^"]+)"/)?.[1];
    assert.match(policy || "", /img-src 'self' data:/);
    assert.doesNotMatch(policy || "", /img-src[^;]*https:/);
    const css = readFrontend("styles/features/conversation.css");
    assert.match(css, /\.msg-content,\s*\.markdown-body[^}]*overflow-wrap:\s*anywhere/s);
  });
});

test("streaming partials, continuation, image blocks and provider errors remain visible", async () => {
  await withMessageApp(async app => {
    const first = { kind: "standard", entryId: "assistant-first", message: { role: "assistant", content: [{ type: "text", text: "部分" }] } };
    const partial = { kind: "standard", entryId: "assistant-first", message: { role: "assistant", content: [{ type: "text", text: "部分回答" }] } };
    app.window.__messages = [first];
    await render(app, 'renderMessages(window.__messages, true, "session-stream")');
    const container = app.document.getElementById("chatMessages");
    assert.equal(container.children.length, 1);
    assert.ok(container.querySelector(".stream-cursor"));
    assert.ok(container.querySelector(".markdown-body")?.textContent?.includes("部分"));
    app.window.__messages = [partial];
    await render(app, 'renderMessages(window.__messages, true, "session-stream")');
    assert.equal(container.children.length, 1);
    assert.ok(container.querySelector(".markdown-body")?.textContent?.includes("部分回答"));
    app.window.__messages = [partial, { kind: "standard", entryId: "assistant-next", message: { role: "assistant", content: [{ type: "text", text: "续写" }] } }];
    await render(app, 'renderMessages(window.__messages, true, "session-stream")');
    assert.equal(container.children.length, 2);
    assert.match(container.children[1].className, /msg-assistant-continuation/);
    assert.equal(container.children[1].querySelector(".msg-role"), null);

    app.window.__messages = [{ kind: "standard", entryId: "image-1", message: { role: "user", content: [{ type: "image", data: "not-base64", mimeType: "text/html" }] } }];
    await render(app, 'renderMessages(window.__messages, false, "session-image")');
    assert.equal(container.querySelector(".msg-image-blocked")?.getAttribute("aria-label"), "Image unavailable");

    const error = { kind: "standard", entryId: "error-1", message: { role: "assistant", errorMessage: "429: rate limit", content: [] } };
    app.window.__messages = [error];
    await render(app, 'renderMessages(window.__messages, false, "session-error")');
    assert.equal(container.querySelector(".msg-error pre")?.textContent, "429: rate limit");
  });
});

test("memoized React rows keep completed messages and skip their Markdown work during streaming", async () => {
  await withMessageApp(async app => {
    const originalMarkdown = app.window.renderMarkdown;
    let parsed = 0;
    app.window.renderMarkdown = text => { parsed++; return originalMarkdown(text); };
    const completed = { kind: "standard", entryId: "finished", message: { role: "user", content: [{ type: "text", text: "old" }] } };
    const firstPartial = { kind: "standard", entryId: "current", message: { role: "assistant", content: [{ type: "text", text: "a" }] } };
    app.window.__messages = [completed, firstPartial];
    await render(app, 'renderMessages(window.__messages, true, "session-performance")');
    const rows = app.document.querySelectorAll("#chatMessages > .msg");
    const completedNode = rows[0];
    const before = parsed;
    const nextPartial = { kind: "standard", entryId: "current", message: { role: "assistant", content: [{ type: "text", text: "ab" }] } };
    app.window.__messages = [completed, nextPartial];
    await render(app, 'renderMessages(window.__messages, true, "session-performance")');
    const updatedRows = app.document.querySelectorAll("#chatMessages > .msg");
    assert.equal(updatedRows[0], completedNode);
    assert.equal(parsed - before, 1);
    assert.ok(updatedRows[1].textContent?.includes("ab"));
  });
});

test("scroll position and composer caret survive a renderer reload", async () => {
  let savedView, savedDraft;
  await withMessageApp(async first => {
    const chat = first.document.getElementById("chatMessages");
    Object.defineProperty(chat, "scrollHeight", { configurable: true, value: 800 });
    Object.defineProperty(chat, "clientHeight", { configurable: true, value: 200 });
    await render(first, 'renderMessages([], false, "session-a")');
    Object.defineProperty(first.document.getElementById("chatMessages"), "scrollHeight", { configurable: true, value: 800 });
    Object.defineProperty(first.document.getElementById("chatMessages"), "clientHeight", { configurable: true, value: 200 });
    first.evaluate('chatAutoScrollPaused = true; document.getElementById("chatMessages").scrollTop = 137; persistSessionViewState("session-a", document.getElementById("chatMessages"));');
    const input = first.document.getElementById("msgInput");
    const range = first.document.createRange();
    range.setStart(input.firstChild, 6); range.setEnd(input.firstChild, 10);
    first.window.getSelection().removeAllRanges(); first.window.getSelection().addRange(range);
    first.evaluate('activeSessionId = "session-a"; captureSessionDraft("session-a");');
    savedView = first.window.sessionStorage.getItem("rozsa:conversation-views:v1");
    savedDraft = first.window.sessionStorage.getItem("rozsa:conversation-drafts:v1");
  }, '<div id="chatMessages"></div><div id="msgInput" contenteditable="true">alpha beta gamma</div>');

  await withMessageApp(async second => {
    second.window.sessionStorage.setItem("rozsa:conversation-views:v1", savedView);
    second.window.sessionStorage.setItem("rozsa:conversation-drafts:v1", savedDraft);
    const chat = second.document.getElementById("chatMessages");
    Object.defineProperty(chat, "scrollHeight", { configurable: true, value: 800 });
    Object.defineProperty(chat, "clientHeight", { configurable: true, value: 200 });
    await render(second, "restoreConversationState(); renderMessages([], false, 'session-a');");
    await act(async () => { await new Promise(resolve => second.window.requestAnimationFrame(resolve)); await new Promise(resolve => second.window.requestAnimationFrame(resolve)); });
    assert.equal(chat.scrollTop, 137);
    const input = second.document.getElementById("msgInput");
    second.evaluate('restoreSessionDraft("session-a");');
    assert.equal(input.textContent, "alpha beta gamma");
    const selection = second.evaluate('getInputSelection(document.getElementById("msgInput"))');
    assert.deepEqual({ start: selection.start, end: selection.end }, { start: 6, end: 10 });
  }, '<div id="chatMessages"></div><div id="msgInput" contenteditable="true"></div>');
});
