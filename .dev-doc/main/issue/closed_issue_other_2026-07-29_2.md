---
source: other
nums: 1
---

- [x] ISSUE-I008：Reduce thinking slider visual scale
  - severity: P1
  - location：crates/rozsa-gui/frontend/index.html:322
  - description：The corrected thinking slider is fully visible but its 420px panel, 42px track, and 54px thumb are visually oversized relative to the compact composer.
  - reproduce：Open the thinking level picker in the validation app and compare its visual weight with adjacent composer controls.
  - fix：Reduced the popover from 420px to 320px, the track from 42px to 28px, and the thumb from 54px to 38px; tightened padding, typography, radius, shadow, and marker scale while preserving fixed positioning and full interaction.
  - files_modify: [crates/rozsa-gui/frontend/index.html, crates/rozsa-gui/tests/session_title_test.rs]
  - files_create: []
