#!/usr/bin/env npx tsx
/**
 * Smoke test: Rust anthropic-messages backend → fake FastAPI server.
 * Requires: fake-anthropic-server.py running on port 19090.
 */
import { streamSimpleRustModel } from "../packages/ai/src/providers/rozsa-model-bridge.ts";

const model = {
  id: "claude-sonnet-4-20250514",
  name: "Claude Sonnet 4",
  api: "anthropic-messages" as const,
  provider: "fireworks",
  baseUrl: "http://127.0.0.1:19090",
  reasoning: false,
  input: ["text"] as string[],
  cost: { input: 3, output: 15, cacheRead: 0.3, cacheWrite: 3.75 },
  contextWindow: 200000,
  maxTokens: 8192,
  compat: {
    supportsEagerToolInputStreaming: false,
    supportsLongCacheRetention: false,
    sendSessionAffinityHeaders: true,
    supportsCacheControlOnTools: false,
    forceAdaptiveThinking: false,
  },
};

const context = {
  systemPrompt: "Be concise.",
  messages: [{ role: "user" as const, content: "Say hello", timestamp: 1 }],
  tools: [{
    name: "lookup",
    description: "Lookup a value",
    parameters: { type: "object", properties: { key: { type: "string" } }, required: ["key"] },
  }],
};

const options = { apiKey: "fw-test-key", maxTokens: 64, sessionId: "sess-001" };

console.log("[smoke] Streaming via Rust backend → http://127.0.0.1:19090/v1/messages\n");

const stream = streamSimpleRustModel(model, context, options);
for await (const event of stream) {
  const e = event as Record<string, unknown>;
  console.log("EVENT:", e.type, e.delta ?? e.content ?? "");
}

const result = await stream.result();
console.log("\n--- RESULT ---");
console.log("stopReason:", result.stopReason);
console.log("content:", JSON.stringify(result.content, null, 2));
console.log("usage:", JSON.stringify(result.usage, null, 2));

const ok = result.stopReason !== "error" && result.content.length > 0;
console.log("\n", ok ? "✓ PASS" : "✗ FAIL");
process.exit(ok ? 0 : 1);
