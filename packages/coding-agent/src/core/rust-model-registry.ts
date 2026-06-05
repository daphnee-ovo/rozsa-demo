/**
 * Rust model registry bridge.
 *
 * Calls the `rozsa-app` JSONL stdio bridge so frontend model lists can be backed by
 * the Rust ModelRegistry. Related docs: docs/model/supported-providers.md.
 */

import { type SpawnSyncReturns, spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import type { Api, Model } from "@earendil-works/pi-ai";

const RUST_APP_BINARY_NAME = process.platform === "win32" ? "rozsa-app.exe" : "rozsa-app";
const SOURCE_REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "../../../..");
const DEFAULT_RUST_APP_BINARY_CANDIDATES = [
	resolve(SOURCE_REPO_ROOT, "target", "debug", RUST_APP_BINARY_NAME),
	resolve(process.cwd(), "target", "debug", RUST_APP_BINARY_NAME),
];
const MAX_STDERR_CHARS = 4000;

export interface ProviderAvailableEntry {
	configured: boolean;
	source?: string;
}

interface RustModelsLine {
	type: "models";
	id: string;
	models: Model<Api>[];
	providerAvailable?: Record<string, ProviderAvailableEntry>;
	errors?: string[];
}

interface RustErrorLine {
	type: "error";
	id: string;
	message: string;
	code?: string;
}

type RustRegistryLine = RustModelsLine | RustErrorLine;

export interface RustRegistryResult {
	models: Model<Api>[];
	providerAvailable?: Record<string, ProviderAvailableEntry>;
	errors: string[];
}

export function loadRustModelRegistryModels(modelsJsonPath: string | undefined): RustRegistryResult | undefined {
	const backend = process.env.ROZSA_MODEL_REGISTRY_BACKEND ?? "auto";
	if (backend === "ts") {
		return undefined;
	}
	const binary = resolveRustAppBinary();
	if (backend === "auto" && !existsSync(binary)) {
		return undefined;
	}

	const requestId = `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
	const request = {
		type: "list_models",
		id: requestId,
		modelsJsonPath,
		discoverNvidia: true,
	};
	const result = spawnSync(binary, resolveRustAppBinaryArgs(), {
		input: `${JSON.stringify(request)}\n`,
		encoding: "utf8",
		maxBuffer: 16 * 1024 * 1024,
	});

	return parseRustRegistryResult(result, requestId);
}

export function resolveRustAppBinary(): string {
	if (process.env.ROZSA_APP_BINARY) {
		return process.env.ROZSA_APP_BINARY;
	}
	return (
		DEFAULT_RUST_APP_BINARY_CANDIDATES.find((candidate) => existsSync(candidate)) ??
		DEFAULT_RUST_APP_BINARY_CANDIDATES[0]
	);
}

export function resolveRustAppBinaryArgs(): string[] {
	const rawArgs = process.env.ROZSA_APP_BINARY_ARGS;
	if (!rawArgs) {
		return [];
	}
	const parsed = JSON.parse(rawArgs) as unknown;
	if (!Array.isArray(parsed) || parsed.some((value) => typeof value !== "string")) {
		throw new Error("ROZSA_APP_BINARY_ARGS must be a JSON string array");
	}
	return parsed;
}

function parseRustRegistryResult(result: SpawnSyncReturns<string>, requestId: string): RustRegistryResult | undefined {
	if (result.error) {
		if (process.env.ROZSA_MODEL_REGISTRY_BACKEND === "rust") {
			throw result.error;
		}
		return undefined;
	}
	if (result.status !== 0) {
		if (process.env.ROZSA_MODEL_REGISTRY_BACKEND === "rust") {
			const stderr = result.stderr.trim().slice(-MAX_STDERR_CHARS);
			throw new Error(`rozsa-app exited with status ${result.status}${stderr ? `: ${stderr}` : ""}`);
		}
		return undefined;
	}

	for (const line of result.stdout.split(/\r?\n/)) {
		if (!line.trim()) continue;
		const parsed = parseRustRegistryLine(line);
		if (!parsed || parsed.id !== requestId) continue;
		if (parsed.type === "error") {
			if (process.env.ROZSA_MODEL_REGISTRY_BACKEND === "rust") {
				throw new Error(parsed.message);
			}
			return undefined;
		}
		return { models: parsed.models, providerAvailable: parsed.providerAvailable, errors: parsed.errors ?? [] };
	}

	if (process.env.ROZSA_MODEL_REGISTRY_BACKEND === "rust") {
		throw new Error("rozsa-app did not return a model registry response");
	}
	return undefined;
}

function parseRustRegistryLine(line: string): RustRegistryLine | undefined {
	try {
		const parsed = JSON.parse(line) as RustRegistryLine;
		return parsed && (parsed.type === "models" || parsed.type === "error") ? parsed : undefined;
	} catch {
		return undefined;
	}
}
