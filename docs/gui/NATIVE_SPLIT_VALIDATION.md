# Native split validation

This document is the GO/NO-GO record for moving the Rózsa macOS GUI to a
persistent `NSSplitViewController` containing two real Tauri WKWebViews.

Current implementation: `crates/rozsa-gui/src/native_split_view.rs`,
`crates/rozsa-gui/src/native_titlebar.rs`, and
`crates/rozsa-gui/src/scene_router.rs`.
Inspector launch: `crates/rozsa-gui/src/inspector.rs`. It is disabled by
default and can be enabled explicitly with `ROZSA_WEB_INSPECTOR=1`.
Reusable product-app deployment: `devtools/deploy-test-app.sh`.
The product window can begin loading after both WKWebViews are installed, but
the complete native split root remains hidden. The main and sidebar readiness
handshake reveals that root in one AppKit operation, so neither pane is exposed
ahead of the other.
Foreground harness: `temp/native-split-validation/`.
Related design: `.dev-doc/main/SPEC.md`.

## Fixed dependency boundary

- Tauri: `2.11.5`
- tauri-runtime-wry: `2.11.4`
- Wry: `0.55.1`
- Tauri feature: `unstable`

## Compile checks

| Check | Result | Evidence |
| --- | --- | --- |
| `cargo check --offline --manifest-path temp/native-split-validation/Cargo.toml` | PASS | Finished `dev` profile successfully on 2026-07-14 |
| `cargo clippy --offline --manifest-path temp/native-split-validation/Cargo.toml -- -D warnings` | PASS | Finished without warnings on 2026-07-14 |
| `cargo build --offline --manifest-path temp/native-split-validation/Cargo.toml` | PASS | Produced the foreground validation app successfully on 2026-07-14 |
| Exact dependency versions resolved | PASS | `cargo tree`: Tauri `2.11.5` → tauri-runtime-wry `2.11.4` → Wry `0.55.1` |

## Foreground observations

Run the normal validation app, then complete every row. Identity values and
`bootCount=1` must remain unchanged throughout all non-close operations.

| Behavior | Result | Evidence |
| --- | --- | --- |
| Two real WKWebViews show different stable identities | PASS | Native pointers and DOM UUIDs remained distinct; both DOM `bootCount` values remained `1` |
| Native parent, frame, and Tauri bounds agree | PASS | Stable geometry logs matched at 1120×720, 960×640, 1280×800, and sidebar widths 190/300 |
| Continuous window resize | PASS | Foreground sequence 960×640 → 1280×800 → 1120×720 preserved both identities and matching sizes |
| Programmatic divider movement | PASS | Native splitter changed from 190 to 300; main/sidebar bounds became 820/300 without reload |
| Mouse divider drag | PASS | User foreground observation confirmed both panes resize with the native divider |
| Sidebar collapse and restore | PASS | Sidebar disappeared and returned with the same UUID, `bootCount=1`, and textarea value |
| Fullscreen enter and exit | PASS | Native window controls disappeared/reappeared; both WebView identities remained stable |
| Focus transfer between WebViews | PASS | AX focus moved from main content to sidebar HTML content and back |
| Direct Chinese text value survives collapse/restore | PASS | Sidebar textarea retained `中文输入保持` after collapse/restore |
| Chinese IME behavior during focus-moving layout actions | PASS | User confirmed active composition ends when collapse/fullscreen moves focus; this is the accepted behavior, while committed textarea and DOM state persist |
| File drag and drop reaches the main WebView | PASS | After granting only `core:event:allow-listen` and `allow-unlisten`, the main WebView reported `native_file_drop: PASS` for Finder `drag-sample.txt` |
| Native window drag | PASS | Split-root titlebar drag view moved the window in the foreground without covering traffic lights or page controls |
| Native titlebar double-click zoom | PASS | User foreground observation confirmed double-clicking the same drag view performs window zoom |
| Main and sidebar devtools open | UNVERIFIED | TASK-T011 verified the product main Inspector independently; the combined harness row still needs a fresh run for the sidebar Inspector |
| Normal close releases the native host | PASS | Process exited `0` after AppKit hierarchy cleanup log |

## Product app acceptance (2026-07-14)

This table records direct observation of `cargo run --offline -p rozsa-cli`.
Harness or automated evidence is named explicitly and does not replace a
product-app observation. `UNVERIFIED` means the behavior was not claimed as a
foreground product result.

| Behavior | Product app | Evidence |
| --- | --- | --- |
| Ordinary window, sidebar expanded | PASS | One `NSWindow` displayed the native divider, persistent sidebar WebView, main WebView, and normal traffic lights |
| Narrow window, sidebar expanded | PASS | Requested 700×600 was correctly clamped to configured 900×600 minimum; both panes remained usable |
| Narrow window, sidebar collapsed | UNVERIFIED | The automation used an unstable button index and closed the window before this combination could be observed |
| Maximized window, expanded/collapsed | UNVERIFIED | The attempted Option-click entered fullscreen; the earlier harness double-click zoom result remains PASS |
| Fullscreen enter, sidebar expanded | PASS | Runtime emitted `native-fullscreen=true`, frame became 1408×881, safe-area top became 0, and divider stayed at 340 |
| Fullscreen exit and collapsed combination | UNVERIFIED | External approval reviewer reached its usage limit before the remaining Space transition could be observed |
| Main/sidebar WKWebView and split-item identity | PASS | One installation log reported distinct stable WKWebView pointers; main/settings scene switches, collapse/restore, divider changes, and fullscreen emitted no reinstall or teardown |
| Divider movement and persistence | PASS | Accessibility changed the native splitter from 264 to 340; collapse/restore and fullscreen still reported 340 |
| Main/settings reuse the same sidebar container | PASS | Settings and main content switched inside the installed host; the same native Sidebar button collapsed/restored both scenes |
| Focus transfer | UNVERIFIED | Foreground harness PASS; no separate product-app focus trace was captured |
| Chinese IME | UNVERIFIED | Foreground harness accepted composition interruption with committed state retained; not repeated in the product app |
| Finder file drag-and-drop | UNVERIFIED | Foreground harness PASS; not repeated in the product app |
| Main Web Inspector | PASS | TASK-T011 foreground run emitted `frontend loaded and detached`; Accessibility reported separate `Rózsa` 1280×820 and `Web Inspector — localhost` 1000×650 standard windows, with unchanged geometry and no intervening native resize logs |
| Sidebar Web Inspector | UNVERIFIED | Foreground harness PASS; not repeated in the product app |
| Traffic lights and sidebar control | PASS | Standard close/fullscreen/minimize buttons and the native Sidebar button remained visible and operable |
| Native window drag | UNVERIFIED | Foreground harness and user observation PASS; not repeated after product migration |
| Titlebar double-click zoom | UNVERIFIED | Foreground harness and user observation PASS; product-app maximize was not captured |
| Normal close | PASS | Earlier product run emitted `native-split teardown: child WebView closed` and exited 0 |
| Close with pending approval denies request | UNVERIFIED | Permission-controller tests cover denial semantics; no live pending product request was created |
| Opaque Dark sidebar | PASS | After making the sidebar WKWebView transparent, both AppKit backing and WebView content rendered Dark without a light layer |
| Translucent Dark sidebar | PASS | Initial test exposed light `NSVisualEffectView`; applying `NSAppearanceNameDarkAqua` fixed the contrast on retest |
| Live theme/surface ordering | PASS | Light/Dark and opaque/translucent switches updated both panes together; no visible backing flash was observed |

The Inspector PASS above was captured with `ROZSA_WEB_INSPECTOR=1`. Normal
product and test-app launches show only the product window.

## Automated acceptance

| Check | Result | Evidence |
| --- | --- | --- |
| macOS native split/source contract | PASS | `native_split_host`, `native_window_behavior`, `theme_surface`, targeted routing, and scene continuity suites pass |
| Non-macOS frontend fallback | PASS | `frontend_platform_fallback` verifies the single-WebView CSS Grid pane fallback remains available outside macOS; this layout fallback is distinct from the `main.css` / `sidebar.css` stylesheet entries |
| Real non-macOS GUI | UNVERIFIED | No non-macOS graphical runtime was available in this workspace |

## Injected failure cleanup

Launch with `ROZSA_NATIVE_SPLIT_INJECT_FAILURE=1`. The app installs the split
controller completely, then deliberately runs rollback. The expected log is:

```text
[native-split-validation] cleanup PASS: constraints, split items, and controllers removed; original main hierarchy restored
[native-split-validation] FAILURE_PROBE PASS
[native-split-validation] cleanup PASS: child WebView closed after AppKit rollback
```

| Check | Result | Evidence |
| --- | --- | --- |
| Child WebView closed | PASS | Tauri dispatcher emitted `child WebView closed after AppKit rollback` |
| Constraints deactivated in reverse cleanup | PASS | Rollback completed without Auto Layout errors and emitted cleanup PASS |
| Split items and controllers removed | PASS | Rollback emitted cleanup PASS before child WebView closure |
| Original single-WebView hierarchy restored | PASS | Foreground screenshot and geometry log showed only main WKWebView at 1120×720 |

## Decision

**GO with explicit observation gaps**. The architecture, native installation,
scene reuse, targeted routing, divider persistence, ordinary/narrow layout,
fullscreen entry, theme backing, main Inspector separation, and normal close
have product or automated PASS evidence. Rows marked `UNVERIFIED` are not represented as
product-app passes; they retain only the separately recorded harness or test
coverage where stated.
