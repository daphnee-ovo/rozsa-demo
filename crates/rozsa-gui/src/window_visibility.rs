use tauri::{AppHandle, Manager, Runtime, Window};

pub(crate) fn main_window<R: Runtime>(app: &AppHandle<R>) -> Option<Window<R>> {
    app.get_window("main")
}
