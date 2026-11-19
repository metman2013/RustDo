// Hide the console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod import;
mod model;
mod storage;

use eframe::egui;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("RustDo")
            .with_inner_size([1120.0, 740.0])
            .with_min_inner_size([760.0, 480.0]),
        ..Default::default()
    };
    eframe::run_native(
        "RustDo",
        options,
        Box::new(|cc| Ok(Box::new(app::TodoApp::new(cc)))),
    )
}
