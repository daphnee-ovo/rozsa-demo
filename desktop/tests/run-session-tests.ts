// Select focused session tests; native acceptance is always explicit.
const slice = process.argv[2];
if (process.argv.length > 3 || (slice !== undefined && slice !== "session-list" && slice !== "session-history" && slice !== "message-routing")) throw new Error("Expected session-list, session-history, or message-routing");
if (!slice || slice === "session-list") await import("./session-list.test.ts");
if (!slice || slice === "session-history") await import("./session-history.test.ts");
if (!slice || slice === "message-routing") await import("./session-message-routing.test.ts");
