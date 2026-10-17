#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
run_dir="$repo_root/tmp/test-prompt-path-$$"
mkdir -p "$run_dir/sessions"

cat > "$run_dir/fake-provider.js" <<'NODE'
const http = require('node:http');
const fs = require('node:fs');
const path = require('node:path');
const portFile = process.argv[2];
const countFile = process.argv[3];
const response = [
  'event: message_start\ndata: {"message":{"id":"msg_fake","usage":{"input_tokens":1,"output_tokens":1}}}\n\n',
  'event: content_block_start\ndata: {"index":0,"content_block":{"type":"text","text":""}}\n\n',
  'event: content_block_delta\ndata: {"index":0,"delta":{"type":"text_delta","text":"fake reply"}}\n\n',
  'event: content_block_stop\ndata: {"index":0}\n\n',
  'event: message_delta\ndata: {"delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":2}}\n\n',
  'event: message_stop\ndata: {}\n\n'
].join('');
const server = http.createServer((request, result) => {
  if (!['/v1/messages', '/v1/chat/completions'].includes(request.url) || request.method !== 'POST') {
    result.writeHead(404).end('not found');
    return;
  }
  let body = '';
  request.setEncoding('utf8');
  request.on('data', chunk => body += chunk);
  request.on('end', () => {
    fs.appendFileSync(countFile, `${JSON.stringify({url: request.url, body, xApiKey: request.headers['x-api-key'], authorization: request.headers.authorization})}\n`);
    if (body.includes('trigger stream failure')) {
      result.writeHead(503, {'content-type': 'application/json'});
      result.end('{"error":{"message":"fake provider unavailable"}}');
      return;
    }
    if (request.url === '/v1/chat/completions' && body.includes('trigger completions failure')) {
      result.writeHead(503, {'content-type': 'application/json', 'retry-after': '0'});
      result.end('{"error":{"message":"fake completions unavailable"}}');
      return;
    }
    result.writeHead(200, {'content-type': 'text/event-stream'});
    if (request.url === '/v1/chat/completions') {
      result.end('data: {"id":"chat_fake","choices":[{"delta":{"content":"openai reply"}}]}\n\ndata: {"choices":[{"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":2,"completion_tokens":2}}\n\ndata: [DONE]\n\n');
    } else {
      result.end(response);
    }
  });
});
server.listen(0, '127.0.0.1', () => fs.writeFileSync(portFile, String(server.address().port)));
NODE

port_file="$run_dir/port"
count_file="$run_dir/requests.jsonl"
node "$run_dir/fake-provider.js" "$port_file" "$count_file" &
provider_pid=$!
trap 'kill "$provider_pid" 2>/dev/null || true; wait "$provider_pid" 2>/dev/null || true' EXIT
for _ in $(seq 1 100); do
  [[ -s "$port_file" ]] && break
  sleep 0.05
done
if [[ ! -s "$port_file" ]]; then
  echo "fake provider did not start" >&2
  exit 1
fi
port="$(cat "$port_file")"
mkdir -p "$run_dir/config/global/models" "$run_dir/config/project"
cat > "$run_dir/config/global/settings.json" <<'JSON'
{"defaultProvider":"custom-anthropic","defaultModel":"custom-model"}
JSON
cat > "$run_dir/config/global/models/custom.json" <<JSON
{"providers":{"custom-anthropic":{"api":"anthropic-messages","baseUrl":"http://127.0.0.1:$port","apiKey":"fake","models":[{"id":"custom-model","name":"Custom test model"}]}}}
JSON
rozsa_cli() { ROZSA_CONFIG_DIR="$run_dir/config/global" ROZSA_PROJECT_CONFIG_DIR="$run_dir/config/project" go -C "$repo_root/backend" run ./cmd/rozsa "$@"; }
rozsa_cli --session-dir "$run_dir/sessions" "remember this prompt" > "$run_dir/prompt.stdout" 2> "$run_dir/prompt.stderr"
[[ "$(cat "$run_dir/prompt.stdout")" == "fake reply" ]]
[[ ! -s "$run_dir/prompt.stderr" ]]
session_file="$(find "$run_dir/sessions" -name '*.jsonl' -print -quit)"
[[ -n "$session_file" ]]
before="$(wc -c < "$session_file" | tr -d ' ')"
rozsa_cli --continue --session-dir "$run_dir/sessions" > "$run_dir/continue.stdout" 2> "$run_dir/continue.stderr"
[[ ! -s "$run_dir/continue.stderr" ]]
after="$(wc -c < "$session_file" | tr -d ' ')"
[[ "$before" == "$after" ]]
[[ "$(wc -l < "$count_file" | tr -d ' ')" == "1" ]]
rg -q '"xApiKey":"fake"' "$count_file"
rg -q 'remember this prompt' "$session_file"
rg -q 'fake reply' "$session_file"
if env -u MISSING_T020_KEY ROZSA_CONFIG_DIR="$run_dir/config/global" ROZSA_PROJECT_CONFIG_DIR="$run_dir/config/project" go -C "$repo_root/backend" run ./cmd/rozsa --session-dir "$run_dir/config-failure" --api-key '$MISSING_T020_KEY' "must fail before provider" > "$run_dir/config.stdout" 2> "$run_dir/config.stderr"; then
  echo "missing configured API key unexpectedly succeeded" >&2
  exit 1
fi
rg -q 'MISSING_T020_KEY.*not set' "$run_dir/config.stderr"
if rozsa_cli --session-dir "$run_dir/stream-failure" "trigger stream failure" > "$run_dir/failure.stdout" 2> "$run_dir/failure.stderr"; then
  echo "provider stream failure unexpectedly succeeded" >&2
  exit 1
fi
rg -q '503' "$run_dir/failure.stderr"
rg -q 'fake provider unavailable' "$(find "$run_dir/stream-failure" -name '*.jsonl' -print -quit)"
if rozsa_cli --continue --session-dir "$run_dir/empty-continue" > "$run_dir/empty.stdout" 2> "$run_dir/empty.stderr"; then
  echo "continue without an existing session unexpectedly succeeded" >&2
  exit 1
fi
rg -q 'No previous session found to continue' "$run_dir/empty.stderr"
[[ -z "$(find "$run_dir/empty-continue" -name '*.jsonl' -print -quit)" ]]
mkdir -p "$run_dir/completions/global/models" "$run_dir/completions/project"
cat > "$run_dir/completions/global/settings.json" <<'JSON'
{"defaultProvider":"custom-openai-compatible","defaultModel":"completions-model"}
JSON
cat > "$run_dir/completions/global/models/custom.json" <<JSON
{"providers":{"custom-openai-compatible":{"api":"openai-completions","baseUrl":"http://127.0.0.1:$port/v1","apiKey":"fake","models":[{"id":"completions-model","name":"Custom Completions model"}]}}}
JSON
ROZSA_CONFIG_DIR="$run_dir/completions/global" ROZSA_PROJECT_CONFIG_DIR="$run_dir/completions/project" go -C "$repo_root/backend" run ./cmd/rozsa --session-dir "$run_dir/completions/sessions" "completions prompt" > "$run_dir/completions.stdout" 2> "$run_dir/completions.stderr"
[[ "$(cat "$run_dir/completions.stdout")" == "openai reply" ]]
[[ ! -s "$run_dir/completions.stderr" ]]
completions_session="$(find "$run_dir/completions/sessions" -name '*.jsonl' -print -quit)"
[[ -n "$completions_session" ]]
rg -q 'completions prompt' "$completions_session"
rg -q 'openai reply' "$completions_session"
rg -q '"api":"openai-completions"' "$completions_session"
rg -q '"totalTokens":4' "$completions_session"
ROZSA_CONFIG_DIR="$run_dir/completions/global" ROZSA_PROJECT_CONFIG_DIR="$run_dir/completions/project" go -C "$repo_root/backend" run ./cmd/rozsa --continue --session-dir "$run_dir/completions/sessions" > "$run_dir/completions-continue.stdout" 2> "$run_dir/completions-continue.stderr"
[[ ! -s "$run_dir/completions-continue.stderr" ]]
[[ "$(rg -c '"url":"/v1/chat/completions"' "$count_file")" == "1" ]]
rg -q '"authorization":"Bearer fake"' "$count_file"
if ROZSA_CONFIG_DIR="$run_dir/completions/global" ROZSA_PROJECT_CONFIG_DIR="$run_dir/completions/project" go -C "$repo_root/backend" run ./cmd/rozsa --session-dir "$run_dir/completions-failure/sessions" "trigger completions failure" > "$run_dir/completions-failure.stdout" 2> "$run_dir/completions-failure.stderr"; then
  echo "Completions HTTP failure unexpectedly succeeded" >&2
  exit 1
fi
rg -q '503' "$run_dir/completions-failure.stderr"
rg -q 'fake completions unavailable' "$(find "$run_dir/completions-failure/sessions" -name '*.jsonl' -print -quit)"
mkdir -p "$run_dir/unsupported/global/models" "$run_dir/unsupported/project"
cat > "$run_dir/unsupported/global/settings.json" <<'JSON'
{"defaultProvider":"custom-responses","defaultModel":"unsupported-model"}
JSON
cat > "$run_dir/unsupported/global/models/custom.json" <<'JSON'
{"providers":{"custom-responses":{"api":"openai-responses","baseUrl":"http://127.0.0.1:1","apiKey":"fake","models":[{"id":"unsupported-model"}]}}}
JSON
if ROZSA_CONFIG_DIR="$run_dir/unsupported/global" ROZSA_PROJECT_CONFIG_DIR="$run_dir/unsupported/project" go -C "$repo_root/backend" run ./cmd/rozsa --session-dir "$run_dir/unsupported/sessions" "unsupported model" > "$run_dir/unsupported.stdout" 2> "$run_dir/unsupported.stderr"; then
  echo "unimplemented Responses API unexpectedly succeeded" >&2
  exit 1
fi
rg -q 'unsupported model API.*openai-responses' "$run_dir/unsupported.stderr"
echo "prompt path passed; isolated evidence: $run_dir"
