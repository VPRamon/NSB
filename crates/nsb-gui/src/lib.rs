//! Native desktop interface for the NSB scientific library.

mod app;
mod compute;
mod input;

pub use app::NsbApp;

/// Launch the native NSB desktop application.
pub fn run() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1320.0, 860.0])
            .with_min_inner_size([980.0, 650.0]),
        ..Default::default()
    };
    eframe::run_native(
        "NSB — Night Sky Brightness",
        options,
        Box::new(|cc| Ok(Box::new(NsbApp::new(cc)))),
    )
}
