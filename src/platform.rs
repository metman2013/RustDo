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

/// Shows or hides the on-screen keyboard. Only needed on Android.
#[allow(unused_variables)]
pub fn show_keyboard(show: bool) {
    #[cfg(target_os = "android")]
    android::show_keyboard(show);
}
