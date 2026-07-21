//! Opt-in cross-platform Web Inspector launch.
//!
//! `ROZSA_WEB_INSPECTOR=1` enables the Inspector. Non-macOS platforms then use
//! Wry's native DevTools window. On macOS, WebKit's
//! inspector receives a retained delegate after the main WKWebView has been
//! installed in the native split hierarchy. The delegate detaches from
//! `inspectorFrontendLoaded:` before WebKit's first bring-to-front operation,
//! so the frontend opens directly in its own window. Selector availability is
//! checked before sending private WebKit messages so an OS mismatch reports an
//! error instead of aborting the process.
//!
//! Related behavior: `docs/gui/NATIVE_SPLIT_VALIDATION.md`.

const INSPECTOR_ENV: &str = "ROZSA_WEB_INSPECTOR";

pub fn enabled() -> bool {
    std::env::var(INSPECTOR_ENV).is_ok_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

#[cfg(not(target_os = "macos"))]
use tauri::WebviewWindow;

#[cfg(not(target_os = "macos"))]
pub fn open_in_separate_window(window: &WebviewWindow) {
    window.open_devtools();
}

#[cfg(target_os = "macos")]
use std::cell::RefCell;

#[cfg(target_os = "macos")]
use objc2::rc::Retained;
#[cfg(target_os = "macos")]
use objc2::runtime::{AnyObject, NSObject};
#[cfg(target_os = "macos")]
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};

#[cfg(target_os = "macos")]
thread_local! {
    static HOST: RefCell<Option<InspectorHost>> = const { RefCell::new(None) };
}

#[cfg(target_os = "macos")]
define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    struct InspectorDelegate;

    impl InspectorDelegate {
        #[unsafe(method(inspectorFrontendLoaded:))]
        fn inspector_frontend_loaded(&self, inspector: &AnyObject) {
            unsafe {
                let _: () = msg_send![inspector, detach];
            }
            eprintln!("[rozsa-gui][inspector] frontend loaded and detached");
        }
    }
);

#[cfg(target_os = "macos")]
impl InspectorDelegate {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        unsafe { msg_send![Self::alloc(mtm), init] }
    }
}

#[cfg(target_os = "macos")]
struct InspectorHost {
    inspector: Retained<AnyObject>,
    _delegate: Retained<InspectorDelegate>,
}

#[cfg(target_os = "macos")]
pub fn open_from_webview_raw(webview_raw: usize) -> Result<(), String> {
    if webview_raw == 0 {
        return Err("main WKWebView handle is null".to_string());
    }
    if HOST.with(|slot| slot.borrow().is_some()) {
        return Err("WebKit Inspector is already configured".to_string());
    }
    let mtm = MainThreadMarker::new()
        .ok_or_else(|| "WebKit Inspector must open on the main thread".to_string())?;

    unsafe {
        let webview = &*(webview_raw as *const AnyObject);
        let inspector: Retained<AnyObject> = msg_send![webview, _inspector];

        for (selector, name) in [
            (sel!(setDelegate:), "setDelegate:"),
            (sel!(connect), "connect"),
            (sel!(show), "show"),
            (sel!(detach), "detach"),
        ] {
            let responds: bool = msg_send![&inspector, respondsToSelector: selector];
            if !responds {
                return Err(format!("WebKit Inspector does not support {name}"));
            }
        }

        let delegate = InspectorDelegate::new(mtm);
        let delegate_object: &AnyObject = &delegate;
        let _: () = msg_send![&inspector, setDelegate: delegate_object];
        let _: () = msg_send![&inspector, connect];
        let _: () = msg_send![&inspector, show];

        HOST.with(|slot| {
            *slot.borrow_mut() = Some(InspectorHost {
                inspector,
                _delegate: delegate,
            });
        });
    }

    Ok(())
}

#[cfg(target_os = "macos")]
pub fn teardown() -> Result<(), String> {
    MainThreadMarker::new()
        .ok_or_else(|| "WebKit Inspector teardown must run on the main thread".to_string())?;
    HOST.with(|slot| {
        if let Some(host) = slot.borrow_mut().take() {
            unsafe {
                let _: () = msg_send![&host.inspector, setDelegate: Option::<&AnyObject>::None];
            }
        }
    });
    Ok(())
}
