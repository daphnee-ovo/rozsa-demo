import type { CodeHighlighterPlugin, HighlightOptions, HighlightResult, ThemeInput } from "@streamdown/code";

const lightTheme = {
  name: "rozsa-light",
  type: "light",
  colors: { "editor.foreground": "#eee8de", "editor.background": "#322d28" },
  tokenColors: [
    { scope: ["comment", "punctuation.definition.comment"], settings: { foreground: "#b3a99a" } },
    { scope: ["keyword", "storage", "keyword.operator"], settings: { foreground: "#d9a7ae" } },
    { scope: ["string", "constant.character"], settings: { foreground: "#c5c49a" } },
    { scope: ["constant.numeric", "constant.language"], settings: { foreground: "#c3ad91" } },
    { scope: ["entity.name.function", "support.function"], settings: { foreground: "#d3b6a0" } },
    { scope: ["entity.name.type", "support.type"], settings: { foreground: "#c4b9a8" } },
  ],
} as unknown as ThemeInput;

const darkTheme = {
  name: "rozsa-dark",
  type: "dark",
  colors: { "editor.foreground": "#e6dfd3", "editor.background": "#211e1b" },
  tokenColors: [
    { scope: ["comment", "punctuation.definition.comment"], settings: { foreground: "#9e9588" } },
    { scope: ["keyword", "storage", "keyword.operator"], settings: { foreground: "#d49aa2" } },
    { scope: ["string", "constant.character"], settings: { foreground: "#b9b98f" } },
    { scope: ["constant.numeric", "constant.language"], settings: { foreground: "#c5a98a" } },
    { scope: ["entity.name.function", "support.function"], settings: { foreground: "#cda991" } },
    { scope: ["entity.name.type", "support.type"], settings: { foreground: "#b9b0a3" } },
  ],
} as unknown as ThemeInput;

export const CODE_HIGHLIGHT_THEMES: [ThemeInput, ThemeInput] = [lightTheme, darkTheme];
export type CodeHighlightRequest = Pick<HighlightOptions, "code" | "isIncomplete"> & { language: string };
export type CodeHighlighter = Pick<CodeHighlighterPlugin, "highlight">;

let highlighter: CodeHighlighterPlugin | undefined;
let highlighterPromise: Promise<CodeHighlighterPlugin> | undefined;

export function loadCodeHighlighter(): Promise<CodeHighlighterPlugin> {
  if (!highlighterPromise) highlighterPromise = import("@streamdown/code").then(({ createCodePlugin }) => {
    highlighter = createCodePlugin({ themes: [...CODE_HIGHLIGHT_THEMES] });
    return highlighter;
  });
  return highlighterPromise;
}

export function highlightCode(request: CodeHighlightRequest, callback?: (result: HighlightResult) => void): HighlightResult | null {
  const options = { ...request, language: request.language as HighlightOptions["language"], themes: [...CODE_HIGHLIGHT_THEMES] as [ThemeInput, ThemeInput] };
  if (highlighter) return highlighter.highlight(options, callback);
  void loadCodeHighlighter().then(plugin => {
    const immediate = plugin.highlight(options, callback);
    if (immediate) callback?.(immediate);
  });
  return null;
}

