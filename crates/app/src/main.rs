#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod assistant;
mod blackjack;
mod burraco_ui;
mod cards_ui;
mod poker;
mod poker_it;
mod psyfly;
mod record;
mod scala40_ui;
mod scopa_ui;
mod settemezzo_ui;
mod update;
mod vision;
mod visual_reader;

fn main() -> eframe::Result {
    // Le partite salvate in automatico dalle versioni precedenti si cancellano una volta sola:
    // da ora si salvano solo quelle per cui si accende la registrazione.
    record::remove_automatic_once(&record::dir());
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title(format!("BrisMaster {}", update::CURRENT))
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([1080.0, 700.0])
            .with_icon(icona())
            // Con la trasparenza la finestra della Psy Fly può essere trasparente (la finestra
            // principale resta piena: il suo sfondo è opaco, vedi `clear_color`).
            .with_transparent(true),
        ..Default::default()
    };
    eframe::run_native("BrisMaster", options, Box::new(|cc| Ok(Box::new(app::BrisApp::new(cc)))))
}

/// L'icona della finestra e della barra delle applicazioni (la stessa dell'exe).
fn icona() -> eframe::egui::IconData {
    eframe::icon_data::from_png_bytes(include_bytes!("../assets/brismaster.png")).unwrap_or_default()
}
