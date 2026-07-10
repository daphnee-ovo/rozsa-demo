---
source: other
nums: 1
---

- [x] ISSUE-I083：GUI streaming cursor still stays below latest text after TASK-T064
  - severity: P1
  - location：crates/rozsa-gui/frontend/app.js:462
  - description：User reports the streaming cursor still does not follow the latest rendered text after the first fix. Need verify actual loaded GUI path and cursor DOM insertion behavior.
  - reproduce：Open GUI, stream an assistant response ending in markdown text/list content, observe cursor remains below latest text instead of inline after latest content.
  - fix：Changed GUI stream cursor insertion to place the cursor immediately after the last non-empty rendered text node, and verified with a localhost harness using the real app.js plus rozsa-gui tests.
