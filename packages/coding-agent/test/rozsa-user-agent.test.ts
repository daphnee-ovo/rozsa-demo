import { describe, expect, it } from "vitest";
import { getRozsaUserAgent } from "../src/utils/rozsa-user-agent.ts";

describe("getRozsaUserAgent", () => {
	it("formats the user agent expected by rozsa.dev", () => {
		const runtime = process.versions.bun ? `bun/${process.versions.bun}` : `node/${process.version}`;
		const userAgent = getRozsaUserAgent("1.2.3");

		expect(userAgent).toBe(`rozsa/1.2.3 (${process.platform}; ${runtime}; ${process.arch})`);
		expect(userAgent).toMatch(/^rozsa\/[^\s()]+ \([^;()]+;\s*[^;()]+;\s*[^()]+\)$/);
	});
});
