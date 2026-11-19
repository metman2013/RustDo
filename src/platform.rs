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

    /// Status bar, navigation bar, display cutout and on-screen keyboard sizes in physical
    /// pixels: (left, top, right, bottom). Uses the Java WindowInsets API through JNI.
    pub fn system_insets() -> Option<[i32; 4]> {
        use jni::objects::{JObject, JValueOwned};
        use jni::JNIEnv;

        // Any Java exception must be cleared before the next JNI call.
        fn call<'a>(env: &mut JNIEnv<'a>, obj: &JObject, name: &str, sig: &str) -> Option<JValueOwned<'a>> {
            match env.call_method(obj, name, sig, &[]) {
                Ok(v) => Some(v),
                Err(_) => {
                    let _ = env.exception_clear();
                    None
                }
            }
        }

        // Note: ndk_context's context is the *Application*, which has no window; the
        // Activity handle has to come from android-activity itself.
        let app = APP.get()?;
        let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) }.ok()?;
        let mut env = vm.attach_current_thread().ok()?;
        // A global reference owned by android-activity: wrap it, never delete it.
        let activity = unsafe { JObject::from_raw(app.activity_as_ptr().cast()) };

        let window = call(&mut env, &activity, "getWindow", "()Landroid/view/Window;")?.l().ok()?;
        let decor = call(&mut env, &window, "getDecorView", "()Landroid/view/View;")?.l().ok()?;
        let insets = call(&mut env, &decor, "getRootWindowInsets", "()Landroid/view/WindowInsets;")?.l().ok()?;
        if insets.is_null() {
            return None;
        }
        // getSystemWindowInset* covers the status and navigation bars, and on Android 11+
        // also the on-screen keyboard while it's open.
        let mut get = |name: &str| call(&mut env, &insets, name, "()I").and_then(|v| v.i().ok()).unwrap_or(0);
        let mut out = [
            get("getSystemWindowInsetLeft"),
            get("getSystemWindowInsetTop"),
            get("getSystemWindowInsetRight"),
            get("getSystemWindowInsetBottom"),
        ];
        // Notches and camera holes (Android 9+), in case they reach further than the status bar.
        if let Some(cutout) = call(&mut env, &insets, "getDisplayCutout", "()Landroid/view/DisplayCutout;")
            .and_then(|v| v.l().ok())
            .filter(|c| !c.is_null())
        {
            let mut safe = |name: &str| call(&mut env, &cutout, name, "()I").and_then(|v| v.i().ok()).unwrap_or(0);
            let cut = [
                safe("getSafeInsetLeft"),
                safe("getSafeInsetTop"),
                safe("getSafeInsetRight"),
                safe("getSafeInsetBottom"),
            ];
            for i in 0..4 {
                out[i] = out[i].max(cut[i]);
            }
        }
        Some(out)
    }

    /// Insets implied by the content rectangle Android reports for the window (no JNI).
    pub fn content_rect_insets() -> Option<[i32; 4]> {
        let app = APP.get()?;
        let window = app.native_window()?;
        let (w, h) = (window.width(), window.height());
        let r = app.content_rect();
        if r.right <= r.left || r.bottom <= r.top || w <= 0 || h <= 0 {
            return None;
        }
        Some([r.left, r.top, (w - r.right).max(0), (h - r.bottom).max(0)])
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

/// Space taken by the phone's own UI (status bar, navigation bar, notch, on-screen
/// keyboard), in egui points: (left, top, right, bottom). Zero except on Android.
#[allow(unused_variables)]
pub fn safe_insets(pixels_per_point: f32) -> [f32; 4] {
    #[cfg(target_os = "android")]
    {
        let (jni, content) = android_inset_sources();
        // Take whichever source reports more space on each edge.
        let mut px = [0i32; 4];
        for source in [jni, content].into_iter().flatten() {
            for i in 0..4 {
                px[i] = px[i].max(source[i]);
            }
        }
        let ppp = pixels_per_point.max(0.1);
        return px.map(|p| p as f32 / ppp);
    }
    #[allow(unreachable_code)]
    [0.0; 4]
}

/// Android: the raw inset readings in pixels, (WindowInsets via JNI, content rectangle),
/// shown in the ⚙ menu to help diagnose layout problems on specific phones.
#[cfg(target_os = "android")]
pub fn android_inset_sources() -> (Option<[i32; 4]>, Option<[i32; 4]>) {
    // Never let a JNI surprise take the app down: a failed reading counts as none.
    let jni = std::panic::catch_unwind(android::system_insets).ok().flatten();
    let content = std::panic::catch_unwind(android::content_rect_insets).ok().flatten();
    (jni, content)
}

/// Shows or hides the on-screen keyboard. Only needed on Android.
#[allow(unused_variables)]
pub fn show_keyboard(show: bool) {
    #[cfg(target_os = "android")]
    android::show_keyboard(show);
}
