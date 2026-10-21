import { afterAll, test } from "vitest";
import assert from "node:assert/strict";
import { getAllByRole, getByRole, getByText } from "@testing-library/dom";
import { act } from "react";
import { createConversationRenderer } from "../renderer/scripts/features/conversation-view.tsx";
import { completeMermaidFences } from "../renderer/scripts/features/markdown-renderer.tsx";
import * as conversation from "../renderer/scripts/features/conversation.ts";
import { createDOM, loadApp, readFrontend, withDOM } from "./helpers/browser.js";

const previousActFlag = globalThis.IS_REACT_ACT_ENVIRONMENT;
globalThis.IS_REACT_ACT_ENVIRONMENT = true;
afterAll(() => {
  if (previousActFlag === undefined) delete globalThis.IS_REACT_ACT_ENVIRONMENT;
  else globalThis.IS_REACT_ACT_ENVIRONMENT = previousActFlag;
});

async function withMessageApp(action, html = '<div id="chatMessages"></div><div id="msgInput" contenteditable="true">draft text</div>') {
  const app = loadApp({ html });
  return withDOM(app.dom, async () => {
    const chat = app.document.getElementById("chatMessages");
    Object.defineProperties(chat, {
      clientHeight: { configurable: true, value: 720 },
      clientWidth: { configurable: true, value: 960 },
      getBoundingClientRect: { configurable: true, value: () => ({ top: 0, left: 0, right: 960, bottom: 720, width: 960, height: 720, x: 0, y: 0, toJSON() {} }) },
    });
    const renderer = createConversationRenderer(chat);
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
  await act(async () => {
    app.evaluate(script);
    await new Promise(resolve => setTimeout(resolve, 20));
  });
  const deadline = Date.now() + 3000;
  while (app.document.querySelector(".markdown-loading")) {
    if (Date.now() >= deadline) assert.fail("Markdown content did not finish loading");
    await new Promise(resolve => setTimeout(resolve, 25));
  }
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
    const rows = container.querySelectorAll(".conversation-virtual-row .msg");
    assert.equal(rows.length, 3, container.innerHTML);
    assert.equal(rows[0].querySelector(".markdown-body")?.textContent, "用户你好");
    assert.ok(rows[1].textContent.includes("const 答案 = 42;"));
    getByRole(rows[1], "button", { name: "Copy code" });
    assert.equal(getByRole(rows[1], "table").textContent.includes("名称"), true);
    assert.equal(getAllByRole(rows[1], "listitem").length, 2);
    const link = getByRole(rows[1], "link", { name: "文档" });
    assert.equal(link.getAttribute("rel"), "noreferrer");
    assert.equal(link.getAttribute("target"), "_blank");
    const image = getByRole(rows[1], "img", { name: "架构图" });
    assert.equal(image.getAttribute("src"), "https://images.example.test/diagram.png");
    assert.ok(rows[1].textContent?.includes("界".repeat(300)));
    assert.equal(rows[2].querySelector(".markdown-body")?.textContent, "系统提示");

    const policy = readFrontend("index.html").match(/Content-Security-Policy" content="([^"]+)"/)?.[1];
    assert.match(policy || "", /img-src 'self' data:/);
    assert.doesNotMatch(policy || "", /img-src[^;]*https:/);
    const css = readFrontend("styles/features/conversation.css");
    assert.match(css, /\.msg-content,\s*\.markdown-body[^}]*overflow-wrap:\s*anywhere/s);
  });
});

test("streaming Markdown keeps open code readable and commits the latest closed source", async () => {
  await withMessageApp(async app => {
    const container = app.document.getElementById("chatMessages");
    const streaming = { kind: "standard", entryId: "highlight-stream", message: { role: "assistant", content: [{ type: "text", text: "```ts\nconst answer = 4" }] } };
    app.window.__messages = [
      { kind: "standard", entryId: "highlight-user", message: { role: "user", content: [{ type: "text", text: "```ts\nconst value = 1\n```" }] } },
      streaming,
    ];
    await render(app, 'renderMessages(window.__messages, true, "session-highlight")');
    assert.ok(container.textContent.includes("const value = 1"));
    assert.ok(container.textContent.includes("const answer = 4"));

    const complete = { ...streaming, message: { role: "assistant", content: [{ type: "text", text: "```ts\nconst answer = 42\n```" }] } };
    app.window.__messages = [app.window.__messages[0], complete];
    await render(app, 'renderMessages(window.__messages, false, "session-highlight")');
    assert.ok(container.textContent.includes("const answer = 42"));
  });
});

test("streaming partials and continuation remain visible while provider errors use notifications", async () => {
  await withMessageApp(async app => {
    const first = { kind: "standard", entryId: "assistant-first", message: { role: "assistant", content: [{ type: "text", text: "部分" }] } };
    const partial = { kind: "standard", entryId: "assistant-first", message: { role: "assistant", content: [{ type: "text", text: "部分回答" }] } };
    app.window.__messages = [first];
    await render(app, 'renderMessages(window.__messages, true, "session-stream")');
    const container = app.document.getElementById("chatMessages");
    assert.equal(container.querySelectorAll(".conversation-virtual-row .msg").length, 1);
    getByText(container, "▌");
    assert.ok(container.querySelector(".markdown-body")?.textContent?.includes("部分"));
    app.window.__messages = [partial];
    await render(app, 'renderMessages(window.__messages, true, "session-stream")');
    assert.equal(container.querySelectorAll(".conversation-virtual-row .msg").length, 1);
    assert.ok(container.querySelector(".markdown-body")?.textContent?.includes("部分回答"));
    app.window.__messages = [partial, { kind: "standard", entryId: "assistant-next", message: { role: "assistant", content: [{ type: "text", text: "续写" }] } }];
    await render(app, 'renderMessages(window.__messages, true, "session-stream")');
    const continuation = container.querySelectorAll(".conversation-virtual-row .msg")[1];
    assert.equal(container.querySelectorAll(".conversation-virtual-row .msg").length, 2);
    assert.ok(continuation.textContent.includes("续写"));

    app.window.__messages = [{ kind: "standard", entryId: "image-1", message: { role: "user", content: [{ type: "image", data: "not-base64", mimeType: "text/html" }] } }];
    await render(app, 'renderMessages(window.__messages, false, "session-image")');
    getByRole(container, "img", { name: "Image unavailable" });

    const error = { kind: "standard", entryId: "error-1", message: { role: "assistant", errorMessage: "429: rate limit", content: [] } };
    const notifications = [];
    app.window.upsertNotification = notification => notifications.push(notification);
    app.window.resolveNotification = () => {};
    app.window.__messages = [error];
    await render(app, 'renderMessages(window.__messages, false, "session-error")');
    assert.deepEqual(notifications.map(({ id, message }) => ({ id, message })), [
      { id: "message-render:session-error:error-1", message: "429: rate limit" },
    ]);
    assert.equal(container.textContent.includes("429: rate limit"), false);
  });
});

test("streaming updates keep completed message text visible", async () => {
  await withMessageApp(async app => {
    const completed = { kind: "standard", entryId: "finished", message: { role: "user", content: [{ type: "text", text: "old" }] } };
    const firstPartial = { kind: "standard", entryId: "current", message: { role: "assistant", content: [{ type: "text", text: "a" }] } };
    app.window.__messages = [completed, firstPartial];
    await render(app, 'renderMessages(window.__messages, true, "session-performance")');
    const nextPartial = { kind: "standard", entryId: "current", message: { role: "assistant", content: [{ type: "text", text: "ab" }] } };
    app.window.__messages = [completed, nextPartial];
    await render(app, 'renderMessages(window.__messages, true, "session-performance")');
    const visibleMessages = app.document.querySelector("#chatMessages").textContent;
    assert.ok(visibleMessages.includes("old"));
    assert.ok(visibleMessages.includes("ab"));
  });
});

test("Mermaid source is eligible only after its matching source fence closes", () => {
  const prefix = "```mermaid\nflowchart LR\n  A --> B\n";
  assert.equal(completeMermaidFences(prefix).size, 0);
  assert.deepEqual([...completeMermaidFences(`${prefix}\`\`\``)], ["flowchart LR\n  A --> B"]);
  assert.deepEqual([...completeMermaidFences("~~~mermaid title=ready\nsequenceDiagram\n  A->>B: hello\n~~~")], ["sequenceDiagram\n  A->>B: hello"]);
  assert.equal(completeMermaidFences("~~~mermaid\nflowchart LR\n  A --> B\n```").size, 0);
  assert.equal(completeMermaidFences("```ts\nconst x = 1;\n```").size, 0);
});

test("an incomplete Mermaid fence stays readable and becomes an interactive placeholder only at closure", async () => {
  await withMessageApp(async app => {
    const prefix = "Before the diagram.\n\n```mermaid\nflowchart LR\n  A --> B\n";
    const partial = { kind: "standard", entryId: "mermaid-stream", activeStream: true, streamTarget: "text", message: { role: "assistant", content: [{ type: "text", text: prefix }] } };
    app.window.__messages = [partial];
    await render(app, 'renderMessages(window.__messages, true, "session-mermaid")');
    assert.equal(app.document.querySelector(".md-mermaid-wrap"), null);
    assert.ok(app.document.querySelector(".md-code-block code")?.textContent?.includes("A --> B"));

    const complete = { ...partial, activeStream: false, message: { role: "assistant", content: [{ type: "text", text: `${prefix}\`\`\`` }] } };
    app.window.__messages = [complete];
    await render(app, 'renderMessages(window.__messages, false, "session-mermaid")');
    assert.equal(app.document.querySelector(".md-mermaid-wrap")?.getAttribute("data-mermaid-source"), "complete");
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
