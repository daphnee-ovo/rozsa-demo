// Create synthetic desktop debugging sessions without replacing existing history.
import { mkdir, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";

function session(id, title, texts) {
  const timestamp = "2026-10-19T00:00:00Z";
  const lines = [{ type: "session", version: 3, id, timestamp, cwd: "/synthetic/rozsa-debug" }];
  let parentId = null;
  const append = (message) => {
    const entryId = `${id}-${lines.length}`;
    lines.push({ type: "message", id: entryId, parentId, timestamp, message });
    parentId = entryId;
  };
  texts.forEach((text, index) => {
    append({ role: "user", content: [{ type: "text", text: index === 0 ? title : `继续检查示例 ${index + 1}` }] });
    const toolCallId = `${id}-read-${index}`;
    append({ role: "assistant", api: "openai-completions", provider: "synthetic", model: "debug-fixture", stopReason: "stop", content: [
      { type: "thinking", thinking: "这是合成调试数据，不会执行真实命令。" },
      { type: "text", text },
      ...(index === 0 ? [{ type: "toolCall", id: toolCallId, name: "read", arguments: { path: "/synthetic/example.ts" } }] : []),
    ] });
    if (index === 0) append({ role: "toolResult", toolCallId, toolName: "read", content: [{ type: "text", text: "export const greeting = 'Hello, Rózsa';" }] });
  });
  return lines.map(line => JSON.stringify(line)).join("\n") + "\n";
}

export async function prepareDebugSessions() {
  const configDirectory = process.env.ROZSA_CONFIG_DIR;
  if (!configDirectory) {
    throw new Error("ROZSA_CONFIG_DIR must be set before preparing debug sessions");
  }

  const projectDirectory = resolve(process.cwd());
  const encodedProjectDirectory = projectDirectory
    .replace(/[\\/]/g, "-")
    .replace(/^-+|-+$/g, "");
  const directory = resolve(configDirectory, "sessions", `-${encodedProjectDirectory}-`);
  await mkdir(directory, { recursive: true });
  const fixtures = [
    ["welcome", "示例：Markdown 与代码", ["# 调试示例\n\n这里包含 **粗体**、*斜体*、[链接](https://example.com) 与 `inline code`。\n\n- 普通消息\n- 可展开的 Thinking 和工具调用\n\n| 项目 | 状态 |\n| --- | --- |\n| Markdown | 可检查 |\n\n```typescript\nexport function greet(name: string) {\n  return `Hello, ${name}`;\n}\n```"]],
    ["diagrams", "示例：Mermaid 图表", ["## 图表操作\n\n悬停图表可检查源码切换、缩放与应用内查看页。\n\n```mermaid\nflowchart LR\n  A[读取需求] --> B[执行任务]\n  B --> C{验证结果}\n  C -->|通过| D[交付]\n  C -->|失败| B\n```\n\n```mermaid\nsequenceDiagram\n  User->>App: 打开示例会话\n  App->>Backend: 读取历史\n  Backend-->>App: 返回消息\n  App-->>User: 渲染 Markdown\n```"]],
    ["long-history", "示例：长历史滚动", Array.from({ length: 80 }, (_, index) => `## 第 ${index + 1} 轮\n\n检查虚拟列表滚动、切换会话后恢复位置，以及输入框响应。\n\n${"这是一段用于调试布局与文本换行的合成内容。".repeat(8)}\n\n\`\`\`javascript\nconst round = ${index + 1};\nconsole.log(round);\n\`\`\``)],
  ];
  for (const [id, title, texts] of fixtures) {
    try { await writeFile(resolve(directory, `${id}.jsonl`), session(id, title, texts), { flag: "wx" }); }
    catch (error) { if (error.code !== "EEXIST") throw error; }
  }
  return directory;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  console.log(await prepareDebugSessions());
}
