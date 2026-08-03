mod app;
mod codegen;
mod model;
mod storage;
mod ui;

fn main() -> eframe::Result<()> {
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
