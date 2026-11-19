//! Small platform-specific hooks so the UI code stays the same everywhere.

/// True on Android and in the browser, where there's no file system to pick files from
/// in the usual way and windows can't be flashed.
pub const IS_MOBILE_OR_WEB: bool = cfg!(any(target_os = "android", target_arch = "wasm32"));

#[cfg(target_os = "android")]
mod android {
    use std::path::PathBuf;
    use std::sync::OnceLock;
    use winit::platform::android::activity::AndroidApp;

    static APP: OnceLock<AndroidApp> = OnceLock::new();

    pub fn init(app: AndroidApp) {
        let _ = APP.set(app);
    }

    pub fn data_dir() -> Option<PathBuf> {
        APP.get().and_then(|a| a.internal_data_path())
    }

    pub fn show_keyboard(show: bool) {
        if let Some(app) = APP.get() {
            if show {
                app.show_soft_input(true);
            } else {
                app.hide_soft_input(true);
            }
        }
    }
}

#[cfg(target_os = "android")]
pub fn android_init(app: winit::platform::android::activity::AndroidApp) {
    android::init(app);
}

/// Folder for the app's data on Android (the app's private internal storage).
#[cfg(target_os = "android")]
pub fn android_data_dir() -> Option<std::path::PathBuf> {
    android::data_dir()
}

/// Web: hands `text` to the browser as a file download.
#[cfg(target_arch = "wasm32")]
pub fn download_text(file_name: &str, text: &str) -> Result<(), String> {
    use wasm_bindgen::JsCast;
    let err = |e: wasm_bindgen::JsValue| format!("{e:?}");
    let parts = js_sys::Array::new();
    parts.push(&wasm_bindgen::JsValue::from_str(text));
    let options = web_sys::BlobPropertyBag::new();
    options.set_type("application/json");
    let blob = web_sys::Blob::new_with_str_sequence_and_options(&parts, &options).map_err(err)?;
    let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(err)?;
    let document = web_sys::window().and_then(|w| w.document()).ok_or("no document")?;
    let link = document
        .create_element("a")
        .map_err(err)?
        .dyn_into::<web_sys::HtmlAnchorElement>()
        .map_err(|_| "couldn't create a download link".to_string())?;
    link.set_href(&url);
    link.set_download(file_name);
    link.click();
    Ok(())
}

/// Web: reloads the page. The browser re-checks the service worker on navigation, so a
/// newly published version is picked up too.
#[cfg(target_arch = "wasm32")]
pub fn reload_page() {
    if let Some(window) = web_sys::window() {
        let _ = window.location().reload();
    }
}

/// Desktop: opens a folder in the system file manager.
#[cfg(not(any(target_os = "android", target_arch = "wasm32")))]
pub fn open_folder(path: &std::path::Path) {
    let program = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(program).arg(path).spawn();
}

/// Shows or hides the on-screen keyboard. Only needed on Android.
#[allow(unused_variables)]
pub fn show_keyboard(show: bool) {
    #[cfg(target_os = "android")]
    android::show_keyboard(show);
}
