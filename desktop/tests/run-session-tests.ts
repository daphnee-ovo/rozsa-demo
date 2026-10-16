// Select only focused session tests; native acceptance is always explicit.
const slice = process.argv[2];
if (process.argv.length > 3 || (slice !== undefined && slice !== "session-list" && slice !== "session-history")) throw new Error("Expected session-list or session-history");
if (!slice || slice === "session-list") await import("./session-list.test.ts");
if (!slice || slice === "session-history") await import("./session-history.test.ts");
