#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod update;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title(format!("BrisMaster {}", update::CURRENT))
            .with_inner_size([1180.0, 760.0])
            .with_min_inner_size([900.0, 620.0]),
        ..Default::default()
    };
    eframe::run_native("BrisMaster", options, Box::new(|cc| Ok(Box::new(app::BrisApp::new(cc)))))
}
