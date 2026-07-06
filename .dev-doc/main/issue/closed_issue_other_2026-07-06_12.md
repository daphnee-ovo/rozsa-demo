---
source: other
nums: 1
---

- [x] ISSUE-I080：GUI: keep tool row expand affordance at far right
  - severity: P2
  - location：crates/rozsa-gui/frontend/index.html
  - description：The tool row chevron should remain at the far right as an expand affordance. Recent tool row layout work moved it near the header text.
  - reproduce：Render GUI tool rows after ISSUE-I079 changes and observe the expand chevron no longer aligned to the far right.
  - fix：Restored the tool row header layout so the expand chevron stays aligned at the far right while keeping the tool title/result formatting from ISSUE-I079.
