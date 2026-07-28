---
source: other
nums: 1
---

- [x] ISSUE-I007：Prevent thinking slider popover clipping inside composer
  - severity: P0
  - location：crates/rozsa-gui/frontend/index.html:2907
  - description：The thinking-level popover is nested inside .input-wrapper, whose overflow:hidden clips the panel so only level dots are visible instead of the full slider.
  - reproduce：Open the validation app, select a reasoning model, click the thinking level badge, and visually inspect the panel above the composer.
  - fix：Moved the popover outside the clipped composer, anchored it with fixed viewport positioning, and rendered a full-height filled range track with a large thumb; added clipping regression coverage and visually verified Off and Medium states in the deployed app.
  - files_modify: [crates/rozsa-gui/frontend/index.html, crates/rozsa-gui/frontend/app.js, crates/rozsa-gui/tests/session_title_test.rs]
  - files_create: []
