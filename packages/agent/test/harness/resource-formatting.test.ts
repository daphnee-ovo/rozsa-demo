import { describe, expect, it } from "vitest";
import { formatPromptTemplateInvocation } from "../../src/harness/prompt-templates.ts";
import { formatSkillInvocation } from "../../src/harness/skills.ts";

describe("resource formatting helpers", () => {
	it("formats skill invocations with additional instructions", () => {
		const skill = {
			name: "inspect",
			description: "Inspect things",
			content: "Use inspection tools.",
			filePath: "/project/.pi/skills/inspect/SKILL.md",
		};

		expect(formatSkillInvocation(skill, "Check errors.")).toBe(
			"<skill>\n<name>inspect</name>\n<content>\nUse inspection tools.\n</content>\n<base_dir>/project/.pi/skills/inspect</base_dir>\n</skill>\n\nCheck errors.",
		);
	});

	it("formats prompt template invocations with positional arguments", () => {
		expect(
			formatPromptTemplateInvocation({ name: "review", content: "Review $1 with $ARGUMENTS" }, ["a.ts", "care"]),
		).toBe("Review a.ts with a.ts care");
	});
});
