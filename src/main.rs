// Hide the console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result<()> {
    use eframe::egui;
    // The background companion: tray icon, sticky notes, reminders (see companion.rs).
    // `--reminders` is how versions before 0.2.7 started it at sign-in.
    let args: Vec<String> = std::env::args().collect();
    let companion = rustdo_app::companion::is_companion_launch(&args);
    // A log file and a crash handler: a desktop RustDo has no console to show errors in.
    rustdo_app::diagnostics::init(if companion { "companion" } else { "rustdo" });
    if companion {
        rustdo_app::companion::run(&args);
        return Ok(());
    }
    let options = || eframe::NativeOptions {
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
    // OpenGL, falling back to Direct3D on Windows (see diagnostics.rs).
    let result = rustdo_app::diagnostics::run_native("RustDo", options, |cc| Box::new(rustdo_app::TodoApp::new(cc)));
    if let Err(e) = &result {
        let log = rustdo_app::diagnostics::log_path("rustdo").map_or_else(String::new, |p| p.display().to_string());
        rustdo_app::diagnostics::show_error(
            "RustDo couldn't start",
            &format!(
                "RustDo couldn't open its window:\n\n{e}\n\nThis usually means the graphics driver is missing or \
                 out of date. Updating it from your PC maker or graphics card maker usually helps.\n\n\
                 Details are in:\n{log}"
            ),
        );
    }
    result
}

// The web build uses the library's `web_start` entry point instead.
#[cfg(target_arch = "wasm32")]
fn main() {}
