#!/usr/bin/env python3
"""
Fake Anthropic Messages API server for smoke testing the Rust backend.

Usage:
    uv run devtools/fake-anthropic-server.py [--port 19090]

Then point models.json baseUrl at http://127.0.0.1:19090 and run:
    ROZSA_MODEL_BACKEND=rust ROZSA_MODEL_RUST_APIS=anthropic-messages <your test>

Behavior:
- Default: streams a long multi-chunk text response
- User message contains "use tool" / "call tool": returns tool_use block
- Thinking enabled in request: prepends a thinking block
"""
# /// script
# requires-python = ">=3.11"
# dependencies = ["fastapi>=0.110", "uvicorn>=0.27"]
# ///

import asyncio
import json
import sys
import argparse
from typing import Any

from fastapi import FastAPI, Request
from fastapi.responses import StreamingResponse

app = FastAPI()

request_log: list[dict[str, Any]] = []

LONG_TEXT_CHUNKS = [
    "Let me help you with that.\n\n",
    "Here's a detailed explanation of how the system works:\n\n",
    "1. The client sends an HTTP POST request to the `/v1/messages` endpoint ",
    "with a JSON payload containing the model ID, messages array, and configuration.\n\n",
    "2. The server processes the request and begins streaming Server-Sent Events (SSE) back. ",
    "Each event contains a type field and associated data.\n\n",
    "3. The stream starts with `message_start` which includes the message ID and initial usage metrics. ",
    "Then `content_block_start` signals the beginning of each content block — ",
    "these can be text, thinking, or tool_use blocks.\n\n",
    "4. As content is generated, `content_block_delta` events carry incremental updates: ",
    "`text_delta` for text content, `thinking_delta` for reasoning, ",
    "and `input_json_delta` for tool call arguments being built up character by character.\n\n",
    "5. Each block ends with `content_block_stop`, and the full message concludes with ",
    "`message_delta` (carrying the stop reason and final usage) followed by `message_stop`.\n\n",
    "This streaming protocol allows the client to progressively render content ",
    "as it arrives, providing a responsive user experience even for long responses.",
]


@app.post("/v1/messages")
async def messages(request: Request):
    body = await request.json()
    headers = dict(request.headers)
    request_log.append({"headers": headers, "body": body})

    print(f"[fake-anthropic] POST /v1/messages model={body.get('model')}", file=sys.stderr)

    # Extract last user message text
    user_msgs = [m for m in body.get("messages", []) if m.get("role") == "user"]
    last_user_text = ""
    if user_msgs:
        content = user_msgs[-1].get("content", "")
        if isinstance(content, str):
            last_user_text = content
        elif isinstance(content, list):
            last_user_text = " ".join(
                b.get("text", "") for b in content if b.get("type") == "text"
            )

    # Determine response mode
    lower_text = last_user_text.lower()
    has_tools = bool(body.get("tools")) and (
        "use tool" in lower_text or "call tool" in lower_text
    )
    has_thinking = body.get("thinking", {}).get("type") in ("enabled", "adaptive")

    async def generate():
        # message_start
        yield sse_event("message_start", {
            "type": "message_start",
            "message": {
                "id": "msg_fake_001",
                "type": "message",
                "role": "assistant",
                "content": [],
                "model": body.get("model", "fake-model"),
                "usage": {
                    "input_tokens": 42,
                    "output_tokens": 0,
                    "cache_read_input_tokens": 10,
                    "cache_creation_input_tokens": 5,
                },
            },
        })
        await asyncio.sleep(0)

        block_idx = 0

        # thinking block (if enabled)
        if has_thinking:
            yield sse_event("content_block_start", {
                "type": "content_block_start",
                "index": block_idx,
                "content_block": {"type": "thinking", "thinking": ""},
            })
            await asyncio.sleep(0)
            thinking_chunks = [
                "Let me analyze this step by step.\n",
                "First, I need to understand what the user is asking for.\n",
                "They want to verify that streaming works correctly.\n",
                "I'll provide a comprehensive response to demonstrate the streaming behavior.",
            ]
            for chunk in thinking_chunks:
                yield sse_event("content_block_delta", {
                    "type": "content_block_delta",
                    "index": block_idx,
                    "delta": {"type": "thinking_delta", "thinking": chunk},
                })
                await asyncio.sleep(0.05)
            yield sse_event("content_block_delta", {
                "type": "content_block_delta",
                "index": block_idx,
                "delta": {"type": "signature_delta", "signature": "sig_fake_thinking_001"},
            })
            await asyncio.sleep(0)
            yield sse_event("content_block_stop", {"type": "content_block_stop", "index": block_idx})
            await asyncio.sleep(0)
            block_idx += 1

        # text block — stream in multiple chunks with delay
        yield sse_event("content_block_start", {
            "type": "content_block_start",
            "index": block_idx,
            "content_block": {"type": "text", "text": ""},
        })
        await asyncio.sleep(0)
        for chunk in LONG_TEXT_CHUNKS:
            yield sse_event("content_block_delta", {
                "type": "content_block_delta",
                "index": block_idx,
                "delta": {"type": "text_delta", "text": chunk},
            })
            await asyncio.sleep(0.05)
        yield sse_event("content_block_stop", {"type": "content_block_stop", "index": block_idx})
        await asyncio.sleep(0)
        block_idx += 1

        # tool_use block (triggered by "use tool" / "call tool" in user message)
        stop_reason = "end_turn"
        if has_tools:
            tool = body["tools"][0]
            tool_name = tool["name"]
            tool_args = build_tool_args(tool)

            yield sse_event("content_block_start", {
                "type": "content_block_start",
                "index": block_idx,
                "content_block": {
                    "type": "tool_use",
                    "id": f"toolu_fake_{block_idx:03d}",
                    "name": tool_name,
                    "input": {},
                },
            })
            await asyncio.sleep(0)
            # Stream tool arguments in multiple partial JSON chunks
            args_json = json.dumps(tool_args)
            chunk_size = max(10, len(args_json) // 4)
            for i in range(0, len(args_json), chunk_size):
                yield sse_event("content_block_delta", {
                    "type": "content_block_delta",
                    "index": block_idx,
                    "delta": {"type": "input_json_delta", "partial_json": args_json[i:i + chunk_size]},
                })
                await asyncio.sleep(0.03)
            yield sse_event("content_block_stop", {"type": "content_block_stop", "index": block_idx})
            await asyncio.sleep(0)
            block_idx += 1
            stop_reason = "tool_use"

        # message_delta with final usage
        total_output = len("".join(LONG_TEXT_CHUNKS).split()) + (50 if has_thinking else 0)
        yield sse_event("message_delta", {
            "type": "message_delta",
            "delta": {"stop_reason": stop_reason},
            "usage": {"output_tokens": total_output},
        })
        await asyncio.sleep(0)

        # message_stop
        yield sse_event("message_stop", {"type": "message_stop"})

    return StreamingResponse(generate(), media_type="text/event-stream")


@app.get("/requests")
async def get_requests():
    return request_log


@app.delete("/requests")
async def clear_requests():
    request_log.clear()
    return {"cleared": True}


def build_tool_args(tool: dict) -> dict:
    """Build realistic arguments from tool schema properties."""
    props = tool.get("input_schema", {}).get("properties", {})
    if not props:
        props = tool.get("parameters", {}).get("properties", {})
    args = {}
    for key, schema in props.items():
        prop_type = schema.get("type", "string")
        if prop_type == "string":
            args[key] = f"/home/user/project/src/main.rs"
        elif prop_type == "number" or prop_type == "integer":
            args[key] = 42
        elif prop_type == "boolean":
            args[key] = True
        elif prop_type == "array":
            args[key] = ["item1", "item2"]
        else:
            args[key] = f"fake_{key}_value"
    if not args:
        args = {"path": "/home/user/project/README.md"}
    return args


def sse_event(event_type: str, data: dict) -> str:
    return f"event: {event_type}\ndata: {json.dumps(data)}\n\n"


if __name__ == "__main__":
    import uvicorn

    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, default=19090)
    args = parser.parse_args()

    print(f"[fake-anthropic] Starting on http://127.0.0.1:{args.port}", file=sys.stderr)
    print(f"[fake-anthropic] Trigger tool call: include 'use tool' or 'call tool' in message", file=sys.stderr)
    uvicorn.run(app, host="127.0.0.1", port=args.port, log_level="warning")
