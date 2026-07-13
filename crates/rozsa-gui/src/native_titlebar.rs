//! Native macOS titlebar integration.
//!
//! The webview owns the content and sidebar layout. AppKit owns the window
//! chrome, while this module adds only the sidebar accessory action required
//! by the GUI prototype.

#![cfg(target_os = "macos")]

use std::cell::RefCell;

use objc2::runtime::{AnyObject, NSObject};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSAutoresizingMaskOptions, NSButton, NSColor, NSEvent, NSImage, NSImageScaling, NSView,
    NSVisualEffectBlendingMode, NSVisualEffectMaterial, NSVisualEffectState, NSVisualEffectView,
    NSWindow, NSWindowDidEnterFullScreenNotification, NSWindowDidExitFullScreenNotification,
    NSWindowDidResizeNotification, NSWindowOrderingMode, NSWindowStyleMask,
    NSWindowTitleVisibility, NSWindowWillEnterFullScreenNotification,
    NSWindowWillExitFullScreenNotification,
};
use objc2_foundation::{NSNotification, NSNotificationCenter, NSPoint, NSRect, NSSize, ns_string};
use tauri::WebviewWindow;

const TITLEBAR_ACCESSORY_HEIGHT: f64 = 32.0;
const TITLEBAR_LEADING_INSET: f64 = 76.0;

fn install_sidebar_material(mtm: MainThreadMarker, webview_parent: &NSView) {
    let material: objc2::rc::Retained<NSVisualEffectView> = unsafe {
        msg_send![
            NSVisualEffectView::alloc(mtm),
            initWithFrame: webview_parent.bounds()
        ]
    };
    material.setMaterial(NSVisualEffectMaterial::Sidebar);
    material.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
    material.setState(NSVisualEffectState::FollowsWindowActiveState);
    material.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable,
    );
    webview_parent.addSubview_positioned_relativeTo(&material, NSWindowOrderingMode::Below, None);
}

struct TitlebarDragViewIvars {
    on_toggle: Box<dyn Fn() + Send + Sync + 'static>,
    fullscreen_observer: RefCell<Option<objc2::rc::Retained<FullscreenObserver>>>,
}

define_class!(
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[ivars = TitlebarDragViewIvars]
    struct TitlebarDragView;

    impl TitlebarDragView {
        #[unsafe(method(toggleSidebar:))]
        fn toggle_sidebar(&self, _sender: Option<&AnyObject>) {
            (self.ivars().on_toggle)();
        }
        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            if let Some(window) = self.window() {
                if event.clickCount() == 2 {
                    window.performZoom(Some(self as &AnyObject));
                } else {
                    window.performWindowDragWithEvent(event);
                }
            }
        }
    }
);

impl TitlebarDragView {
    fn new(
        mtm: MainThreadMarker,
        on_toggle: impl Fn() + Send + Sync + 'static,
    ) -> objc2::rc::Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(TitlebarDragViewIvars {
            on_toggle: Box::new(on_toggle),
            fullscreen_observer: RefCell::new(None),
        });
        unsafe { msg_send![super(this), init] }
    }

    fn retain_fullscreen_observer(&self, observer: objc2::rc::Retained<FullscreenObserver>) {
        self.ivars().fullscreen_observer.replace(Some(observer));
    }
}

struct FullscreenObserverIvars {
    window: objc2::rc::Weak<NSWindow>,
    webview_parent: objc2::rc::Weak<NSView>,
    drag_view: objc2::rc::Weak<TitlebarDragView>,
    on_fullscreen: Box<dyn Fn(bool, bool) + Send + Sync + 'static>,
}

fn log_window_geometry(label: &str, window: &NSWindow, webview_parent: Option<&NSView>) {
    let frame = window.frame();
    let layout = window.contentLayoutRect();
    let content_frame = window.contentView().map(|view| view.frame());
    let webview_parent_frame = webview_parent.map(NSView::frame);
    let webview_parent_safe_area = webview_parent.map(NSView::safeAreaInsets);
    let webview_child_geometry = webview_parent.map(|parent| {
        parent
            .subviews()
            .iter()
            .map(|view| {
                (
                    view.frame(),
                    view.safeAreaInsets(),
                    view.additionalSafeAreaInsets(),
                )
            })
            .collect::<Vec<_>>()
    });
    eprintln!(
        "[rozsa-gui][native-titlebar] {label} window={frame:?} content_layout={layout:?} content_view={content_frame:?} webview_parent={webview_parent_frame:?} parent_safe_area={webview_parent_safe_area:?} webview_children={webview_child_geometry:?}"
    );
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = FullscreenObserverIvars]
    struct FullscreenObserver;

    impl FullscreenObserver {
        #[unsafe(method(rozsaWindowWillEnterFullScreen:))]
        fn will_enter_fullscreen(&self, _notification: &NSNotification) {
            if let Some(drag_view) = self.ivars().drag_view.load() {
                drag_view.setHidden(true);
            }
            (self.ivars().on_fullscreen)(true, true);
        }

        #[unsafe(method(rozsaWindowDidEnterFullScreen:))]
        fn did_enter_fullscreen(&self, _notification: &NSNotification) {
            if let Some(window) = self.ivars().window.load() {
                let webview_parent = self.ivars().webview_parent.load();
                log_window_geometry("did-enter-fullscreen", &window, webview_parent.as_deref());
            }
            (self.ivars().on_fullscreen)(true, false);
        }

        #[unsafe(method(rozsaWindowWillExitFullScreen:))]
        fn will_exit_fullscreen(&self, _notification: &NSNotification) {
            (self.ivars().on_fullscreen)(false, true);
        }

        #[unsafe(method(rozsaWindowDidExitFullScreen:))]
        fn did_exit_fullscreen(&self, _notification: &NSNotification) {
            if let Some(drag_view) = self.ivars().drag_view.load() {
                drag_view.setHidden(false);
            }
            if let Some(window) = self.ivars().window.load() {
                let webview_parent = self.ivars().webview_parent.load();
                log_window_geometry("did-exit-fullscreen", &window, webview_parent.as_deref());
            }
            (self.ivars().on_fullscreen)(false, false);
        }

        #[unsafe(method(rozsaWindowDidResize:))]
        fn did_resize(&self, _notification: &NSNotification) {
            if let Some(window) = self.ivars().window.load() {
                let webview_parent = self.ivars().webview_parent.load();
                log_window_geometry("did-resize", &window, webview_parent.as_deref());
            }
        }
    }
);

impl FullscreenObserver {
    fn new(
        mtm: MainThreadMarker,
        window: objc2::rc::Weak<NSWindow>,
        webview_parent: objc2::rc::Weak<NSView>,
        drag_view: objc2::rc::Weak<TitlebarDragView>,
        on_fullscreen: impl Fn(bool, bool) + Send + Sync + 'static,
    ) -> objc2::rc::Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(FullscreenObserverIvars {
            window,
            webview_parent,
            drag_view,
            on_fullscreen: Box::new(on_fullscreen),
        });
        unsafe { msg_send![super(this), init] }
    }
}

/// Install the native traffic-light-compatible sidebar accessory and unify
/// the webview content with the titlebar without removing native decorations.
pub fn install(
    window: &WebviewWindow,
    on_toggle: impl Fn() + Send + Sync + 'static,
    on_fullscreen: impl Fn(bool, bool) + Send + Sync + 'static,
) -> Result<(), String> {
    let raw_window = window
        .ns_window()
        .map_err(|error| format!("failed to access native NSWindow: {error}"))?;
    let ns_window = unsafe { &*(raw_window as *const NSWindow) };
    let mtm = MainThreadMarker::new()
        .ok_or_else(|| "native titlebar must be installed on the main thread".to_string())?;

    let retained_window =
        unsafe { objc2::rc::Retained::retain(ns_window as *const NSWindow as *mut NSWindow) }
            .ok_or_else(|| "native NSWindow was released during titlebar setup".to_string())?;
    let raw_webview_parent = window
        .ns_view()
        .map_err(|error| format!("failed to access native WebView parent: {error}"))?;
    let webview_parent = unsafe { &*(raw_webview_parent as *const NSView) };
    let retained_webview_parent =
        unsafe { objc2::rc::Retained::retain(webview_parent as *const NSView as *mut NSView) }
            .ok_or_else(|| {
                "native WebView parent was released during titlebar setup".to_string()
            })?;
    install_sidebar_material(mtm, webview_parent);

    let drag_view = TitlebarDragView::new(mtm, on_toggle);
    let webview_bounds = webview_parent.bounds();
    let titlebar_width = (webview_bounds.size.width - TITLEBAR_LEADING_INSET).max(1.0);
    drag_view.setFrame(NSRect::new(
        NSPoint::new(
            TITLEBAR_LEADING_INSET,
            (webview_bounds.size.height - TITLEBAR_ACCESSORY_HEIGHT).max(0.0),
        ),
        NSSize::new(titlebar_width, TITLEBAR_ACCESSORY_HEIGHT),
    ));
    drag_view.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewMinYMargin,
    );

    let sidebar_button = NSButton::new(mtm);
    sidebar_button.setFrame(NSRect::new(NSPoint::new(5.0, 4.0), NSSize::new(28.0, 24.0)));
    sidebar_button.setBordered(false);
    sidebar_button.setRefusesFirstResponder(true);
    sidebar_button.setToolTip(Some(ns_string!("Show or hide sidebar")));
    if let Some(image) = NSImage::imageWithSystemSymbolName_accessibilityDescription(
        ns_string!("rectangle.split.2x1"),
        Some(ns_string!("Sidebar")),
    ) {
        sidebar_button.setImage(Some(&image));
        sidebar_button.setImageScaling(NSImageScaling::ScaleProportionallyUpOrDown);
    }
    unsafe {
        sidebar_button.setTarget(Some(drag_view.as_ref() as &AnyObject));
        sidebar_button.setAction(Some(sel!(toggleSidebar:)));
    }
    drag_view.addSubview(&sidebar_button);
    webview_parent.addSubview_positioned_relativeTo(&drag_view, NSWindowOrderingMode::Above, None);

    let window_weak = objc2::rc::Weak::from_retained(&retained_window);
    let webview_parent_weak = objc2::rc::Weak::from_retained(&retained_webview_parent);
    let fullscreen_observer = FullscreenObserver::new(
        mtm,
        window_weak,
        webview_parent_weak,
        objc2::rc::Weak::from_retained(&drag_view),
        on_fullscreen,
    );
    drag_view.retain_fullscreen_observer(fullscreen_observer.clone());
    let notification_center = NSNotificationCenter::defaultCenter();
    unsafe {
        let observer = fullscreen_observer.as_ref() as &AnyObject;
        let window_object = ns_window as &AnyObject;
        notification_center.addObserver_selector_name_object(
            observer,
            sel!(rozsaWindowWillEnterFullScreen:),
            Some(NSWindowWillEnterFullScreenNotification),
            Some(window_object),
        );
        notification_center.addObserver_selector_name_object(
            observer,
            sel!(rozsaWindowDidEnterFullScreen:),
            Some(NSWindowDidEnterFullScreenNotification),
            Some(window_object),
        );
        notification_center.addObserver_selector_name_object(
            observer,
            sel!(rozsaWindowWillExitFullScreen:),
            Some(NSWindowWillExitFullScreenNotification),
            Some(window_object),
        );
        notification_center.addObserver_selector_name_object(
            observer,
            sel!(rozsaWindowDidExitFullScreen:),
            Some(NSWindowDidExitFullScreenNotification),
            Some(window_object),
        );
        notification_center.addObserver_selector_name_object(
            observer,
            sel!(rozsaWindowDidResize:),
            Some(NSWindowDidResizeNotification),
            Some(window_object),
        );
    }

    let mut style_mask = ns_window.styleMask();
    style_mask.insert(NSWindowStyleMask::FullSizeContentView);
    ns_window.setStyleMask(style_mask);
    ns_window.setTitlebarAppearsTransparent(true);
    ns_window.setTitleVisibility(NSWindowTitleVisibility::Hidden);
    ns_window.setTitlebarSeparatorStyle(objc2_app_kit::NSTitlebarSeparatorStyle::None);
    ns_window.setOpaque(false);
    ns_window.setBackgroundColor(Some(&NSColor::clearColor()));
    log_window_geometry("installed", ns_window, Some(webview_parent));

    Ok(())
}
