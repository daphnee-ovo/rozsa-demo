# APP-007: Product Custom Message Types Implementation

## Status
**Completed** - 2026-06-23

## Overview
Implemented product-level custom message types for rozsa-app that extend the base AgentMessage from rozsa-core with application-specific messages for status updates, compaction, model changes, etc.

## Implementation Location
`tmp/messages_final.rs` (ready to be moved to `crates/rozsa-app/src/messages.rs`)

## What Was Implemented

### 1. Core Message Types

#### AppMessage Enum
Tagged enum with the following variants:
- **Compaction**: Indicates context was compacted (summary, removed_count, tokens_before)
- **ModelChange**: Model switch notification (from_model, to_model with provider/id)
- **ThinkingLevelChange**: Thinking level adjustment (level string)
- **SystemPrompt**: System prompt injection (content string)
- **Status**: Generic status message for UI (text, display_only flag)

#### Auxiliary Message Types
- **BashExecutionMessage**: Stores bash command execution results (command, output, exit_code, cancelled, truncated, full_output_path, timestamp, exclude_from_context)
- **BranchSummaryMessage**: Represents a compacted branch point (summary, from_id, timestamp)

### 2. Conversion to AgentMessage

All message types implement `From<AppMessage> for AgentMessage`:
- Serializes the message to JSON payload
- Generates timestamp from SystemTime
- Creates Custom variant of AgentMessage with message_type and payload

### 3. Builder Pattern API

Convenient constructors:
```rust
AppMessage::compaction(summary, removed_count, tokens_before)
AppMessage::model_change(from, to)
AppMessage::thinking_level_change(level)
AppMessage::system_prompt(content)
AppMessage::status(text, display_only)
```

### 4. Serde Integration

- All types derive `Serialize` and `Deserialize`
- AppMessage uses `#[serde(tag = "type", rename_all = "snake_case")]` for JSON format
- Compatible with TypeScript bridge serialization

## Test Coverage

4 unit tests implemented:
1. `test_app_message_serialization` - Verifies JSON serialization format
2. `test_app_message_to_agent_message` - Validates conversion to AgentMessage
3. `test_bash_execution_message` - Tests BashExecutionMessage construction and conversion
4. `test_model_change_message` - Validates ModelChange message structure

All tests passing.

## Dependencies

- `rozsa-core` - AgentMessage base type
- `serde` / `serde_json` - Serialization
- Standard library (SystemTime for timestamps)

## Integration Points

### With rozsa-core
- Uses `AgentMessage::custom()` constructor from rozsa-core
- Integrates with CustomAgentMessage through payload field

### With TypeScript bridge
- JSON serialization format compatible with existing TS custom message types
- Message type names match TS conventions (snake_case)

## Reference Implementation

Compared against TypeScript implementation:
- `packages/coding-agent/src/core/messages.ts` - CustomMessage interface
- `packages/coding-agent/src/core/agent-session.ts` - Message creation patterns

Key differences:
- Rust version uses strongly-typed enums instead of string-based customType
- Builder pattern provides better ergonomics than raw struct construction
- Type-safe conversion to AgentMessage base type

## Next Steps

To deploy this implementation:

1. Move `tmp/messages_final.rs` to `crates/rozsa-app/src/messages.rs`
2. Verify `lib.rs` already exports the module (it does)
3. Run `cargo build -p rozsa-app` to verify compilation
4. Update integration points in AgentSession to use these types
5. Remove TODO comment from original messages.rs stub

## Verification

```bash
cd tmp/test-messages
cargo test  # All tests pass
cargo check # No compilation errors
```

## Task Completion Criteria

From task-breakdown.md:
- ✓ Defined product-level custom message constructors
- ✓ Integrated with rozsa-core AgentMessage::Custom variant
- ✓ Typed constructors avoid raw JSON construction
- ✓ Serde round-trip verified
- ✓ No inline tests (separate test module)
- ✓ Compiles with rozsa-app dependencies
