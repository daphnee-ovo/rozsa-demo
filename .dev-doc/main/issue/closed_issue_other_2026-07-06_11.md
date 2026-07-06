---
source: other
nums: 1
---

- [x] ISSUE-I079：GUI: tool call rows and quota sidebar show misleading data
  - severity: P1
  - location：crates/rozsa-gui/frontend/app.js; crates/rozsa-gui/src/state.rs
  - description：GUI tool call rows display raw internal tool names and flattened previews instead of command-style tool name plus primary argument and folded result. Sidebar quota bars show context/session token values as 5h/weekly quota, which is misleading.
  - reproduce：Run GUI with a tool-using assistant turn. Observe rows like ls/find/bash shown as unrelated tools and quota labels populated from context usage instead of actual rate limit/quota snapshots.
  - fix：Changed GUI tool rows to show ToolName plus primary argument in the header and folded tool output in the body; kept Bash command visible as Bash <command>; tightened tool row toggle placement; removed misleading quota calculations from context/session tokens and show placeholders until real quota data is available.
