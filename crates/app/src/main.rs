//! Thread Vision: measure diameters and threads with the backlit telecentric camera.

mod app;
mod log;
mod settings;
mod worker;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1500.0, 900.0])
            .with_title("Thread Vision"),
        ..Default::default()
    };
    eframe::run_native(
        "Thread Vision",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}
