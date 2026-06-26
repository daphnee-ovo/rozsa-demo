# Permissions and Runtime Sidebar

Pi checks tool calls and interactive bash commands before execution through one permission manager.

## Modes

Set the mode with CLI or settings:

```bash
pi --permission-mode on-request
pi --permission-mode auto-permission
pi --permission-mode free-permission
```

```json
{
  "permissionMode": "auto-permission",
  "autoPermissionReviewer": {
    "enabled": true,
    "provider": "anthropic",
    "model": "claude-haiku-4-5",
    "temperature": 0,
    "maxTokens": 400
  }
}
```

`on-request` is the default. Whitelist matches are approved. Blacklist matches are rejected. Other calls ask for a user decision: approve once, approve for this session, reject, or reject and ask the agent to choose another approach.

`auto-permission` applies blacklist and whitelist first. Other calls go to a reviewer model, which must return strict JSON:

```json
{
  "decision": "approve",
  "risk_level": "write",
  "is_workspace_scoped": true,
  "reason": "Workspace-scoped edit",
  "safer_alternative": ""
}
```

The reviewer rejects when it is uncertain. Auto-permission is not completely safe; it is automated review by another model.

`free-permission` approves ordinary calls without user confirmation or reviewer calls. Built-in hard blacklist rules still reject clearly destructive commands such as `rm -rf /`, `git reset --hard`, `git clean -fd`, and force push. Free-permission is risky because it accepts agent operation requests without interactive review.

## Rules

Settings can define whitelist and blacklist rules:

```json
{
  "permissions": {
    "whitelist": {
      "toolNames": ["read", "grep", "find", "ls"],
      "commandPrefixes": ["git status", "npm run check"],
      "riskLevels": ["read"]
    },
    "blacklist": {
      "commandPatterns": ["\\bsudo\\b"],
      "pathPatterns": ["(^|/)\\.git($|/)"]
    }
  }
}
```

Rules support tool exact names, tool prefixes, command exact matches, command prefixes, command regex patterns, path scopes, path patterns, and risk levels.

## Audit Log

Permission decisions are written to:

```text
.pi-agent/sessions/<session-id>.jsonl
```

Each line records timestamp, session id, mode, tool, command preview, arguments preview, risk level, affected paths, decision source, reviewer reason, user choice, and final status. Arguments and command previews are redacted for obvious secrets, tokens, keys, and credentials.

## Sidebar

Interactive mode renders a compact runtime sidebar/status panel from `RuntimeStateStore`. It does not call tool executors, model clients, git services, or subagent internals while rendering.

Fields include project, cwd, permission mode, model, thinking level, token usage, git branch and dirty count, subagent status, changed files, and tool call counts. Non-git directories show git as disabled. Missing subagents show idle.

## Known Limits

Token reasoning counts depend on provider usage data. Providers that do not expose reasoning tokens show `0`.

File additions and deletions come from `git diff --numstat` when available. Outside git repositories, changed files remain visible with unknown line counts.

Reviewer safety depends on the configured model and prompt compliance. Invalid JSON, unsafe scope, or uncertainty is treated as rejection.
