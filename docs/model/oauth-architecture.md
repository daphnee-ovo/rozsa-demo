# OAuth architecture

`rozsa-model` owns the supported OAuth implementations. GUI commands start a
flow, present its events to the user, and send required responses back to the
same Rust flow; there is no TypeScript relay or process bridge.

## Code map

- [`oauth/`](../../crates/rozsa-model/src/oauth/): provider flows, PKCE, callback server, and device-code polling
- [`oauth/types.rs`](../../crates/rozsa-model/src/oauth/types.rs): `OAuthCredentials`, `OAuthFlowEvent`, and errors
- [`credentials.rs`](../../crates/rozsa-model/src/credentials.rs): credential storage and refresh during request-option resolution
- [`commands.rs`](../../crates/rozsa-gui/src/commands.rs): GUI command boundary

## Supported flows

| Provider | Flow | User interaction |
| --- | --- | --- |
| Anthropic | Authorization Code + PKCE | Browser authorization, local callback, or manually pasted redirect URL |
| OpenAI Codex | Authorization Code + PKCE | Browser authorization, local callback, or manually pasted redirect URL |
| GitHub Copilot | Device Code | Optional enterprise domain, verification URL and user code |

All flows send `OAuthFlowEvent` values to their caller. The caller must surface
the URL, code, prompt, progress, or waiting state faithfully and respect
cancellation. Errors are returned explicitly rather than silently falling back
to another authentication mechanism.

## Credential handling

OAuth credentials are stored in `auth.json` through
`store_oauth_credentials`. `resolve_request_options` refreshes expired
credentials for the supported providers before a model request is made. The
credential file remains local user configuration and must not be logged or
committed.

## OpenAI Codex compatibility note

The OpenAI authorization URL currently retains `originator=pi` in
[`build_auth_url`](../../crates/rozsa-model/src/oauth/openai_codex.rs). It is a
login compatibility input, not an internal product protocol. Its removal or
replacement is deferred in [`docs/TODO.md`](../TODO.md): first establish an
officially accepted value and add an authorization-flow regression test.
