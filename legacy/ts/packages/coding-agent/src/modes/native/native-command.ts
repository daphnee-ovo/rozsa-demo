import { existsSync } from "node:fs";
import { resolve } from "node:path";
import { getPackageDir } from "../../config.ts";
import type { NativeToHostMessage } from "./protocol.ts";

function getDefaultNativeBinaryPath(): string {
	const suffix = process.platform === "win32" ? ".exe" : "";
	const repoRoot = resolve(getPackageDir(), "..", "..");
	const workspaceBin = resolve(repoRoot, "target", "debug", `rozsa-tui${suffix}`);
	if (existsSync(workspaceBin)) return workspaceBin;
	return resolve(repoRoot, "crates", "rozsa-tui", "target", "debug", `rozsa-tui${suffix}`);
}

function getNativeCargoManifestPath(): string {
	return resolve(getPackageDir(), "..", "..", "crates", "rozsa-tui", "Cargo.toml");
}

export function resolveNativeCommand(): { command: string; args: string[] } {
	const configuredPath = process.env.ROZSA_NATIVE_TUI_PATH;
	if (configuredPath) {
		return { command: configuredPath, args: [] };
	}

	const binaryPath = getDefaultNativeBinaryPath();
	if (existsSync(binaryPath)) {
		return { command: binaryPath, args: [] };
	}

	const manifestPath = getNativeCargoManifestPath();
	if (existsSync(manifestPath)) {
		return { command: "cargo", args: ["run", "--manifest-path", manifestPath, "--quiet"] };
	}

	return { command: binaryPath, args: [] };
}

export function parseNativeLine(line: string): NativeToHostMessage | undefined {
	try {
		const parsed = JSON.parse(line) as NativeToHostMessage;
		if (parsed && typeof parsed === "object" && "type" in parsed) return parsed;
	} catch {
		return undefined;
	}
	return undefined;
}

export function stringRecord(values: Map<string, string>): Record<string, string> {
	const record: Record<string, string> = {};
	for (const [key, value] of values) record[key] = value;
	return record;
}

export function linesRecord(values: Map<string, string[]>): Record<string, string[]> {
	const record: Record<string, string[]> = {};
	for (const [key, value] of values) record[key] = value;
	return record;
}

export function resolveTuiBackend(value: string | undefined): "rust" | "typescript" {
	return value === "typescript" || value === "ts" ? "typescript" : "rust";
}
