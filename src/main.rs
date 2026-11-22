// Hide the console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result<()> {
    use eframe::egui;
    // The background companion: tray icon, sticky notes, reminders (see companion.rs).
    // `--reminders` is how versions before 0.2.7 started it at sign-in.
    let args: Vec<String> = std::env::args().collect();
    if rustdo_app::companion::is_companion_launch(&args) {
        rustdo_app::companion::run(&args);
        return Ok(());
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("RustDo")
            .with_inner_size([1120.0, 740.0])
            .with_min_inner_size([340.0, 480.0])
            .with_icon(
                eframe::icon_data::from_png_bytes(include_bytes!("../web/icons/icon-192.png"))
                    .unwrap_or_default(),
            ),
        ..Default::default()
    };
    eframe::run_native(
        "RustDo",
        options,
        Box::new(|cc| Ok(Box::new(rustdo_app::TodoApp::new(cc)))),
    )
}

// The web build uses the library's `web_start` entry point instead.
#[cfg(target_arch = "wasm32")]
fn main() {}
