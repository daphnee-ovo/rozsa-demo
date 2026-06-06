# OAuth Architecture

Rust-owned OAuth login and credential management for `rozsa-model`.

Related code:

- [`crates/rozsa-model/src/oauth/`](../../crates/rozsa-model/src/oauth/): Rust OAuth module
- [`crates/rozsa-model/src/credentials.rs`](../../crates/rozsa-model/src/credentials.rs): Credential storage and token refresh
- [`crates/rozsa-model/src/protocol.rs`](../../crates/rozsa-model/src/protocol.rs): Bridge protocol (OAuth message types)
- [`packages/agent/src/rozsa-model-client.ts`](../../packages/agent/src/rozsa-model-client.ts): TS-side bridge client (`oauthLoginRustModel`)
- [`packages/coding-agent/src/core/auth-storage.ts`](../../packages/coding-agent/src/core/auth-storage.ts): TS-side auth orchestrator

Related docs:

- [`rozsa-model-migration.md`](./rozsa-model-migration.md): Overall migration plan and status

## Overview

Built-in OAuth providers (Anthropic, GitHub Copilot, OpenAI Codex) execute their login flows entirely in Rust. The TS layer acts as a thin UI relay — forwarding events to the login dialog and user responses back to Rust.

Extension-defined OAuth providers continue to run their JS login callbacks in the TS process (the pi-ai OAuth registry is preserved for this purpose).

## Bridge Protocol

Three new message types extend the JSONL bridge:

### Input (TS → Rust)

```jsonc
// Initiate login
{"type": "oauth_login", "id": "...", "provider": "anthropic", "options": {"authJsonPath": "..."}}

// Relay user input back to a pending prompt/select
{"type": "oauth_response", "id": "...", "response": {"value": "user-input"}}

// Cancel a login in progress (reuses existing cancel)
{"type": "cancel", "id": "..."}
```

### Output (Rust → TS)

```jsonc
// Open browser for authorization
{"type": "oauth_event", "id": "...", "event": {"type": "auth_url", "url": "...", "instructions": "..."}}

// Show device code (GitHub Copilot)
{"type": "oauth_event", "id": "...", "event": {"type": "device_code", "userCode": "...", "verificationUri": "..."}}

// Request text input
{"type": "oauth_event", "id": "...", "event": {"type": "prompt", "message": "...", "placeholder": "..."}}

// Request selection
{"type": "oauth_event", "id": "...", "event": {"type": "select", "message": "...", "options": ["..."]}}

// Progress / waiting
{"type": "oauth_event", "id": "...", "event": {"type": "progress", "message": "..."}}
{"type": "oauth_event", "id": "...", "event": {"type": "waiting", "message": "..."}}

// Login succeeded
{"type": "oauth_event", "id": "...", "event": {"type": "complete", "credentials": {"access": "...", "refresh": "...", "expires": 123}}}

// Login failed
{"type": "oauth_event", "id": "...", "event": {"type": "error", "message": "..."}}

// Extension provider — TS should handle login locally
{"type": "oauth_event", "id": "...", "event": {"type": "delegate"}}
```

## Provider Flows

### Anthropic — Authorization Code + PKCE

1. Rust generates PKCE verifier/challenge + random state
2. Rust sends `auth_url` event (authorization URL at `claude.ai/oauth/authorize`)
3. TS opens browser + shows manual paste option
4. Rust starts local HTTP server on port `53692`
5. Race: callback server receives redirect OR user pastes code via `oauth_response`
6. Rust exchanges code at `platform.claude.com/v1/oauth/token`
7. Credentials stored to `auth.json`

### GitHub Copilot — Device Code (RFC 8628)

1. Rust sends `prompt` event asking for enterprise domain
2. User responds (or empty → `github.com`)
3. Rust requests device code from `{domain}/login/device/code`
4. Rust sends `device_code` event (shows user code + verification URI)
5. Rust polls `{domain}/login/oauth/access_token` per RFC 8628
6. On success, exchanges GitHub token for Copilot token via `api.{domain}/copilot_internal/v2/token`
7. Enables known models (best effort)
8. Credentials stored to `auth.json`

### OpenAI Codex — Authorization Code + PKCE

1. Rust generates PKCE verifier/challenge + random state
2. Rust sends `auth_url` event (authorization URL at `auth.openai.com/oauth/authorize`)
3. TS opens browser + shows manual paste option
4. Rust starts local HTTP server on port `1455`
5. Race: callback server receives redirect OR user pastes code
6. Rust exchanges code at `auth.openai.com/oauth/token`
7. Extracts `accountId` from JWT payload
8. Credentials stored to `auth.json`

## Credential Storage

`auth.json` format (unchanged from TS era):

```json
{
  "anthropic": {
    "type": "oauth",
    "access": "...",
    "refresh": "...",
    "expires": 1234567890000
  },
  "github-copilot": {
    "type": "oauth",
    "access": "...",
    "refresh": "...",
    "expires": 1234567890000,
    "enterpriseUrl": "github.example.com"
  }
}
```

File locking: `{auth.json}.lock` with atomic `create_new()` — compatible with the existing TS-side `proper-lockfile` semantics.

## Token Refresh

Handled by `credentials.rs` (pre-existing, not part of the OAuth migration):

| Provider | Endpoint | Method |
|----------|----------|--------|
| Anthropic | `platform.claude.com/v1/oauth/token` | POST JSON (refresh_token grant) |
| GitHub Copilot | `api.{domain}/copilot_internal/v2/token` | GET with Bearer header |
| OpenAI Codex | `auth.openai.com/oauth/token` | POST form (refresh_token grant) |

Refresh is triggered automatically when `resolve_request_options()` detects an expired token before model execution.

## Extension OAuth Providers

Extensions register via:

```typescript
pi.registerProvider("my-provider", {
  oauth: {
    name: "My Provider",
    login(callbacks) { /* JS implementation */ },
    refreshToken(credentials) { /* JS implementation */ },
    getApiKey(credentials) { return credentials.access; },
  }
});
```

When Rust receives an `oauth_login` for a non-built-in provider, it returns `{"type": "delegate"}` and TS runs the JS login callback path unchanged.

## Module Structure

```
crates/rozsa-model/src/oauth/
├── mod.rs              — Module declarations
├── types.rs            — OAuthCredentials, OAuthFlowEvent, OAuthLoginError
├── pkce.rs             — PKCE verifier/challenge (SHA256 + base64url)
├── callback_server.rs  — Minimal HTTP server for OAuth redirects
├── device_code.rs      — RFC 8628 polling with slow_down/timeout/cancel
├── anthropic.rs        — Anthropic login implementation
├── github_copilot.rs   — GitHub Copilot login implementation
└── openai_codex.rs     — OpenAI Codex login implementation
```
