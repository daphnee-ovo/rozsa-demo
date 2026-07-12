//! Native macOS titlebar integration.
//!
//! The webview owns the content and sidebar layout. AppKit owns the window
//! chrome, while this module adds only the sidebar accessory action required
//! by the GUI prototype.

#![cfg(target_os = "macos")]

use objc2::runtime::{AnyObject, NSObject};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, Message, define_class, msg_send, sel};
use objc2_app_kit::{
    NSAutoresizingMaskOptions, NSButton, NSEvent, NSImage, NSImageScaling, NSLayoutAttribute,
    NSTitlebarAccessoryViewController, NSView, NSWindow, NSWindowDidEnterFullScreenNotification,
    NSWindowDidExitFullScreenNotification, NSWindowDidResizeNotification, NSWindowStyleMask,
    NSWindowTitleVisibility,
};
use objc2_foundation::{NSNotification, NSNotificationCenter, NSPoint, NSRect, NSSize, ns_string};
use tauri::WebviewWindow;

const TITLEBAR_ACCESSORY_HEIGHT: f64 = 32.0;

struct TitlebarActionIvars {
    on_toggle: Box<dyn Fn() + Send + Sync + 'static>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = TitlebarActionIvars]
    struct TitlebarActionTarget;

    impl TitlebarActionTarget {
        #[unsafe(method(toggleSidebar:))]
        fn toggle_sidebar(&self, _sender: Option<&AnyObject>) {
            (self.ivars().on_toggle)();
        }
    }
);

impl TitlebarActionTarget {
    fn new(
        mtm: MainThreadMarker,
        on_toggle: impl Fn() + Send + Sync + 'static,
    ) -> objc2::rc::Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(TitlebarActionIvars {
            on_toggle: Box::new(on_toggle),
        });
        unsafe { msg_send![super(this), init] }
    }
}

struct TitlebarDragViewIvars;

define_class!(
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[ivars = TitlebarDragViewIvars]
    struct TitlebarDragView;

    impl TitlebarDragView {
        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            if let Some(window) = self.window() {
                window.performWindowDragWithEvent(event);
            }
        }
    }
);

impl TitlebarDragView {
    fn new(mtm: MainThreadMarker) -> objc2::rc::Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(TitlebarDragViewIvars);
        unsafe { msg_send![super(this), init] }
    }
}

struct FullscreenObserverIvars {
    window: objc2::rc::Weak<NSWindow>,
    webview_parent: objc2::rc::Weak<NSView>,
    drag_view: objc2::rc::Retained<TitlebarDragView>,
    on_fullscreen: Box<dyn Fn(bool) + Send + Sync + 'static>,
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
            .map(|view| (view.frame(), view.safeAreaInsets(), view.additionalSafeAreaInsets()))
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
        #[unsafe(method(rozsaWindowDidEnterFullScreen:))]
        fn did_enter_fullscreen(&self, _notification: &NSNotification) {
            self.ivars().drag_view.setHidden(true);
            let current_frame = self.ivars().drag_view.frame();
            self.ivars()
                .drag_view
                .setFrameSize(NSSize::new(current_frame.size.width, 0.0));
            if let Some(window) = self.ivars().window.load() {
                let webview_parent = self.ivars().webview_parent.load();
                log_window_geometry("did-enter-fullscreen", &window, webview_parent.as_deref());
            }
            (self.ivars().on_fullscreen)(true);
        }

        #[unsafe(method(rozsaWindowDidExitFullScreen:))]
        fn did_exit_fullscreen(&self, _notification: &NSNotification) {
            let current_frame = self.ivars().drag_view.frame();
            self.ivars().drag_view.setFrameSize(NSSize::new(
                current_frame.size.width,
                TITLEBAR_ACCESSORY_HEIGHT,
            ));
            self.ivars().drag_view.setHidden(false);
            if let Some(window) = self.ivars().window.load() {
                let webview_parent = self.ivars().webview_parent.load();
                log_window_geometry("did-exit-fullscreen", &window, webview_parent.as_deref());
            }
            (self.ivars().on_fullscreen)(false);
        }

        #[unsafe(method(rozsaWindowDidResize:))]
        fn did_resize(&self, _notification: &NSNotification) {
            let current_frame = self.ivars().drag_view.frame();
            if let Some(window) = self.ivars().window.load() {
                let width = window.frame().size.width.max(640.0);
                if (current_frame.size.width - width).abs() > 0.5 {
                    self.ivars()
                        .drag_view
                        .setFrameSize(NSSize::new(width, current_frame.size.height));
                }
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
        drag_view: objc2::rc::Retained<TitlebarDragView>,
        on_fullscreen: impl Fn(bool) + Send + Sync + 'static,
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

struct TitlebarAccessoryIvars {
    // Retain the target because NSControl's target property is weak.
    #[allow(dead_code)]
    target: objc2::rc::Retained<TitlebarActionTarget>,
    // NSNotificationCenter does not retain selector observers.
    #[allow(dead_code)]
    fullscreen_observer: objc2::rc::Retained<FullscreenObserver>,
}

define_class!(
    #[unsafe(super(NSTitlebarAccessoryViewController))]
    #[thread_kind = MainThreadOnly]
    #[ivars = TitlebarAccessoryIvars]
    struct TitlebarAccessoryController;
);

impl TitlebarAccessoryController {
    fn new(
        mtm: MainThreadMarker,
        target: objc2::rc::Retained<TitlebarActionTarget>,
        fullscreen_observer: objc2::rc::Retained<FullscreenObserver>,
    ) -> objc2::rc::Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(TitlebarAccessoryIvars {
            target,
            fullscreen_observer,
        });
        unsafe { msg_send![super(this), init] }
    }
}

/// Install the native traffic-light-compatible sidebar accessory and unify
/// the webview content with the titlebar without removing native decorations.
pub fn install(
    window: &WebviewWindow,
    on_toggle: impl Fn() + Send + Sync + 'static,
    on_fullscreen: impl Fn(bool) + Send + Sync + 'static,
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

    let target = TitlebarActionTarget::new(mtm, on_toggle);
    let drag_view = TitlebarDragView::new(mtm);
    let titlebar_width = ns_window.frame().size.width.max(640.0);
    drag_view.setFrame(NSRect::new(
        NSPoint::new(0.0, 0.0),
        NSSize::new(titlebar_width, TITLEBAR_ACCESSORY_HEIGHT),
    ));
    drag_view.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable,
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
        sidebar_button.setTarget(Some(target.as_ref() as &AnyObject));
        sidebar_button.setAction(Some(sel!(toggleSidebar:)));
    }
    drag_view.addSubview(&sidebar_button);

    let window_weak = objc2::rc::Weak::from_retained(&retained_window);
    let webview_parent_weak = objc2::rc::Weak::from_retained(&retained_webview_parent);
    let fullscreen_observer = FullscreenObserver::new(
        mtm,
        window_weak,
        webview_parent_weak,
        drag_view.retain(),
        on_fullscreen,
    );
    let notification_center = NSNotificationCenter::defaultCenter();
    unsafe {
        let observer = fullscreen_observer.as_ref() as &AnyObject;
        let window_object = ns_window as &AnyObject;
        notification_center.addObserver_selector_name_object(
            observer,
            sel!(rozsaWindowDidEnterFullScreen:),
            Some(NSWindowDidEnterFullScreenNotification),
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

    let controller = TitlebarAccessoryController::new(mtm, target, fullscreen_observer);
    controller.setView(&drag_view);
    controller.setLayoutAttribute(NSLayoutAttribute::Leading);
    controller.setAutomaticallyAdjustsSize(false);
    ns_window.addTitlebarAccessoryViewController(&controller);

    let mut style_mask = ns_window.styleMask();
    style_mask.insert(NSWindowStyleMask::FullSizeContentView);
    ns_window.setStyleMask(style_mask);
    ns_window.setTitlebarAppearsTransparent(true);
    ns_window.setTitleVisibility(NSWindowTitleVisibility::Hidden);
    ns_window.setTitlebarSeparatorStyle(objc2_app_kit::NSTitlebarSeparatorStyle::None);
    log_window_geometry("installed", ns_window, Some(webview_parent));

    Ok(())
}
