import { mkdir, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { deflateSync } from "node:zlib";

const signature = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);

function crc32(buffer) {
  let crc = 0xffffffff;
  for (const byte of buffer) {
    crc ^= byte;
    for (let bit = 0; bit < 8; bit++) crc = (crc >>> 1) ^ (crc & 1 ? 0xedb88320 : 0);
  }
  return (crc ^ 0xffffffff) >>> 0;
}

function pngChunk(name, data) {
  const type = Buffer.from(name);
  const length = Buffer.alloc(4);
  length.writeUInt32BE(data.length);
  const checksum = Buffer.alloc(4);
  checksum.writeUInt32BE(crc32(Buffer.concat([type, data])));
  return Buffer.concat([length, type, data, checksum]);
}

function imageData(seed = 1, width = 128, height = 96) {
  const header = Buffer.alloc(13);
  header.writeUInt32BE(width, 0);
  header.writeUInt32BE(height, 4);
  header[8] = 8;
  header[9] = 6;
  const pixels = Buffer.alloc(height * (1 + width * 4));
  for (let y = 0; y < height; y++) {
    const row = y * (1 + width * 4);
    for (let x = 0; x < width; x++) {
      const offset = row + 1 + x * 4;
      pixels[offset] = (x * 29 + seed) & 255;
      pixels[offset + 1] = (y * 41 + seed * 3) & 255;
      pixels[offset + 2] = (x * 13 + y * 17 + seed * 7) & 255;
      pixels[offset + 3] = 255;
    }
  }
  return Buffer.concat([
    signature,
    pngChunk("IHDR", header),
    pngChunk("IDAT", deflateSync(pixels)),
    pngChunk("IEND", Buffer.alloc(0)),
  ]).toString("base64");
}

function markdown(index) {
  return [
    `## Review ${index}: preserve the active branch`,
    "",
    "A long-session rendering sample with CJK text: 会话状态应保持稳定。",
    "",
    "- Keep the selected session visible.",
    "- Retain scroll position while content streams.",
    "",
    "| Stage | Expected | Result |",
    "| --- | --- | --- |",
    `| ${index} | render | stable |`,
    "",
    "```ts",
    `const session = "session-${index}";`,
    "await renderMarkdown(session);",
    "```",
    "",
    "[Local reference](https://example.invalid/performance-fixture)",
  ].join("\n");
}

export async function writeConversationFixture(directory, id, {
  pairs = 600,
  imageEvery = 8,
  imageAtEnd = false,
  toolEvery = 12,
  toolAtEnd = false,
} = {}) {
  await mkdir(directory, { recursive: true });
  const path = join(directory, `${id}.jsonl`);
  const lines = [JSON.stringify({
    type: "session", version: 3, id,
    timestamp: "2026-10-18T00:00:00Z", cwd: process.cwd(),
  })];
  let parentId = null;
  let sequence = 0;
  const image = imageData(17);
  const append = (messageId, message) => {
    lines.push(JSON.stringify({
      type: "message", id: messageId, parentId,
      timestamp: String(sequence++), message,
    }));
    parentId = messageId;
  };

  for (let index = 0; index < pairs; index++) {
    const userId = `${id}-user-${index}`;
    const userContent = [{ type: "text", text: markdown(index) }];
    if ((imageEvery > 0 && index % imageEvery === 0) || (imageAtEnd && index === pairs - 1)) {
      userContent.push({ type: "image", mimeType: "image/png", data: image });
    }
    append(userId, { role: "user", content: userContent });

    const assistantId = `${id}-assistant-${index}`;
    const content = [
      { type: "thinking", thinking: `Inspecting session ${id}, step ${index}.\n\n- Compare the current branch.\n- Preserve the pending work.` },
      { type: "text", text: `Completed analysis ${index}.\n\n${markdown(index)}` },
    ];
    const toolCallId = `${id}-tool-${index}`;
    if (toolEvery > 0 && (index % toolEvery === 0 || (toolAtEnd && index === pairs - 1))) {
      content.push({ type: "toolCall", id: toolCallId, name: "read", arguments: { path: `/synthetic/${index}.txt` } });
    }
    append(assistantId, {
      role: "assistant", content,
      api: "openai-completions", provider: "synthetic", model: "stream", stopReason: "stop",
    });

    if (toolEvery > 0 && (index % toolEvery === 0 || (toolAtEnd && index === pairs - 1))) {
      append(`${id}-tool-result-${index}`, {
        role: "toolResult", toolCallId, toolName: "read",
        content: [{ type: "text", text: `Synthetic tool output ${index}\n${markdown(index)}` }],
      });
    }
  }

  const source = `${lines.join("\n")}\n`;
  await writeFile(path, source);
  return { id, path, entries: sequence, pairs, sourceJSONLBytes: Buffer.byteLength(source), imageBase64Bytes: Buffer.byteLength(image, "base64") };
}

