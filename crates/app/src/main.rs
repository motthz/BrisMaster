#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod assistant;
mod blackjack;
mod burraco_ui;
mod cards_ui;
mod poker;
mod poker_it;
mod record;
mod scala40_ui;
mod scopa_ui;
mod settemezzo_ui;
mod update;
mod vision;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title(format!("BrisMaster {}", update::CURRENT))
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([1080.0, 700.0])
            .with_icon(icona()),
        ..Default::default()
    };
    eframe::run_native("BrisMaster", options, Box::new(|cc| Ok(Box::new(app::BrisApp::new(cc)))))
}

/// L'icona della finestra e della barra delle applicazioni (la stessa dell'exe).
fn icona() -> eframe::egui::IconData {
    eframe::icon_data::from_png_bytes(include_bytes!("../assets/brismaster.png")).unwrap_or_default()
}
