//! Persistent macOS native split host.
//!
//! Structure: `install` creates the sidebar WebView; `record_webview` waits for
//! both native WKWebView handles; `install_native_split` moves them into one
//! `NSSplitViewController`; `apply_theme_surface` updates the AppKit-owned
//! sidebar backing; `teardown` restores the original Tauri hierarchy.
//! Tauri owns both WebView handles. The AppKit window/controller hierarchy owns
//! the split controller, items, child controllers, panes, and constraints.
//! `NativeSplitHost` remains in main-thread `thread_local` storage so AppKit
//! objects never enter Tauri managed state or cross a thread boundary.
//!
//! Installation order: create child WebView -> retain native handles -> create
//! child controllers/items -> pin both WKWebViews with Auto Layout -> install
//! the split controller. The window can then load while the complete split root
//! stays hidden. Both WebViews completing `gui_webview_ready` reveals that root
//! in one AppKit operation, so the two panes first appear as one surface.
//! Teardown reverses that order before asynchronously closing the child WebView,
//! avoiding a main-thread dispatcher deadlock.
//! Theme updates are revisioned and ordered native backing first, WebView event
//! second. The sidebar WKWebView stays transparent above either a system
//! sidebar material or the current theme's opaque sidebar color.
//!
//! Related design: `docs/gui/ARCHITECTURE.md` and `.dev-doc/main/SPEC.md`.
//! Lifecycle evidence: `docs/gui/NATIVE_SPLIT_VALIDATION.md`.

#![cfg(target_os = "macos")]

use std::cell::RefCell;
use std::sync::{Arc, Mutex};

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{MainThreadMarker, MainThreadOnly, msg_send};
use objc2_app_kit::{
    NSAppearance, NSAppearanceCustomization, NSAutoresizingMaskOptions, NSBox, NSBoxType, NSColor,
    NSLayoutConstraint, NSSplitViewController, NSSplitViewItem, NSView, NSViewController,
    NSVisualEffectBlendingMode, NSVisualEffectMaterial, NSVisualEffectState, NSVisualEffectView,
    NSWindow,
};
use objc2_foundation::{NSArray, NSRect, NSString, ns_string};
use tauri::webview::WebviewBuilder;
use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, WebviewUrl, WebviewWindow};

const SIDEBAR_INITIAL_WIDTH: f64 = 260.0;
const SIDEBAR_MIN_WIDTH: f64 = 190.0;
const SIDEBAR_MAX_WIDTH: f64 = 420.0;

thread_local! {
    static HOST: RefCell<Option<NativeSplitHost>> = const { RefCell::new(None) };
}

#[derive(Default)]
struct PendingInstall {
    main_view: Option<usize>,
    sidebar_view: Option<usize>,
    window: Option<usize>,
    started: bool,
    on_installed: Option<Box<dyn FnOnce(usize) -> Result<(), String> + Send>>,
}

struct NativeSplitHost {
    window: Retained<NSWindow>,
    original_controller: Option<Retained<NSViewController>>,
    original_parent: Retained<NSView>,
    original_frame: NSRect,
    original_autoresizing: NSAutoresizingMaskOptions,
    main_view: Retained<NSView>,
    sidebar_view: Retained<NSView>,
    sidebar_pane: Retained<NSView>,
    sidebar_overlay: Retained<NSView>,
    sidebar_material: Retained<NSVisualEffectView>,
    sidebar_opaque_backing: Retained<NSBox>,
    main_constraints: Retained<NSArray<NSLayoutConstraint>>,
    sidebar_constraints: Retained<NSArray<NSLayoutConstraint>>,
    sidebar_material_constraints: Retained<NSArray<NSLayoutConstraint>>,
    sidebar_opaque_constraints: Retained<NSArray<NSLayoutConstraint>>,
    sidebar_overlay_constraints: Retained<NSArray<NSLayoutConstraint>>,
    sidebar_overlay_width_constraint: Retained<NSLayoutConstraint>,
    overlay_sidebar_constraints: Retained<NSArray<NSLayoutConstraint>>,
    overlay_material_constraints: Retained<NSArray<NSLayoutConstraint>>,
    overlay_opaque_constraints: Retained<NSArray<NSLayoutConstraint>>,
    split_controller: Retained<NSSplitViewController>,
    sidebar_item: Retained<NSSplitViewItem>,
    main_item: Retained<NSSplitViewItem>,
    _sidebar_controller: Retained<NSViewController>,
    _main_controller: Retained<NSViewController>,
    sidebar_webview: tauri::Webview,
    theme_revision: u64,
    sidebar_overlay_visible: bool,
}

#[derive(Clone)]
pub struct NativeThemeVariant {
    pub translucent: bool,
    pub opaque_color: String,
}

#[derive(Clone)]
pub struct NativeThemeSurface {
    pub theme_mode: String,
    pub light: NativeThemeVariant,
    pub dark: NativeThemeVariant,
}

impl NativeSplitHost {
    fn set_sidebar_overlay_visible(&mut self, visible: bool) -> Result<(), String> {
        if visible == self.sidebar_overlay_visible {
            return Ok(());
        }
        if visible && !self.sidebar_item.isCollapsed() {
            return Ok(());
        }

        if visible {
            NSLayoutConstraint::deactivateConstraints(&self.sidebar_constraints);
            NSLayoutConstraint::deactivateConstraints(&self.sidebar_material_constraints);
            NSLayoutConstraint::deactivateConstraints(&self.sidebar_opaque_constraints);
            self.sidebar_view.removeFromSuperview();
            self.sidebar_material.removeFromSuperview();
            self.sidebar_opaque_backing.removeFromSuperview();
            self.sidebar_overlay
                .addSubview(&self.sidebar_opaque_backing);
            self.sidebar_overlay.addSubview(&self.sidebar_material);
            self.sidebar_overlay.addSubview(&self.sidebar_view);
            NSLayoutConstraint::activateConstraints(&self.overlay_opaque_constraints);
            NSLayoutConstraint::activateConstraints(&self.overlay_material_constraints);
            NSLayoutConstraint::activateConstraints(&self.overlay_sidebar_constraints);
            self.sidebar_overlay.setHidden(false);
        } else {
            self.sidebar_overlay.setHidden(true);
            NSLayoutConstraint::deactivateConstraints(&self.overlay_sidebar_constraints);
            NSLayoutConstraint::deactivateConstraints(&self.overlay_material_constraints);
            NSLayoutConstraint::deactivateConstraints(&self.overlay_opaque_constraints);
            self.sidebar_view.removeFromSuperview();
            self.sidebar_material.removeFromSuperview();
            self.sidebar_opaque_backing.removeFromSuperview();
            self.sidebar_pane.addSubview(&self.sidebar_opaque_backing);
            self.sidebar_pane.addSubview(&self.sidebar_material);
            self.sidebar_pane.addSubview(&self.sidebar_view);
            NSLayoutConstraint::activateConstraints(&self.sidebar_opaque_constraints);
            NSLayoutConstraint::activateConstraints(&self.sidebar_material_constraints);
            NSLayoutConstraint::activateConstraints(&self.sidebar_constraints);
        }
        self.sidebar_overlay_visible = visible;
        Ok(())
    }

    fn restore_hierarchy(self) -> tauri::Webview {
        self.main_view.removeFromSuperview();
        self.sidebar_view.removeFromSuperview();
        self.sidebar_material.removeFromSuperview();
        self.sidebar_opaque_backing.removeFromSuperview();
        NSLayoutConstraint::deactivateConstraints(&self.main_constraints);
        NSLayoutConstraint::deactivateConstraints(&self.sidebar_constraints);
        NSLayoutConstraint::deactivateConstraints(&self.sidebar_material_constraints);
        NSLayoutConstraint::deactivateConstraints(&self.sidebar_opaque_constraints);
        NSLayoutConstraint::deactivateConstraints(&self.sidebar_overlay_constraints);
        NSLayoutConstraint::deactivateConstraints(&self.overlay_sidebar_constraints);
        NSLayoutConstraint::deactivateConstraints(&self.overlay_material_constraints);
        NSLayoutConstraint::deactivateConstraints(&self.overlay_opaque_constraints);
        self.sidebar_overlay.removeFromSuperview();
        self.split_controller.removeSplitViewItem(&self.main_item);
        self.split_controller
            .removeSplitViewItem(&self.sidebar_item);

        if let Some(controller) = self.original_controller.as_deref() {
            self.window.setContentViewController(Some(controller));
        } else {
            self.window.setContentView(Some(&self.original_parent));
        }

        self.original_parent.addSubview(&self.main_view);
        self.main_view.setFrame(self.original_frame);
        self.main_view
            .setAutoresizingMask(self.original_autoresizing);
        self.main_view
            .setTranslatesAutoresizingMaskIntoConstraints(true);

        self.sidebar_webview
    }
}

fn make_sidebar_material(
    mtm: MainThreadMarker,
    parent: &NSView,
) -> (
    Retained<NSVisualEffectView>,
    Retained<NSArray<NSLayoutConstraint>>,
) {
    let material: Retained<NSVisualEffectView> =
        unsafe { msg_send![NSVisualEffectView::alloc(mtm), initWithFrame: parent.bounds()] };
    material.setMaterial(NSVisualEffectMaterial::Sidebar);
    material.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
    material.setState(NSVisualEffectState::FollowsWindowActiveState);
    let constraints = pin_to_parent(&material, parent);
    (material, constraints)
}

fn make_opaque_backing(
    mtm: MainThreadMarker,
    parent: &NSView,
) -> (Retained<NSBox>, Retained<NSArray<NSLayoutConstraint>>) {
    let backing = NSBox::initWithFrame(NSBox::alloc(mtm), parent.bounds());
    backing.setBoxType(NSBoxType::Custom);
    backing.setBorderWidth(0.0);
    backing.setHidden(true);
    let constraints = pin_to_parent(&backing, parent);
    (backing, constraints)
}

fn pin_to_parent(child: &NSView, parent: &NSView) -> Retained<NSArray<NSLayoutConstraint>> {
    child.setTranslatesAutoresizingMaskIntoConstraints(false);
    parent.addSubview(child);
    let constraints = constraints_to_parent(child, parent);
    NSLayoutConstraint::activateConstraints(&constraints);
    constraints
}

fn constraints_to_parent(child: &NSView, parent: &NSView) -> Retained<NSArray<NSLayoutConstraint>> {
    let constraints = NSArray::from_retained_slice(&[
        child
            .leadingAnchor()
            .constraintEqualToAnchor(&parent.leadingAnchor()),
        child
            .trailingAnchor()
            .constraintEqualToAnchor(&parent.trailingAnchor()),
        child
            .topAnchor()
            .constraintEqualToAnchor(&parent.topAnchor()),
        child
            .bottomAnchor()
            .constraintEqualToAnchor(&parent.bottomAnchor()),
    ]);
    constraints
}

fn install_native_split(
    main_raw: usize,
    sidebar_raw: usize,
    window_raw: usize,
    sidebar_webview: tauri::Webview,
) -> Result<NativeSplitHost, String> {
    let mtm = MainThreadMarker::new()
        .ok_or_else(|| "native split installation must run on the main thread".to_string())?;
    let window = unsafe { Retained::retain(window_raw as *mut NSWindow) }
        .ok_or_else(|| "NSWindow was released before native split installation".to_string())?;
    let main_view = unsafe { Retained::retain(main_raw as *mut NSView) }
        .ok_or_else(|| "main WKWebView was released before installation".to_string())?;
    let sidebar_view = unsafe { Retained::retain(sidebar_raw as *mut NSView) }
        .ok_or_else(|| "sidebar WKWebView was released before installation".to_string())?;
    let original_parent = unsafe { main_view.superview() }
        .ok_or_else(|| "main WKWebView has no original parent".to_string())?;
    let original_controller = window.contentViewController();
    let original_window_frame = window.frame();
    let original_frame = main_view.frame();
    let original_autoresizing = main_view.autoresizingMask();

    main_view.removeFromSuperview();
    sidebar_view.removeFromSuperview();

    let sidebar_controller = NSViewController::new(mtm);
    let sidebar_pane = NSView::new(mtm);
    sidebar_controller.setView(&sidebar_pane);
    let (sidebar_opaque_backing, sidebar_opaque_constraints) =
        make_opaque_backing(mtm, &sidebar_pane);
    let (sidebar_material, sidebar_material_constraints) =
        make_sidebar_material(mtm, &sidebar_pane);
    let sidebar_constraints = pin_to_parent(&sidebar_view, &sidebar_pane);

    let main_controller = NSViewController::new(mtm);
    let main_pane = NSView::new(mtm);
    main_controller.setView(&main_pane);
    let main_constraints = pin_to_parent(&main_view, &main_pane);

    let sidebar_item = NSSplitViewItem::sidebarWithViewController(&sidebar_controller);
    sidebar_item.setMinimumThickness(SIDEBAR_MIN_WIDTH);
    sidebar_item.setMaximumThickness(SIDEBAR_MAX_WIDTH);
    let main_item = NSSplitViewItem::splitViewItemWithViewController(&main_controller);
    let split_controller = NSSplitViewController::new(mtm);
    split_controller.addSplitViewItem(&sidebar_item);
    split_controller.addSplitViewItem(&main_item);
    let split_view = split_controller.splitView();
    split_view.setPosition_ofDividerAtIndex(SIDEBAR_INITIAL_WIDTH, 0);
    split_view.setAutosaveName(Some(ns_string!("RózsaNativeSidebarSplit")));

    window.setContentViewController(Some(&split_controller));
    let split_root = split_controller.view();
    let sidebar_overlay = NSView::new(mtm);
    sidebar_overlay.setHidden(true);
    sidebar_overlay.setTranslatesAutoresizingMaskIntoConstraints(false);
    split_root.addSubview(&sidebar_overlay);
    let sidebar_overlay_width_constraint = sidebar_overlay
        .widthAnchor()
        .constraintEqualToConstant(SIDEBAR_INITIAL_WIDTH);
    let sidebar_overlay_constraints = NSArray::from_retained_slice(&[
        sidebar_overlay
            .leadingAnchor()
            .constraintEqualToAnchor(&split_root.leadingAnchor()),
        sidebar_overlay
            .topAnchor()
            .constraintEqualToAnchor(&split_root.topAnchor()),
        sidebar_overlay
            .bottomAnchor()
            .constraintEqualToAnchor(&split_root.bottomAnchor()),
        sidebar_overlay_width_constraint.clone(),
    ]);
    NSLayoutConstraint::activateConstraints(&sidebar_overlay_constraints);
    let overlay_sidebar_constraints = constraints_to_parent(&sidebar_view, &sidebar_overlay);
    let overlay_material_constraints = constraints_to_parent(&sidebar_material, &sidebar_overlay);
    let overlay_opaque_constraints =
        constraints_to_parent(&sidebar_opaque_backing, &sidebar_overlay);
    split_controller.view().setHidden(true);
    window.setFrame_display(original_window_frame, true);

    eprintln!(
        "[rozsa-gui][native-split] installed main={main_raw:#x} sidebar={sidebar_raw:#x} window={window_raw:#x}"
    );

    Ok(NativeSplitHost {
        window,
        original_controller,
        original_parent,
        original_frame,
        original_autoresizing,
        main_view,
        sidebar_view,
        sidebar_pane,
        sidebar_overlay,
        sidebar_material,
        sidebar_opaque_backing,
        main_constraints,
        sidebar_constraints,
        sidebar_material_constraints,
        sidebar_opaque_constraints,
        sidebar_overlay_constraints,
        sidebar_overlay_width_constraint,
        overlay_sidebar_constraints,
        overlay_material_constraints,
        overlay_opaque_constraints,
        split_controller,
        sidebar_item,
        main_item,
        _sidebar_controller: sidebar_controller,
        _main_controller: main_controller,
        sidebar_webview,
        theme_revision: 0,
        sidebar_overlay_visible: false,
    })
}

fn close_sidebar_async(sidebar: tauri::Webview, context: &'static str) {
    std::thread::spawn(move || match sidebar.close() {
        Ok(()) => eprintln!("[rozsa-gui][native-split] {context}: child WebView closed"),
        Err(error) => {
            eprintln!("[rozsa-gui][native-split] {context}: failed to close child WebView: {error}")
        }
    });
}

fn record_webview(
    pending: &Arc<Mutex<PendingInstall>>,
    role: &'static str,
    view_raw: usize,
    window_raw: usize,
    sidebar_webview: &tauri::Webview,
    app: &AppHandle,
) {
    let ready = {
        let mut pending = pending.lock().expect("native split pending mutex poisoned");
        match role {
            "main" => pending.main_view = Some(view_raw),
            "sidebar" => pending.sidebar_view = Some(view_raw),
            _ => unreachable!("unknown native split WebView role"),
        }
        pending.window = Some(window_raw);
        if pending.started {
            None
        } else if let (Some(main), Some(sidebar), Some(window)) =
            (pending.main_view, pending.sidebar_view, pending.window)
        {
            pending.started = true;
            Some((main, sidebar, window, pending.on_installed.take()))
        } else {
            None
        }
    };

    let Some((main, sidebar, window, on_installed)) = ready else {
        return;
    };

    match install_native_split(main, sidebar, window, sidebar_webview.clone()) {
        Ok(host) => {
            let duplicate_sidebar = HOST.with(|slot| {
                let mut slot = slot.borrow_mut();
                if slot.is_some() {
                    Some(host.restore_hierarchy())
                } else {
                    *slot = Some(host);
                    None
                }
            });
            if let Some(sidebar) = duplicate_sidebar {
                eprintln!("[rozsa-gui][native-split] duplicate host installation");
                close_sidebar_async(sidebar, "duplicate installation rollback");
                app.exit(1);
                return;
            }
            if let Some(on_installed) = on_installed
                && let Err(error) = on_installed(main)
            {
                eprintln!("[rozsa-gui][native-split] post-install setup failed: {error}");
                let sidebar = HOST.with(|slot| {
                    slot.borrow_mut()
                        .take()
                        .map(NativeSplitHost::restore_hierarchy)
                });
                if let Some(sidebar) = sidebar {
                    close_sidebar_async(sidebar, "post-install rollback");
                }
                app.exit(1);
            }
        }
        Err(error) => {
            eprintln!("[rozsa-gui][native-split] installation failed: {error}");
            close_sidebar_async(sidebar_webview.clone(), "installation rollback");
            app.exit(1);
        }
    }
}

/// Create the persistent sidebar WebView and schedule native split installation.
pub fn install(
    main: &WebviewWindow,
    sidebar_url: WebviewUrl,
    on_installed: impl FnOnce(usize) -> Result<(), String> + Send + 'static,
) -> Result<(), String> {
    MainThreadMarker::new()
        .ok_or_else(|| "native split setup must start on the main thread".to_string())?;
    let host_window = main.as_ref().window();
    let sidebar = host_window
        .add_child(
            WebviewBuilder::new("sidebar", sidebar_url)
                .transparent(true)
                .devtools(cfg!(debug_assertions)),
            LogicalPosition::new(0, 0),
            LogicalSize::new(SIDEBAR_INITIAL_WIDTH, 720.0),
        )
        .map_err(|error| format!("failed to create sidebar WebView: {error}"))?;
    let pending = Arc::new(Mutex::new(PendingInstall {
        on_installed: Some(Box::new(on_installed)),
        ..PendingInstall::default()
    }));
    let app = main.app_handle().clone();

    let main_pending = Arc::clone(&pending);
    let main_sidebar = sidebar.clone();
    let main_app = app.clone();
    if let Err(error) = main.with_webview(move |platform| {
        record_webview(
            &main_pending,
            "main",
            platform.inner() as usize,
            platform.ns_window() as usize,
            &main_sidebar,
            &main_app,
        );
    }) {
        close_sidebar_async(sidebar.clone(), "main handle failure");
        return Err(format!("failed to access main WKWebView: {error}"));
    }

    let sidebar_pending = Arc::clone(&pending);
    let sidebar_handle = sidebar.clone();
    sidebar
        .with_webview(move |platform| {
            record_webview(
                &sidebar_pending,
                "sidebar",
                platform.inner() as usize,
                platform.ns_window() as usize,
                &sidebar_handle,
                &app,
            );
        })
        .map_err(|error| {
            close_sidebar_async(sidebar.clone(), "sidebar handle failure");
            format!("failed to access sidebar WKWebView: {error}")
        })?;

    Ok(())
}

/// Toggle the single AppKit-owned sidebar used by every GUI scene.
pub fn toggle_sidebar() -> Result<bool, String> {
    MainThreadMarker::new()
        .ok_or_else(|| "native sidebar toggle must run on the main thread".to_string())?;
    HOST.with(|slot| {
        let mut slot = slot.borrow_mut();
        let host = slot
            .as_mut()
            .ok_or_else(|| "native split host is not installed".to_string())?;
        if !host.sidebar_item.isCollapsed() {
            let expanded_width = host.sidebar_pane.frame().size.width;
            if expanded_width >= SIDEBAR_MIN_WIDTH {
                host.sidebar_overlay_width_constraint
                    .setConstant(expanded_width);
            }
        }
        host.set_sidebar_overlay_visible(false)?;
        unsafe { host.split_controller.toggleSidebar(None::<&AnyObject>) };
        Ok(host.sidebar_item.isCollapsed())
    })
}

/// Show or hide the existing sidebar WebView as a leading overlay while the
/// AppKit sidebar item remains collapsed.
pub fn set_sidebar_overlay_visible(visible: bool) -> Result<(), String> {
    MainThreadMarker::new()
        .ok_or_else(|| "native sidebar overlay must update on the main thread".to_string())?;
    HOST.with(|slot| {
        slot.borrow_mut()
            .as_mut()
            .ok_or_else(|| "native split host is not installed".to_string())?
            .set_sidebar_overlay_visible(visible)
    })
}

/// Read the AppKit-owned collapsed state without changing it.
pub fn is_sidebar_collapsed() -> Result<bool, String> {
    MainThreadMarker::new()
        .ok_or_else(|| "native sidebar state must be read on the main thread".to_string())?;
    HOST.with(|slot| {
        slot.borrow()
            .as_ref()
            .map(|host| host.sidebar_item.isCollapsed())
            .ok_or_else(|| "native split host is not installed".to_string())
    })
}

/// Reveal both WebView panes together after their frontend roots are ready.
pub fn reveal_content() -> Result<(), String> {
    MainThreadMarker::new()
        .ok_or_else(|| "native split reveal must run on the main thread".to_string())?;
    HOST.with(|slot| {
        let slot = slot.borrow();
        let host = slot
            .as_ref()
            .ok_or_else(|| "native split host is not installed".to_string())?;
        host.split_controller.view().setHidden(false);
        Ok(())
    })
}

fn window_uses_dark_appearance(window: &NSWindow) -> bool {
    let appearances = vec![
        NSString::from_str("NSAppearanceNameAqua"),
        NSString::from_str("NSAppearanceNameDarkAqua"),
    ];
    let names = NSArray::from_retained_slice(&appearances);
    window
        .effectiveAppearance()
        .bestMatchFromAppearancesWithNames(&names)
        .is_some_and(|name| name.to_string() == "NSAppearanceNameDarkAqua")
}

fn apply_sidebar_appearance(material: &NSVisualEffectView, theme_mode: &str) -> Result<(), String> {
    let appearance_name = match theme_mode {
        "light" => Some("NSAppearanceNameAqua"),
        "dark" => Some("NSAppearanceNameDarkAqua"),
        "system" => None,
        other => return Err(format!("unknown native theme mode: {other}")),
    };
    let Some(appearance_name) = appearance_name else {
        material.setAppearance(None);
        return Ok(());
    };
    let name = NSString::from_str(appearance_name);
    let appearance = NSAppearance::appearanceNamed(&name)
        .ok_or_else(|| format!("AppKit appearance is unavailable: {appearance_name}"))?;
    material.setAppearance(Some(&appearance));
    Ok(())
}

fn parse_hex_color(value: &str) -> Option<(f64, f64, f64, f64)> {
    let hex = value.strip_prefix('#')?;
    let expanded;
    let hex = match hex.len() {
        3 | 4 => {
            expanded = hex
                .chars()
                .flat_map(|character| [character, character])
                .collect::<String>();
            expanded.as_str()
        }
        6 | 8 => hex,
        _ => return None,
    };
    let component = |offset| u8::from_str_radix(&hex[offset..offset + 2], 16).ok();
    let red = component(0)? as f64 / 255.0;
    let green = component(2)? as f64 / 255.0;
    let blue = component(4)? as f64 / 255.0;
    let alpha = if hex.len() == 8 {
        component(6)? as f64 / 255.0
    } else {
        1.0
    };
    Some((red, green, blue, alpha))
}

fn parse_rgb_component(value: &str) -> Option<f64> {
    if let Some(percent) = value.strip_suffix('%') {
        return Some(percent.trim().parse::<f64>().ok()? / 100.0);
    }
    Some(value.trim().parse::<f64>().ok()? / 255.0)
}

fn parse_alpha(value: &str) -> Option<f64> {
    if let Some(percent) = value.strip_suffix('%') {
        return Some(percent.trim().parse::<f64>().ok()? / 100.0);
    }
    value.trim().parse::<f64>().ok()
}

fn parse_rgb_color(value: &str) -> Option<(f64, f64, f64, f64)> {
    let contents = value
        .strip_prefix("rgb(")
        .or_else(|| value.strip_prefix("rgba("))?
        .strip_suffix(')')?;
    let normalized = contents.replace([',', '/'], " ");
    let components = normalized.split_whitespace().collect::<Vec<_>>();
    if !(3..=4).contains(&components.len()) {
        return None;
    }
    Some((
        parse_rgb_component(components[0])?,
        parse_rgb_component(components[1])?,
        parse_rgb_component(components[2])?,
        components
            .get(3)
            .and_then(|value| parse_alpha(value))
            .unwrap_or(1.0),
    ))
}

fn linear_to_srgb(component: f64) -> f64 {
    if component <= 0.003_130_8 {
        12.92 * component
    } else {
        1.055 * component.powf(1.0 / 2.4) - 0.055
    }
}

fn parse_oklch_color(value: &str) -> Option<(f64, f64, f64, f64)> {
    let contents = value.strip_prefix("oklch(")?.strip_suffix(')')?;
    let normalized = contents.replace('/', " ");
    let components = normalized.split_whitespace().collect::<Vec<_>>();
    if !(3..=4).contains(&components.len()) {
        return None;
    }
    let lightness = components[0].strip_suffix('%')?.parse::<f64>().ok()? / 100.0;
    let chroma = components[1].parse::<f64>().ok()?;
    let hue = components[2].parse::<f64>().ok()?.to_radians();
    let alpha = components
        .get(3)
        .and_then(|value| parse_alpha(value))
        .unwrap_or(1.0);
    let a = chroma * hue.cos();
    let b = chroma * hue.sin();
    let l_root = lightness + 0.396_337_777_4 * a + 0.215_803_757_3 * b;
    let m_root = lightness - 0.105_561_345_8 * a - 0.063_854_172_8 * b;
    let s_root = lightness - 0.089_484_177_5 * a - 1.291_485_548 * b;
    let l = l_root.powi(3);
    let m = m_root.powi(3);
    let s = s_root.powi(3);
    Some((
        linear_to_srgb(4.076_741_662_1 * l - 3.307_711_591_3 * m + 0.230_969_929_2 * s),
        linear_to_srgb(-1.268_438_004_6 * l + 2.609_757_401_1 * m - 0.341_319_396_5 * s),
        linear_to_srgb(-0.004_196_086_3 * l - 0.703_418_614_7 * m + 1.707_614_701 * s),
        alpha,
    ))
}

fn parse_sidebar_color(value: &str) -> Result<Retained<NSColor>, String> {
    let normalized = value.trim().to_ascii_lowercase();
    let components = parse_hex_color(&normalized)
        .or_else(|| parse_rgb_color(&normalized))
        .or_else(|| parse_oklch_color(&normalized))
        .ok_or_else(|| {
            format!(
                "unsupported native sidebar color {value:?}; use hex, rgb(), rgba(), or oklch()"
            )
        })?;
    Ok(NSColor::colorWithSRGBRed_green_blue_alpha(
        components.0.clamp(0.0, 1.0),
        components.1.clamp(0.0, 1.0),
        components.2.clamp(0.0, 1.0),
        components.3.clamp(0.0, 1.0),
    ))
}

/// Apply a revisioned AppKit sidebar surface before the matching WebView event.
pub fn apply_theme_surface(revision: u64, surface: NativeThemeSurface) -> Result<(), String> {
    MainThreadMarker::new()
        .ok_or_else(|| "native theme surface must update on the main thread".to_string())?;
    HOST.with(|slot| {
        let mut slot = slot.borrow_mut();
        let host = slot
            .as_mut()
            .ok_or_else(|| "native split host is not installed".to_string())?;
        if revision <= host.theme_revision {
            return Ok(());
        }
        apply_sidebar_appearance(&host.sidebar_material, &surface.theme_mode)?;
        let variant = match surface.theme_mode.as_str() {
            "light" => &surface.light,
            "dark" => &surface.dark,
            "system" if window_uses_dark_appearance(&host.window) => &surface.dark,
            "system" => &surface.light,
            other => return Err(format!("unknown native theme mode: {other}")),
        };
        if variant.translucent {
            host.sidebar_opaque_backing.setHidden(true);
            host.sidebar_material.setHidden(false);
        } else {
            let color = parse_sidebar_color(&variant.opaque_color)?;
            host.sidebar_opaque_backing.setFillColor(&color);
            host.sidebar_material.setHidden(true);
            host.sidebar_opaque_backing.setHidden(false);
        }
        host.theme_revision = revision;
        Ok(())
    })
}

/// Restore the original hierarchy before the window and child WebView close.
pub fn teardown() -> Result<(), String> {
    MainThreadMarker::new()
        .ok_or_else(|| "native split teardown must run on the main thread".to_string())?;
    let sidebar = HOST.with(|slot| {
        slot.borrow_mut()
            .take()
            .map(NativeSplitHost::restore_hierarchy)
    });
    if let Some(sidebar) = sidebar {
        close_sidebar_async(sidebar, "teardown");
    }
    Ok(())
}
