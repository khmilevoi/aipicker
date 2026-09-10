#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod charts;
mod theme;
mod tray;

use aipicker::storage::Store;
use std::path::PathBuf;

fn main() -> eframe::Result {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let demo = args.iter().any(|a| a == "--demo");
    let hidden = args.iter().any(|a| a == "--start-hidden");
    let directory = args
        .iter()
        .position(|a| a == "--data-dir")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
        .unwrap_or_else(Store::default_directory);
    let store = Store::new(directory).map_err(|e| eframe::Error::AppCreation(e.into()))?;
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size(app::COMPACT)
            .with_min_inner_size(app::COMPACT)
            .with_decorations(false)
            .with_transparent(true)
            .with_resizable(false)
            .with_title("AI Picker")
            .with_taskbar(false)
            .with_icon(eframe::egui::IconData {
                rgba: tray::icon_rgba(),
                width: 32,
                height: 32,
            }),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "AI Picker",
        options,
        Box::new(move |cc| Ok(Box::new(app::PickerApp::new(cc, store, demo, hidden)))),
    )
}
