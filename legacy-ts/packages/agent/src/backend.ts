/**
 * Backend abstraction for the agent loop execution.
 *
 * Internal Framework:
 * backend.ts
 * ├── AgentLoopBackend            # interface: runPrompt, runContinue
 * └── TsAgentLoopBackend          # wraps existing runAgentLoop / runAgentLoopContinue
 *
 * Related Docs:
 * - [Agent Loop](./agent-loop.ts)
 * - [Agent](./agent.ts)
 * - [Rust Core Client](./rust-core-client.ts)
 */

import { runAgentLoop, runAgentLoopContinue } from "./agent-loop.ts";
import type { AgentContext, AgentEvent, AgentLoopConfig, AgentMessage, StreamFn } from "./types.ts";

/**
 * Backend abstraction for the agent loop execution.
 * Allows swapping between TS-native and Rust-bridge implementations.
 */
export interface AgentLoopBackend {
	runPrompt(
		prompts: AgentMessage[],
		context: AgentContext,
		config: AgentLoopConfig,
		emit: (event: AgentEvent) => Promise<void> | void,
		signal?: AbortSignal,
		streamFn?: StreamFn,
	): Promise<AgentMessage[]>;

	runContinue(
		context: AgentContext,
		config: AgentLoopConfig,
		emit: (event: AgentEvent) => Promise<void> | void,
		signal?: AbortSignal,
		streamFn?: StreamFn,
	): Promise<AgentMessage[]>;
}

/**
 * Default TS backend that delegates to the existing agent loop functions.
 * This preserves all existing behavior unchanged.
 */
export class TsAgentLoopBackend implements AgentLoopBackend {
	async runPrompt(
		prompts: AgentMessage[],
		context: AgentContext,
		config: AgentLoopConfig,
		emit: (event: AgentEvent) => Promise<void> | void,
		signal?: AbortSignal,
		streamFn?: StreamFn,
	): Promise<AgentMessage[]> {
		return runAgentLoop(prompts, context, config, emit, signal, streamFn);
	}

	async runContinue(
		context: AgentContext,
		config: AgentLoopConfig,
		emit: (event: AgentEvent) => Promise<void> | void,
		signal?: AbortSignal,
		streamFn?: StreamFn,
	): Promise<AgentMessage[]> {
		return runAgentLoopContinue(context, config, emit, signal, streamFn);
	}
}
