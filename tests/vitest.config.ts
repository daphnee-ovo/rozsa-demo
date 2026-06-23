import { fileURLToPath } from "node:url";
import { defineConfig } from "vitest/config";

const aiSrcIndex = fileURLToPath(new URL("../packages/ai/src/index.ts", import.meta.url));
const aiSrcOAuth = fileURLToPath(new URL("../packages/ai/src/oauth.ts", import.meta.url));
const agentSrcIndex = fileURLToPath(new URL("../packages/agent/src/index.ts", import.meta.url));
const tuiSrcIndex = fileURLToPath(new URL("../packages/tui/src/index.ts", import.meta.url));

export default defineConfig({
	test: {
		globals: true,
		environment: "node",
		testTimeout: 30000,
		root: ".",
		server: {
			deps: {
				external: [/@silvia-odwyer\/photon-node/],
			},
		},
	},
	resolve: {
		alias: [
			{ find: /^@earendil-works\/rozsa-ai$/, replacement: aiSrcIndex },
			{ find: /^@earendil-works\/rozsa-ai\/oauth$/, replacement: aiSrcOAuth },
			{ find: /^@earendil-works\/rozsa-agent-core$/, replacement: agentSrcIndex },
			{ find: /^@earendil-works\/rozsa-tui$/, replacement: tuiSrcIndex },
			{ find: /^@mariozechner\/rozsa-ai$/, replacement: aiSrcIndex },
			{ find: /^@mariozechner\/rozsa-ai\/oauth$/, replacement: aiSrcOAuth },
			{ find: /^@mariozechner\/rozsa-agent-core$/, replacement: agentSrcIndex },
		],
	},
});
