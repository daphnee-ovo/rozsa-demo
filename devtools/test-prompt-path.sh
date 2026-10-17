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
  if (request.url !== '/v1/messages' || request.method !== 'POST') {
    result.writeHead(404).end('not found');
    return;
  }
  let body = '';
  request.setEncoding('utf8');
  request.on('data', chunk => body += chunk);
  request.on('end', () => {
    fs.appendFileSync(countFile, `${JSON.stringify({body, xApiKey: request.headers['x-api-key'], authorization: request.headers.authorization})}\n`);
    if (body.includes('trigger stream failure')) {
      result.writeHead(503, {'content-type': 'application/json'});
      result.end('{"error":{"message":"fake provider unavailable"}}');
      return;
    }
    result.writeHead(200, {'content-type': 'text/event-stream'});
    result.end(response);
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
mkdir -p "$run_dir/unsupported/global/models" "$run_dir/unsupported/project"
cat > "$run_dir/unsupported/global/settings.json" <<'JSON'
{"defaultProvider":"custom-openai","defaultModel":"unsupported-model"}
JSON
cat > "$run_dir/unsupported/global/models/custom.json" <<'JSON'
{"providers":{"custom-openai":{"api":"openai-completions","baseUrl":"http://127.0.0.1:1","apiKey":"fake","models":[{"id":"unsupported-model"}]}}}
JSON
if ROZSA_CONFIG_DIR="$run_dir/unsupported/global" ROZSA_PROJECT_CONFIG_DIR="$run_dir/unsupported/project" go -C "$repo_root/backend" run ./cmd/rozsa --session-dir "$run_dir/unsupported/sessions" "unsupported model" > "$run_dir/unsupported.stdout" 2> "$run_dir/unsupported.stderr"; then
  echo "unimplemented provider API unexpectedly succeeded" >&2
  exit 1
fi
rg -q 'unsupported model API.*openai-completions' "$run_dir/unsupported.stderr"
echo "prompt path passed; isolated evidence: $run_dir"
