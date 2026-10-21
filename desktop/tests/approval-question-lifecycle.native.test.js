import assert from "node:assert/strict";
import { createServer } from "node:http";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { test } from "node:test";
import { attachPage, createEnvironment, startElectron } from "./performance/harness.js";

function toolChunk(calls) {
  return [
    { choices: [{ delta: { tool_calls: calls.map((call, index) => ({ index, id: call.id, type: "function", function: { name: call.name, arguments: JSON.stringify(call.arguments) } })) } }] },
    { choices: [{ delta: {}, finish_reason: "tool_calls" }], usage: { prompt_tokens: 24, completion_tokens: 5 } },
  ];
}

function textChunk(text) {
  return [
    { choices: [{ delta: { content: text } }] },
    { choices: [{ delta: {}, finish_reason: "stop" }], usage: { prompt_tokens: 18, completion_tokens: 4 } },
  ];
}

function sendCompletion(response, chunks) {
  response.writeHead(200, { "content-type": "text/event-stream", "cache-control": "no-cache", connection: "keep-alive" });
  for (const chunk of chunks) response.write(`data: ${JSON.stringify(chunk)}\n\n`);
  response.write("data: [DONE]\n\n");
  response.end();
}

test("real Electron and Go execute the permission and question lifecycle", { timeout: 150000 }, async () => {
  const root = new URL("../..", import.meta.url).pathname;
  const artifactRoot = join(root, "tmp", "approval-question-lifecycle");
  await mkdir(artifactRoot, { recursive: true });
  const trustPath = join(artifactRoot, `trust-${"scope".repeat(8)}.txt`);
  const queuedPath = join(artifactRoot, "queued-write.txt");
  const hintPath = join(artifactRoot, "hint-denied.txt");
  const denyPath = join(artifactRoot, "direct-deny.txt");
  const oncePath = join(artifactRoot, "allow-once.txt");
  const longHintCommand = `printf 'denied' > '${hintPath}' # ${"review-scope ".repeat(8)}`;
  const commands = [
    toolChunk([
      { id: "write-trust", name: "write", arguments: { file_path: trustPath, content: "trusted" } },
      { id: "write-queued", name: "write", arguments: { file_path: queuedPath, content: "queued" } },
    ]), textChunk("Two writes complete."),
    toolChunk([{ id: "bash-hint", name: "bash", arguments: { command: longHintCommand } }]), textChunk("The hinted command was denied."),
    toolChunk([{ id: "bash-deny", name: "bash", arguments: { command: `printf 'denied' > '${denyPath}'` } }]), textChunk("The denied command did not run."),
    toolChunk([{ id: "bash-once", name: "bash", arguments: { command: `printf 'once' > '${oncePath}'` } }]), textChunk("The one-time command completed."),
    toolChunk([{ id: "ask-answer", name: "askUserQuestion", arguments: { questions: [
      { question: "Choose a mode", header: "Mode", options: [{ label: "Fast", description: "Quick execution" }] },
      { question: "Choose features", header: "Features", multiSelect: true, options: [{ label: "Tests", description: "Run tests" }, { label: "Docs", description: "Update docs" }] },
    ] } }]), textChunk("Answers received."),
    toolChunk([{ id: "ask-cancel", name: "askUserQuestion", arguments: { questions: [
      { question: "Cancel this request", header: "Cancel", options: [{ label: "Continue", description: "Keep going" }] },
    ] } }]), textChunk("The canceled question was handled."),
  ];
  let modelRequest = 0;
  const provider = createServer((request, response) => {
    request.resume();
    request.on("end", () => {
      const next = commands[modelRequest++];
      if (!next) { sendCompletion(response, textChunk("Unexpected extra model turn.")); return; }
      sendCompletion(response, next);
    });
  });
  await new Promise((resolve, reject) => provider.listen(0, "127.0.0.1", resolve).once("error", reject));
  const address = provider.address();
  assert.ok(address && typeof address === "object");
  const environment = await createEnvironment("approval-question-lifecycle", { baseURL: `http://127.0.0.1:${address.port}/v1` });
  const sessionId = "approval-lifecycle";
  await writeFile(join(environment.sessions, `${sessionId}.jsonl`), JSON.stringify({ type: "session", version: 3, id: sessionId, timestamp: "t", cwd: environment.base }) + "\n");
  const app = await startElectron(environment, { performanceTrace: false });
  let page;
  try {
    page = await attachPage(await app.devToolsURL());
    await page.waitFor("!!window.RozsaUIReady && document.querySelectorAll('#sessionList .session-item').length===1", "product UI ready");
    const initialState = await page.evaluate("window.RozsaUIHost.port.request('get_state')");
    assert.equal(initialState.sessionId, sessionId, "the fixture file name determines the selected Go session identity");
    await page.evaluate("window.__interruptionStates=[]; window.RozsaUIHost.port.subscribe('ui-state', state => window.__interruptionStates.push(state)); true");
    await page.evaluate("window.RozsaUIHost.port.request('send_message',{message:'Write two files with permission.'})");
    await page.waitFor("document.querySelector('#permPanel.visible')?.dataset.requestId", "first real Go permission");
    const firstId = await page.evaluate("document.querySelector('#permPanel').dataset.requestId");
    const pending = await page.evaluate(`window.rozsa.getInterruptionState(${JSON.stringify(sessionId)})`);
    const firstApproval = pending.approvals.find(item => item.requestId === firstId);
    assert.ok(firstApproval, `Go state did not include the visible approval: ${JSON.stringify({ sessionId, firstId, pending, active: await page.evaluate("window.RozsaUIHost.port.request('get_state')") })}`);
    assert.equal(firstApproval.status, "pending");
    assert.ok(pending.approvals.some(item => item.status === "queued"), "second concurrent Go approval should wait in the session queue");
    const badTrust = await page.evaluate(`window.rozsa.respondPermission({sessionId:${JSON.stringify(sessionId)},requestId:${JSON.stringify(firstId)},choice:'allow-session',trustKeys:['Write:invalid']}).then(()=>false,()=>true)`);
    assert.equal(badTrust, true, "Go must reject a stale trust level without removing the request");
    assert.equal(await page.evaluate("document.querySelector('#permPanel')?.dataset.requestId"), firstId);
    await page.evaluate("document.querySelector('[data-permission-action=\"trust\"]').click()");
    try {
      await page.waitFor("!!document.querySelector('#permTrustActions')", "trust-level panel", 5000);
    } catch (error) {
      const diagnostic = await page.evaluate("({ panel: document.querySelector('#permPanel')?.outerHTML, active: document.activeElement?.outerHTML })");
      throw new Error(`${error instanceof Error ? error.message : String(error)}; after T: ${JSON.stringify(diagnostic)}`);
    }
    await page.evaluate("document.querySelector('[data-permission-action=\"trust-level\"]').click()");
    try {
      await page.waitFor("document.querySelector('#permPanel.visible')?.dataset.requestId && document.querySelector('#permPanel.visible').dataset.requestId !== " + JSON.stringify(firstId), "queued Go approval promoted", 5000);
    } catch (error) {
      const state = await page.evaluate(`window.rozsa.getInterruptionState(${JSON.stringify(sessionId)}).then(value => ({ approvals: value.approvals.map(item => ({ requestId: item.requestId, status: item.status })), panel: document.querySelector('#permPanel')?.textContent }))`);
      throw new Error(`${error instanceof Error ? error.message : String(error)}; after trust: ${JSON.stringify(state)}`);
    }
    const secondId = await page.evaluate("document.querySelector('#permPanel').dataset.requestId");
    await page.evaluate("document.querySelector('[data-permission-action=\"allow\"]').click()");
    await page.waitFor("!document.querySelector('#permPanel.visible')", "allow once confirmation");
    await page.waitFor("window.__interruptionStates.some(state=>state.sessionId===" + JSON.stringify(sessionId) + "&&!state.isStreaming)", "first agent turn complete");
    assert.equal(await readFile(trustPath, "utf8"), "trusted");
    assert.equal(await readFile(queuedPath, "utf8"), "queued");
    assert.ok(secondId && modelRequest === 2);

    await page.evaluate("window.__interruptionStates=[]; window.RozsaUIHost.port.request('send_message',{message:'Show a long command and deny it with a hint.'})");
    await page.waitFor("document.querySelector('#permPanel.visible') && document.querySelector('#permTool')?.textContent==='bash'", "Bash approval");
    await page.waitFor("!!document.querySelector('[data-command-toggle]') && !document.querySelector('[data-command-toggle]').hidden", "long command control");
    await page.evaluate("document.querySelector('[data-command-toggle]').click()");
    assert.ok(await page.evaluate("document.querySelector('#permCmd').classList.contains('expanded')"));
    await page.evaluate("document.querySelector('[data-permission-action=\"hint\"]').click()");
    await page.waitFor("!!document.querySelector('[data-permission-hint]')", "hint input");
    await page.evaluate(`document.querySelector('[data-permission-hint]').focus()`);
    await page.send("Input.insertText", { text: "Use Write after checking the target." });
    assert.ok(await page.evaluate("document.querySelector('[data-permission-hint]').value==='Deny, Use Write after checking the target.'"));
    await page.evaluate("document.querySelector('[data-permission-action=\"submit-hint\"]').click()");
    await page.waitFor("!document.querySelector('#permPanel.visible')", "hint response confirmed");
    await page.waitFor("window.__interruptionStates.some(state=>state.sessionId===" + JSON.stringify(sessionId) + "&&!state.isStreaming)", "hinted agent turn complete");
    await assert.rejects(readFile(hintPath, "utf8"));

    await page.evaluate("window.__interruptionStates=[]; window.RozsaUIHost.port.request('send_message',{message:'Show a command and deny it directly.'})");
    await page.waitFor("document.querySelector('#permPanel.visible') && document.querySelector('#permTool')?.textContent==='bash'", "direct-deny approval");
    await page.evaluate("document.querySelector('[data-permission-action=\"deny\"]').click()");
    await page.waitFor("!document.querySelector('#permPanel.visible')", "direct deny confirmation");
    await page.waitFor("window.__interruptionStates.some(state=>state.sessionId===" + JSON.stringify(sessionId) + "&&!state.isStreaming)", "denied agent turn complete");
    await assert.rejects(readFile(denyPath, "utf8"));

    await page.evaluate("window.__interruptionStates=[]; window.RozsaUIHost.port.request('send_message',{message:'Run the one-time approved command.'})");
    await page.waitFor("document.querySelector('#permPanel.visible')?.dataset.requestId", "allow-once approval");
    await page.evaluate("document.querySelector('[data-permission-action=\"allow\"]').click()");
    await page.waitFor("!document.querySelector('#permPanel.visible')", "one-time response confirmation");
    await page.waitFor("window.__interruptionStates.some(state=>state.sessionId===" + JSON.stringify(sessionId) + "&&!state.isStreaming)", "one-time command complete");
    assert.equal(await readFile(oncePath, "utf8"), "once");

    await page.evaluate("window.__interruptionStates=[]; window.RozsaUIHost.port.request('send_message',{message:'Ask me two choices.'})");
    await page.waitFor("!!document.querySelector('#questionPanel')", "real Go question request");
    await page.evaluate(`document.querySelector('#questionPanelOptions input[data-other="true"]').click()`);
    await page.evaluate("document.querySelector('[data-question-other]').focus()");
    await page.send("Input.insertText", { text: "Custom mode" });
    await page.evaluate("document.querySelector('#questionPanelSubmit').click()");
    await page.waitFor("document.querySelector('#questionPanelTitle')?.textContent.includes('Choose features')", "Next multi-select question");
    await page.evaluate("document.querySelector('#questionPanelOptions input[value=\\\"Tests\\\"]').click()");
    await page.evaluate(`document.querySelector('#questionPanelOptions input[data-other="true"]').click()`);
    await page.evaluate("document.querySelector('[data-question-other]').focus()");
    await page.send("Input.insertText", { text: "Review" });
    const answeredId = await page.evaluate("document.querySelector('#questionPanel').dataset.requestId");
    await page.evaluate("document.querySelector('#questionPanelSubmit').click()");
    await page.waitFor("!document.querySelector('#questionPanel.visible')", "question answers confirmed");
    await page.waitFor("window.__interruptionStates.some(state=>state.sessionId===" + JSON.stringify(sessionId) + "&&!state.isStreaming)", "question agent turn complete");
    assert.equal(await page.evaluate(`window.rozsa.respondUserQuestion(${JSON.stringify(answeredId)},{Mode:'late',Features:[]}).then(()=>false,()=>true)`), true,
      "Go must reject an answer delivered after successful response");

    await page.evaluate("window.__interruptionStates=[]; window.RozsaUIHost.port.request('send_message',{message:'Ask a question I will cancel.'})");
    await page.waitFor("!!document.querySelector('#questionPanel')", "cancelable Go question");
    const cancelledId = await page.evaluate("document.querySelector('#questionPanel').dataset.requestId");
    await page.evaluate("document.querySelector('[data-permission-action=\"cancel-question\"]').click()");
    await page.waitFor("!document.querySelector('#questionPanel.visible')", "question cancellation acknowledged");
    const lateAnswerRejected = await page.evaluate(`window.rozsa.respondUserQuestion(${JSON.stringify(cancelledId)},{Cancel:'late'}).then(()=>false,()=>true)`);
    assert.equal(lateAnswerRejected, true, "Go must reject an answer delivered after cancellation");
    await page.waitFor("window.__interruptionStates.some(state=>state.sessionId===" + JSON.stringify(sessionId) + "&&!state.isStreaming)", "canceled question agent turn complete");
    assert.equal(modelRequest, 12, "each tool turn must cross the actual Go provider loop");
    assert.equal(page.failures.length, 0, JSON.stringify(page.failures.slice(0, 3).map(item => item.exceptionDetails?.exception?.description || item)));
  } finally {
    page?.socket.close();
    await app.stop();
    await new Promise(resolve => provider.close(resolve));
  }
});
