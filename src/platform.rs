//! Small platform-specific hooks so the UI code stays the same everywhere.

/// True on Android and in the browser, where there's no file system to pick files from
/// in the usual way and windows can't be flashed.
pub const IS_MOBILE_OR_WEB: bool = cfg!(any(target_os = "android", target_arch = "wasm32"));

#[cfg(target_os = "android")]
mod android {
    use jni::objects::JObject;
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
        use jni::objects::JValueOwned;

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

    /// Back at the top level: send RustDo to the background, like other Android apps
    /// (Activity.moveTaskToBack), instead of closing it.
    pub fn move_to_background() {
        let Some(app) = APP.get() else { return };
        let Ok(vm) = (unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) }) else { return };
        let Ok(mut env) = vm.attach_current_thread() else { return };
        let activity = unsafe { JObject::from_raw(app.activity_as_ptr().cast()) };
        if env.call_method(&activity, "moveTaskToBack", "(Z)Z", &[JValue::Bool(1)]).is_err() {
            let _ = env.exception_clear();
        }
    }

    // ---------------------------------------------------------------- photos
    //
    // NativeActivity can't receive the system photo picker's result, so RustDo lists the
    // phone's photos itself (MediaStore) and shows them in its own picker.

    use jni::objects::{JByteArray, JValue};
    use jni::JNIEnv;

    /// Runs `f` with a JNI environment and the activity; a Java exception gives None.
    fn with_env<T>(f: impl for<'a> FnOnce(&mut JNIEnv<'a>, &JObject<'a>) -> jni::errors::Result<T>) -> Option<T> {
        let app = APP.get()?;
        let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) }.ok()?;
        let mut env = vm.attach_current_thread().ok()?;
        // A global reference owned by android-activity: wrap it, never delete it.
        let activity = unsafe { JObject::from_raw(app.activity_as_ptr().cast()) };
        match f(&mut env, &activity) {
            Ok(v) => Some(v),
            Err(e) => {
                if env.exception_check().unwrap_or(false) {
                    let _ = env.exception_describe();
                    let _ = env.exception_clear();
                }
                log::warn!(target: "rustdo", "photos: {e}");
                None
            }
        }
    }

    fn sdk(env: &mut JNIEnv) -> jni::errors::Result<i32> {
        env.get_static_field("android/os/Build$VERSION", "SDK_INT", "I")?.i()
    }

    /// The permission that lets RustDo see your photos (Android 13 split it out).
    fn photo_permission(env: &mut JNIEnv) -> jni::errors::Result<&'static str> {
        Ok(if sdk(env)? >= 33 { "android.permission.READ_MEDIA_IMAGES" } else { "android.permission.READ_EXTERNAL_STORAGE" })
    }

    pub fn photo_access() -> bool {
        with_env(|env, activity| {
            let name = photo_permission(env)?;
            let name = env.new_string(name)?;
            Ok(env.call_method(activity, "checkSelfPermission", "(Ljava/lang/String;)I", &[JValue::Object(&name)])?.i()? == 0)
        })
        .unwrap_or(false)
    }

    /// Shows Android's "Allow RustDo to access photos?" (the answer is seen by checking
    /// `photo_access` again; NativeActivity doesn't get the callback).
    pub fn ask_photo_access() {
        with_env(|env, activity| {
            let name = photo_permission(env)?;
            let name = env.new_string(name)?;
            let names = env.new_object_array(1, "java/lang/String", &name)?;
            env.call_method(activity, "requestPermissions", "([Ljava/lang/String;I)V", &[JValue::Object(names.as_ref()), JValue::Int(7)])?;
            Ok(())
        });
    }

    fn images_uri<'a>(env: &mut JNIEnv<'a>) -> jni::errors::Result<JObject<'a>> {
        env.get_static_field("android/provider/MediaStore$Images$Media", "EXTERNAL_CONTENT_URI", "Landroid/net/Uri;")?.l()
    }

    /// The newest photos' MediaStore ids, newest first.
    pub fn recent_photos(max: usize) -> Vec<i64> {
        with_env(|env, activity| {
            let resolver = env.call_method(activity, "getContentResolver", "()Landroid/content/ContentResolver;", &[])?.l()?;
            let uri = images_uri(env)?;
            let column = env.new_string("_id")?;
            let projection = env.new_object_array(1, "java/lang/String", &column)?;
            let order = env.new_string("date_added DESC")?;
            let null = JObject::null();
            let cursor = env
                .call_method(
                    &resolver,
                    "query",
                    "(Landroid/net/Uri;[Ljava/lang/String;Ljava/lang/String;[Ljava/lang/String;Ljava/lang/String;)Landroid/database/Cursor;",
                    &[JValue::Object(&uri), JValue::Object(projection.as_ref()), JValue::Object(&null), JValue::Object(&null), JValue::Object(&order)],
                )?
                .l()?;
            let mut ids = Vec::new();
            if cursor.is_null() {
                return Ok(ids);
            }
            while ids.len() < max && env.call_method(&cursor, "moveToNext", "()Z", &[])?.z()? {
                ids.push(env.call_method(&cursor, "getLong", "(I)J", &[JValue::Int(0)])?.j()?);
            }
            env.call_method(&cursor, "close", "()V", &[])?;
            Ok(ids)
        })
        .unwrap_or_default()
    }

    /// One photo as JPEG or the file's own bytes: shrunk to fit `size` (a thumbnail for
    /// the picker, or the picture itself). Android 10+ shrinks it (and reads any format
    /// the phone can show, e.g. HEIC); older phones give a small thumbnail or the file.
    pub fn photo(id: i64, size: i32) -> Option<Vec<u8>> {
        with_env(|env, activity| {
            env.with_local_frame(32, |env| -> jni::errors::Result<Vec<u8>> {
                let resolver = env.call_method(activity, "getContentResolver", "()Landroid/content/ContentResolver;", &[])?.l()?;
                let base = images_uri(env)?;
                let uri = env
                    .call_static_method("android/content/ContentUris", "withAppendedId", "(Landroid/net/Uri;J)Landroid/net/Uri;", &[JValue::Object(&base), JValue::Long(id)])?
                    .l()?;
                let null = JObject::null();
                let modern = sdk(env)? >= 29;
                let bitmap = if modern {
                    let wanted = env.new_object("android/util/Size", "(II)V", &[JValue::Int(size), JValue::Int(size)])?;
                    env.call_method(
                        &resolver,
                        "loadThumbnail",
                        "(Landroid/net/Uri;Landroid/util/Size;Landroid/os/CancellationSignal;)Landroid/graphics/Bitmap;",
                        &[JValue::Object(&uri), JValue::Object(&wanted), JValue::Object(&null)],
                    )?
                    .l()?
                } else if size <= 512 {
                    // MINI_KIND = 1
                    env.call_static_method(
                        "android/provider/MediaStore$Images$Thumbnails",
                        "getThumbnail",
                        "(Landroid/content/ContentResolver;JILandroid/graphics/BitmapFactory$Options;)Landroid/graphics/Bitmap;",
                        &[JValue::Object(&resolver), JValue::Long(id), JValue::Int(1), JValue::Object(&null)],
                    )?
                    .l()?
                } else {
                    // The file itself (RustDo shrinks it).
                    let stream = env.call_method(&resolver, "openInputStream", "(Landroid/net/Uri;)Ljava/io/InputStream;", &[JValue::Object(&uri)])?.l()?;
                    let buf = env.new_byte_array(64 * 1024)?;
                    let mut data = Vec::new();
                    let mut chunk = vec![0i8; 64 * 1024];
                    loop {
                        let n = env.call_method(&stream, "read", "([B)I", &[JValue::Object(buf.as_ref())])?.i()?;
                        if n < 0 || data.len() > 60 * 1024 * 1024 {
                            break;
                        }
                        let n = n as usize;
                        env.get_byte_array_region(&buf, 0, &mut chunk[..n])?;
                        data.extend(chunk[..n].iter().map(|b| *b as u8));
                    }
                    env.call_method(&stream, "close", "()V", &[])?;
                    return Ok(data);
                };
                if bitmap.is_null() {
                    return Ok(Vec::new());
                }
                let format = env
                    .get_static_field("android/graphics/Bitmap$CompressFormat", "JPEG", "Landroid/graphics/Bitmap$CompressFormat;")?
                    .l()?;
                let out = env.new_object("java/io/ByteArrayOutputStream", "()V", &[])?;
                env.call_method(
                    &bitmap,
                    "compress",
                    "(Landroid/graphics/Bitmap$CompressFormat;ILjava/io/OutputStream;)Z",
                    &[JValue::Object(&format), JValue::Int(88), JValue::Object(&out)],
                )?;
                env.call_method(&bitmap, "recycle", "()V", &[])?;
                let bytes = JByteArray::from(env.call_method(&out, "toByteArray", "()[B", &[])?.l()?);
                env.convert_byte_array(&bytes)
            })
        })
        .filter(|b| !b.is_empty())
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

/// Android: whether RustDo may see your photos.
#[cfg(target_os = "android")]
pub fn android_photo_access() -> bool {
    android::photo_access()
}

/// Android: asks for permission to see your photos.
#[cfg(target_os = "android")]
pub fn android_ask_photo_access() {
    android::ask_photo_access()
}

/// Android: the newest photos (ids), newest first.
#[cfg(target_os = "android")]
pub fn android_recent_photos(max: usize) -> Vec<i64> {
    android::recent_photos(max)
}

/// Android: one photo, shrunk to fit `size` pixels where the phone can.
#[cfg(target_os = "android")]
pub fn android_photo(id: i64, size: i32) -> Option<Vec<u8>> {
    android::photo(id, size)
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

/// Opens a printable page in the browser (Show list › Print): a file on the desktop, a
/// new tab in the web app. The browser's print dialog can also save it as a PDF.
/// On the desktop the page is saved in `folder` (RustDo's own folder: the Store version's
/// temp folder is private to it, so a browser couldn't open a file there).
pub fn open_printable(folder: Option<std::path::PathBuf>, file_name: &str, html: &str) -> Result<(), String> {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (file_name, folder);
        let err = |e: wasm_bindgen::JsValue| format!("{e:?}");
        let parts = js_sys::Array::new();
        parts.push(&wasm_bindgen::JsValue::from_str(html));
        let options = web_sys::BlobPropertyBag::new();
        options.set_type("text/html");
        let blob = web_sys::Blob::new_with_str_sequence_and_options(&parts, &options).map_err(err)?;
        let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(err)?;
        let window = web_sys::window().ok_or("no window")?;
        match window.open_with_url_and_target(&url, "_blank").map_err(err)? {
            Some(_) => Ok(()),
            None => Err(crate::i18n::tr("The browser blocked the new tab. Allow pop-ups for rustdo.app and try again.").into()),
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let dir = folder.map(|f| f.join("Print")).unwrap_or_else(std::env::temp_dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = dir.join(file_name);
        std::fs::write(&path, html).map_err(|e| e.to_string())?;
        #[cfg(not(target_os = "android"))]
        open_folder(&path);
        Ok(())
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
        // WindowInsets measure exactly how far the system bars overlap *this window*, so
        // they're authoritative. The content rectangle can include offsets the window
        // already accounts for (v0.1.3 padded far too much by mixing the two), so it's
        // only a fallback for when the JNI reading isn't available.
        let px = jni.or(content).unwrap_or([0; 4]);
        let ppp = pixels_per_point.max(0.1);
        return px.map(|p| p.max(0) as f32 / ppp);
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

/// Whether to start in touch mode: always on Android; in the browser when the main
/// pointer is a finger (phones, tablets). Any touch later switches it on too.
pub fn prefers_touch() -> bool {
    #[cfg(target_os = "android")]
    {
        true
    }
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window()
            .and_then(|w| w.match_media("(pointer: coarse)").ok().flatten())
            .is_some_and(|m| m.matches())
    }
    // Desktop: off unless forced (RUSTDO_TOUCH=1, handy for testing); a touch screen
    // switches it on at the first touch anyway.
    #[cfg(not(any(target_os = "android", target_arch = "wasm32")))]
    {
        std::env::var_os("RUSTDO_TOUCH").is_some_and(|v| v == "1")
    }
}

/// The web app on a phone or tablet (a finger is the main pointer). Asked once.
pub fn web_touch_screen() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        thread_local! {
            static TOUCH: bool = prefers_touch();
        }
        TOUCH.with(|t| *t)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        false
    }
}

/// Android: back at RustDo's top level puts it in the background. Elsewhere: nothing.
pub fn leave_app() {
    #[cfg(target_os = "android")]
    let _ = std::panic::catch_unwind(android::move_to_background);
}

/// Shows or hides the on-screen keyboard. Only needed on Android.
#[allow(unused_variables)]
pub fn show_keyboard(show: bool) {
    #[cfg(target_os = "android")]
    android::show_keyboard(show);
}
