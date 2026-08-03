pub mod app;
pub mod codegen;
pub mod model;
pub mod storage;
pub mod ui;

pub fn run() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 820.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Clap CLI Maker",
        options,
        Box::new(|_cc| Ok(Box::new(app::CliMakerApp::new()))),
    )
}
