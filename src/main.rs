// Without this, a release build on Windows opens a stray console window.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod config;
mod net;
mod stats;
mod term;
mod theme;
mod ui;

use eframe::{NativeOptions, Renderer};

const APP_NAME: &str = "edex-rs";

fn options(renderer: Renderer) -> NativeOptions {
    NativeOptions {
        renderer,
        // Ctrl+C and friends must reach the shell; we pick our own shortcuts.
        clipboard_shortcuts: false,
        viewport: egui::ViewportBuilder::default()
            .with_title(APP_NAME)
            .with_inner_size([1600.0, 900.0])
            .with_min_inner_size([ui::layout::MIN_WINDOW.x, ui::layout::MIN_WINDOW.y]),
        ..Default::default()
    }
}

/// Try the GPU renderer first, then OpenGL, so a machine with a poor driver
/// still gets a window. If neither works, say so instead of exiting silently.
fn main() {
    let mut failure = String::new();
    for renderer in [Renderer::Wgpu, Renderer::Glow] {
        let result = eframe::run_native(
            APP_NAME,
            options(renderer),
            Box::new(|cc| Ok(Box::new(app::EdexApp::new(cc)))),
        );
        match result {
            Ok(()) => return,
            Err(e) => failure = e.to_string(),
        }
    }
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title(APP_NAME)
        .set_description(format!(
            "{APP_NAME} could not start because no graphics renderer was usable. \
             Updating the graphics driver may help.\n\n{failure}"
        ))
        .show();
}
