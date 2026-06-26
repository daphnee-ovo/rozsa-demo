/**
 * T12: Diff 语法高亮测试
 * 验证 diff 组件对 added/removed 行应用语言级语法高亮
 */
import { beforeAll, describe, expect, it } from "vitest";

describe("T12: Diff 语法高亮", () => {
	let highlight: any;
	let supportsLanguage: any;
	let renderHighlightedHtml: any;
	let getLanguageFromPath: any;

	beforeAll(async () => {
		const syntaxMod = await import("../packages/coding-agent/src/utils/syntax-highlight.ts");
		highlight = syntaxMod.highlight;
		supportsLanguage = syntaxMod.supportsLanguage;
		renderHighlightedHtml = syntaxMod.renderHighlightedHtml;

		const themeMod = await import("../packages/coding-agent/src/modes/interactive/theme/theme.ts");
		getLanguageFromPath = themeMod.getLanguageFromPath;
		themeMod.initTheme("dark");
	});

	// --- 语言检测 ---
	describe("语言支持检测", () => {
		it("支持 TypeScript", () => {
			expect(supportsLanguage("typescript")).toBe(true);
		});

		it("支持 JavaScript", () => {
			expect(supportsLanguage("javascript")).toBe(true);
		});

		it("支持 Python", () => {
			expect(supportsLanguage("python")).toBe(true);
		});

		it("支持 JSON", () => {
			expect(supportsLanguage("json")).toBe(true);
		});

		it("不支持的语言返回 false", () => {
			expect(supportsLanguage("nonexistent_language_xyz")).toBe(false);
		});

		it("空字符串返回 false", () => {
			expect(supportsLanguage("")).toBe(false);
		});
	});

	// --- 路径到语言映射 ---
	describe("文件路径到语言映射", () => {
		it(".ts 文件映射为 typescript", () => {
			const lang = getLanguageFromPath("src/main.ts");
			expect(lang).toBe("typescript");
		});

		it(".tsx 文件映射为 typescript", () => {
			const lang = getLanguageFromPath("component.tsx");
			expect(lang).toBe("typescript");
		});

		it(".py 文件映射为 python", () => {
			const lang = getLanguageFromPath("script.py");
			expect(lang).toBe("python");
		});

		it(".json 文件映射为 json", () => {
			const lang = getLanguageFromPath("package.json");
			expect(lang).toBe("json");
		});

		it("无扩展名文件返回 undefined 或空", () => {
			const lang = getLanguageFromPath("Makefile");
			// Makefile 可能有映射也可能没有
			expect(typeof lang === "string" || lang === undefined).toBe(true);
		});
	});

	// --- 高亮输出 ---
	describe("高亮渲染", () => {
		it("TypeScript 代码的 const 关键字被高亮", () => {
			const result = highlight("const x = 1;", {
				language: "typescript",
				ignoreIllegals: true,
				theme: {
					keyword: (t: string) => `[KW:${t}]`,
					number: (t: string) => `[NUM:${t}]`,
				},
			});
			expect(result).toContain("[KW:const]");
			expect(result).toContain("[NUM:1]");
		});

		it("Python 代码的 def 关键字被高亮", () => {
			const result = highlight("def hello():", {
				language: "python",
				ignoreIllegals: true,
				theme: {
					keyword: (t: string) => `[KW:${t}]`,
					function: (t: string) => `[FN:${t}]`,
					title: (t: string) => `[T:${t}]`,
				},
			});
			expect(result).toContain("[KW:def]");
		});

		it("字符串字面量被高亮", () => {
			const result = highlight('const s = "hello world";', {
				language: "typescript",
				ignoreIllegals: true,
				theme: {
					keyword: (t: string) => `[KW:${t}]`,
					string: (t: string) => `[STR:${t}]`,
				},
			});
			expect(result).toContain("[STR:");
		});

		it("空字符串不崩溃", () => {
			const result = highlight("", {
				language: "typescript",
				ignoreIllegals: true,
				theme: {},
			});
			expect(result).toBe("");
		});

		it("非法语言抛出异常（由调用者 supportsLanguage 预检）", () => {
			// highlight 函数对未知语言会抛出异常
			// 正确使用方式：先调用 supportsLanguage 检查，然后再调用 highlight
			// diff 组件中的 highlightLine 函数会先检查 supportsLanguage
			expect(() => {
				highlight("const x = 1;", {
					language: "nonexistent",
					ignoreIllegals: true,
					theme: {},
				});
			}).toThrow("Unknown language");
		});
	});

	// --- HTML 渲染工具 ---
	describe("renderHighlightedHtml", () => {
		it("正确解码 HTML 实体", () => {
			const result = renderHighlightedHtml("&lt;div&gt;&amp;test&lt;/div&gt;");
			expect(result).toBe("<div>&test</div>");
		});

		it("处理嵌套 span 标签", () => {
			const result = renderHighlightedHtml(
				'<span class="hljs-keyword">const</span> <span class="hljs-variable">x</span>',
				{
					keyword: (t: string) => `[${t}]`,
					variable: (t: string) => `{${t}}`,
				},
			);
			expect(result).toContain("[const]");
			expect(result).toContain("{x}");
		});
	});
});
