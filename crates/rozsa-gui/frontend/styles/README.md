# Runtime CSS architecture

The Tauri webviews load two stylesheet entry points:

- `main.css` for the main window.
- `sidebar.css` for the native sidebar webview.

Import order is part of the visual contract. Keep imports ordered from broad,
stable foundations to narrow state overrides:

1. `tokens.css` and `reset.css`
2. `base.css` and `layout/`
3. `components/`
4. `features/`
5. `utilities.css` and late compatibility overrides

## Responsibilities

- `tokens.css`: semantic color, typography, radius, and scale custom properties.
- `reset.css`: shared browser normalization that is safe in every webview.
- `base.css`: main-window document defaults.
- `layout/`: window shells and region geometry; no feature-specific presentation.
- `components/`: reusable controls, overlays, forms, and feedback surfaces.
- `features/`: styles coupled to a product feature or scene.
- `utilities.css`: cross-feature state helpers and rich-content utilities.

Add a rule to the narrowest stable responsibility. Create a new feature file
when a scene grows independently; do not add runtime `<style>` tags or static
`style` attributes to HTML. Dynamic values such as progress width may still be
set by JavaScript when they represent runtime state.

## Related docs

- [`docs/gui/ARCHITECTURE.md`](../../../../docs/gui/ARCHITECTURE.md)
- [`docs/gui/UI_USAGE_GUIDELINES.md`](../../../../docs/gui/UI_USAGE_GUIDELINES.md)
- [`docs/gui/FRONTEND_TERMINOLOGY.md`](../../../../docs/gui/FRONTEND_TERMINOLOGY.md)
- [`docs/gui/themes.md`](../../../../docs/gui/themes.md)
