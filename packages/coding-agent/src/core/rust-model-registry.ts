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
import type { Api, ImagesApi, ImagesModel, Model } from "@earendil-works/pi-ai";

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

interface RustImageModelsLine {
	type: "image_models";
	id: string;
	imageModels: ImagesModel<ImagesApi>[];
	providerAvailable?: Record<string, ProviderAvailableEntry>;
}

interface RustErrorLine {
	type: "error";
	id: string;
	message: string;
	code?: string;
}

type RustRegistryLine = RustModelsLine | RustErrorLine;
type RustImageRegistryLine = RustImageModelsLine | RustErrorLine;

export interface RustRegistryResult {
	models: Model<Api>[];
	providerAvailable?: Record<string, ProviderAvailableEntry>;
	errors: string[];
}

export interface RustImageRegistryResult {
	imageModels: ImagesModel<ImagesApi>[];
	providerAvailable?: Record<string, ProviderAvailableEntry>;
}

export function loadRustModelRegistryModels(modelsJsonPath: string | undefined): RustRegistryResult {
	const binary = resolveRustAppBinary();

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

export function loadRustImageModelRegistryModels(): RustImageRegistryResult {
	const binary = resolveRustAppBinary();

	const requestId = `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
	const request = {
		type: "list_image_models",
		id: requestId,
	};
	const result = spawnSync(binary, resolveRustAppBinaryArgs(), {
		input: `${JSON.stringify(request)}\n`,
		encoding: "utf8",
		maxBuffer: 16 * 1024 * 1024,
	});

	return parseRustImageRegistryResult(result, requestId);
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

function parseRustRegistryResult(result: SpawnSyncReturns<string>, requestId: string): RustRegistryResult {
	if (result.error) {
		throw result.error;
	}
	if (result.status !== 0) {
		const stderr = result.stderr.trim().slice(-MAX_STDERR_CHARS);
		throw new Error(`rozsa-app exited with status ${result.status}${stderr ? `: ${stderr}` : ""}`);
	}

	for (const line of result.stdout.split(/\r?\n/)) {
		if (!line.trim()) continue;
		const parsed = parseRustRegistryLine(line);
		if (!parsed || parsed.id !== requestId) continue;
		if (parsed.type === "error") {
			throw new Error(parsed.message);
		}
		return { models: parsed.models, providerAvailable: parsed.providerAvailable, errors: parsed.errors ?? [] };
	}

	throw new Error("rozsa-app did not return a model registry response");
}

function parseRustImageRegistryResult(result: SpawnSyncReturns<string>, requestId: string): RustImageRegistryResult {
	if (result.error) {
		throw result.error;
	}
	if (result.status !== 0) {
		const stderr = result.stderr.trim().slice(-MAX_STDERR_CHARS);
		throw new Error(`rozsa-app exited with status ${result.status}${stderr ? `: ${stderr}` : ""}`);
	}

	for (const line of result.stdout.split(/\r?\n/)) {
		if (!line.trim()) continue;
		const parsed = parseRustImageRegistryLine(line);
		if (!parsed || parsed.id !== requestId) continue;
		if (parsed.type === "error") {
			throw new Error(parsed.message);
		}
		return { imageModels: parsed.imageModels, providerAvailable: parsed.providerAvailable };
	}

	throw new Error("rozsa-app did not return an image model registry response");
}

function parseRustRegistryLine(line: string): RustRegistryLine | undefined {
	try {
		const parsed = JSON.parse(line) as RustRegistryLine;
		return parsed && (parsed.type === "models" || parsed.type === "error") ? parsed : undefined;
	} catch {
		return undefined;
	}
}

function parseRustImageRegistryLine(line: string): RustImageRegistryLine | undefined {
	try {
		const parsed = JSON.parse(line) as RustImageRegistryLine;
		return parsed && (parsed.type === "image_models" || parsed.type === "error") ? parsed : undefined;
	} catch {
		return undefined;
	}
}
