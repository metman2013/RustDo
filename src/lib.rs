//! RustDo: one codebase for the desktop app, the web app (PWA) and the Android app.

pub mod app;
pub mod import;
pub mod model;
pub mod notify;
pub mod platform;
pub mod storage;
pub mod update;

pub use app::TodoApp;

// ---------------------------------------------------------------- Web entry point

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn web_start() {
    use wasm_bindgen::JsCast;
    let document = web_sys::window().and_then(|w| w.document()).expect("no document");
    let canvas = document
        .get_element_by_id("rustdo_canvas")
        .expect("missing #rustdo_canvas")
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .expect("#rustdo_canvas is not a canvas");

    wasm_bindgen_futures::spawn_local(async move {
        let result = eframe::WebRunner::new()
            .start(
                canvas,
                eframe::WebOptions::default(),
                Box::new(|cc| Ok(Box::new(TodoApp::new(cc)))),
            )
            .await;
        if let Some(loading) = document.get_element_by_id("loading") {
            match result {
                Ok(()) => loading.remove(),
                Err(e) => loading.set_inner_html(&format!("RustDo couldn't start: {e:?}")),
            }
        }
    });
}

// ---------------------------------------------------------------- Android entry point

#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(app: winit::platform::android::activity::AndroidApp) {
    android_logger::init_once(android_logger::Config::default().with_max_level(log::LevelFilter::Info));
    platform::android_init(app.clone());

    let options = eframe::NativeOptions {
        android_app: Some(app),
        ..Default::default()
    };
    if let Err(e) = eframe::run_native("RustDo", options, Box::new(|cc| Ok(Box::new(TodoApp::new(cc))))) {
        log::error!("RustDo failed to start: {e}");
    }
}
