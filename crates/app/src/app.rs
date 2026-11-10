//! Interfaccia: l'assistente. Si sceglie il gioco, si inseriscono le carte della partita e il motore
//! consiglia la mossa.

use crate::assistant::Assistant;
use crate::blackjack::BlackjackUi;
use crate::burraco_ui::BurracoAssistant;
use crate::cards_ui;
use crate::poker::HoldemAssistant;
use crate::poker_it::PokerIt;
use crate::psyfly::Fly;
use crate::scala40_ui::Scala40Ui;
use crate::scopa_ui::ScopaUi;
use crate::settemezzo_ui::{self, SetteMezzo};
use crate::update::{self, Release};
use briscola_core::ai::EngineConfig;
use eframe::egui::{self, Color32, FontId, Rect, RichText, Stroke, TextStyle};
use serde::{Deserialize, Serialize};
use std::sync::mpsc::{channel, Receiver};

/// Da questa precisione in su il consiglio viene dall'ISMCTS potenziato con altrettante
/// iterazioni: a 20.000 batte il motore a 3000 mondi (53% su 8.000 partite, bench/RISULTATI.md).
pub(crate) const ISMCTS_FROM: u32 = 20000;

/// Configurazione del motore per l'app: i mondi si dividono su tutti i core del PC.
pub(crate) fn engine_cfg(samples: u32) -> EngineConfig {
    let mut cfg = EngineConfig::strength(samples.min(3000));
    cfg.threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    if samples >= ISMCTS_FROM {
        cfg.ismcts = Some(samples);
    }
    cfg
}

/// Gioco scelto nella barra in alto.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
enum Gioco {
    #[default]
    Briscola,
    Scopa,
    Scala40,
    Burraco,
    SetteMezzo,
    Blackjack,
    PokerIt,
    Holdem,
}

impl Gioco {
    const ALL: [Gioco; 8] = [
        Gioco::Briscola,
        Gioco::Scopa,
        Gioco::Scala40,
        Gioco::Burraco,
        Gioco::SetteMezzo,
        Gioco::Blackjack,
        Gioco::PokerIt,
        Gioco::Holdem,
    ];
    fn label(self) -> &'static str {
        match self {
            Gioco::Briscola => "Briscola",
            Gioco::Scopa => "Scopa",
            Gioco::Scala40 => "Scala 40",
            Gioco::Burraco => "Burraco",
            Gioco::SetteMezzo => "Sette e mezzo",
            Gioco::Blackjack => "Blackjack",
            Gioco::PokerIt => "Poker all'italiana",
            Gioco::Holdem => "Texas Hold'em",
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
struct Settings {
    auto_update: bool,
    game: Gioco,
    /// Precisione dei consigli: mondi simulati, o iterazioni dell'ISMCTS da `ISMCTS_FROM` in su.
    /// Nome nuovo: chi aveva il vecchio valore (2000) passa al nuovo predefinito.
    assist_worlds: u32,
    burraco_samples: u32,
    settemezzo: settemezzo_ui::Prefs,
    /// Finestra compatta (solo il pannello dell'assistente), da affiancare al browser.
    compact: bool,
    /// Finestra compatta sempre in primo piano.
    on_top: bool,
    /// La Psy Fly accesa.
    psy_fly: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            auto_update: true,
            game: Gioco::Briscola,
            assist_worlds: ISMCTS_FROM,
            burraco_samples: 96,
            settemezzo: settemezzo_ui::Prefs::default(),
            compact: false,
            on_top: false,
            psy_fly: false,
        }
    }
}

enum UpdateState {
    Idle,
    Checking(Receiver<Result<Option<Release>, String>>),
    UpToDate,
    Available(Release),
    Installing(Receiver<String>),
    Error(String),
}

pub struct BrisApp {
    settings: Settings,
    update: UpdateState,
    assistant: Assistant,
    scopa: ScopaUi,
    scala40: Scala40Ui,
    burraco: BurracoAssistant,
    settemezzo: SetteMezzo,
    blackjack: BlackjackUi,
    poker_it: PokerIt,
    holdem: HoldemAssistant,
    /// Dimensioni e livello della finestra applicati: (compatta, in primo piano).
    window: Option<(bool, bool)>,
    /// La mosca psichica che indica sul tavolo del sito la carta da giocare.
    fly: Fly,
    /// Il gioco per cui la mosca ha già acceso la lettura dello schermo.
    fly_reading: Option<Gioco>,
    active_game: Option<Gioco>,
    visual: crate::visual_reader::VisualReader,
    visual_error: Option<String>,
}

impl BrisApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let settings: Settings = cc.storage.and_then(|s| eframe::get_value(s, "settings")).unwrap_or_default();
        install_theme(&cc.egui_ctx);
        let mut app = BrisApp {
            update: UpdateState::Idle,
            assistant: Assistant::default(),
            scopa: ScopaUi::new(cc.storage.and_then(|s| eframe::get_value(s, "scopa")).unwrap_or_default()),
            scala40: Scala40Ui::default(),
            burraco: BurracoAssistant::default(),
            settemezzo: SetteMezzo::new(&settings.settemezzo),
            blackjack: BlackjackUi::new(cc.storage.and_then(|s| eframe::get_value(s, "blackjack")).unwrap_or_default()),
            poker_it: PokerIt::load(cc.storage),
            holdem: HoldemAssistant::default(),
            window: None,
            fly: Fly::default(),
            fly_reading: None,
            active_game: None,
            visual: Default::default(),
            visual_error: None,
            settings,
        };
        if app.settings.auto_update {
            app.check_updates();
        }
        app
    }

    fn check_updates(&mut self) {
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(update::check());
        });
        self.update = UpdateState::Checking(rx);
    }

    fn poll_background(&mut self) {
        let next = match &self.update {
            UpdateState::Checking(rx) => rx.try_recv().ok().map(|r| match r {
                Ok(Some(rel)) => UpdateState::Available(rel),
                Ok(None) => UpdateState::UpToDate,
                Err(e) => UpdateState::Error(e),
            }),
            UpdateState::Installing(rx) => rx.try_recv().ok().map(UpdateState::Error),
            _ => None,
        };
        if let Some(n) = next {
            self.update = n;
        }
    }

    /// La finestra compatta c'è per la briscola e la scopa.
    fn compact(&self) -> bool {
        self.settings.compact && matches!(self.settings.game, Gioco::Briscola | Gioco::Scopa)
    }

    /// Ridimensiona la finestra quando si entra o si esce dalla modalità compatta.
    fn apply_window(&mut self, ctx: &egui::Context) {
        let want = (self.compact(), self.compact() && self.settings.on_top);
        if self.window == Some(want) {
            return;
        }
        let (min, size) = if want.0 { ([470.0, 560.0], [520.0, 760.0]) } else { ([1080.0, 700.0], [1280.0, 800.0]) };
        ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(min.into()));
        // Al primo avvio la finestra grande ha già le sue dimensioni: si cambiano solo se serve.
        if self.window.is_some() || want.0 {
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size.into()));
        }
        let level = if want.1 { egui::WindowLevel::AlwaysOnTop } else { egui::WindowLevel::Normal };
        ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(level));
        self.window = Some(want);
    }

    /// L'interruttore della Psy Fly: accendendola si accende anche la lettura dello schermo.
    fn fly_switch(&mut self, ui: &mut egui::Ui) {
        self.fly.on = self.settings.psy_fly;
        if self.fly.switch(ui) {
            self.settings.psy_fly = self.fly.on;
            if !self.fly.on {
                self.assistant.vision_off(ui.ctx());
                self.scopa.vision_off(ui.ctx());
                self.visual.stop();
            }
        }
    }

    fn vision_on(&mut self, ctx: &egui::Context) {
        match self.settings.game {
            Gioco::Briscola => self.assistant.vision_on(ctx),
            Gioco::Scopa => self.scopa.vision_on(ctx),
            _ => self.visual.set_enabled(true, self.settings.game == Gioco::SetteMezzo, ctx),
        }
    }

    /// Le coordinate e il consiglio provengono dal lettore/assistente del gioco attivo.
    fn show_fly(&mut self, ctx: &egui::Context) {
        if !self.settings.psy_fly {
            self.fly.on = false;
            self.fly.show(ctx, crate::psyfly::Sight::off());
            return;
        }
        let sight = match self.settings.game {
            Gioco::Briscola => self.assistant.fly_sight(),
            Gioco::Scopa => self.scopa.fly_sight(),
            game => {
                let hint = match game {
                    Gioco::Scala40 => self.scala40.visual_hint(),
                    Gioco::Burraco => self.burraco.visual_hint(self.settings.burraco_samples),
                    Gioco::SetteMezzo => self.settemezzo.visual_hint(&self.settings.settemezzo),
                    Gioco::Blackjack => self.blackjack.visual_hint(),
                    Gioco::PokerIt => self.poker_it.visual_hint(),
                    Gioco::Holdem => self.holdem.visual_hint(),
                    _ => unreachable!(),
                };
                self.visual.sight(game as u8, hint)
            }
        };
        self.fly.on = self.settings.psy_fly;
        self.fly.show(ctx, sight);
    }

    fn compact_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let title = format!("BrisMaster · {}", self.settings.game.label());
            ui.label(RichText::new(title).strong().color(cards_ui::GOLD));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Finestra grande").clicked() {
                    self.settings.compact = false;
                }
                ui.checkbox(&mut self.settings.on_top, "In primo piano");
                ui.separator();
                self.fly_switch(ui);
            });
        });
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.heading(RichText::new("BrisMaster").strong().color(cards_ui::GOLD));
            ui.label(RichText::new("IL TUO TAVOLO, LA TUA STRATEGIA").size(10.0).weak());
            ui.separator();
            egui::ComboBox::from_id_salt("game")
                .width(180.0)
                .selected_text(RichText::new(self.settings.game.label()).size(16.0).strong())
                .show_ui(ui, |ui| {
                    for g in Gioco::ALL {
                        ui.selectable_value(&mut self.settings.game, g, g.label());
                    }
                });
            if matches!(self.settings.game, Gioco::Briscola | Gioco::Scopa) {
                ui.separator();
                if ui.button("Finestra compatta")
                    .on_hover_text("Consiglio e carte accanto al browser").clicked() {
                    self.settings.compact = true;
                }
                self.fly_switch(ui);
            }
        });
        if !matches!(self.settings.game, Gioco::Briscola | Gioco::Scopa) {
            ui.horizontal_wrapped(|ui| {
                self.visual.controls(ui, self.settings.game == Gioco::SetteMezzo);
                self.fly_switch(ui);
            });
        }
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            match self.settings.game {
                Gioco::Briscola | Gioco::Scopa => {
                    ui.label(RichText::new("Inserisci le carte o attiva la lettura dello schermo. Il consiglio segue la partita.").weak());
                }
                Gioco::Scala40 => self.scala40.top_controls(ui),
                Gioco::SetteMezzo => self.settemezzo.top_controls(ui, &mut self.settings.settemezzo),
                Gioco::Blackjack => self.blackjack.top_controls(ui),
                Gioco::PokerIt => self.poker_it.top_controls(ui),
                Gioco::Burraco | Gioco::Holdem => {
                    ui.label(RichText::new("Descrivi la partita per confrontare le mosse disponibili.").weak());
                }
            }
        });
    }

    fn update_widget(&mut self, ui: &mut egui::Ui) {
        let mut action = None;
        match &self.update {
            UpdateState::Idle => {
                if ui.small_button("Cerca aggiornamenti").clicked() {
                    action = Some(0);
                }
            }
            UpdateState::Checking(_) => {
                ui.spinner();
                ui.label("Controllo aggiornamenti...");
            }
            UpdateState::UpToDate => {
                ui.label(RichText::new(format!("v{} aggiornata", update::CURRENT)).weak());
            }
            UpdateState::Available(rel) => {
                let btn = ui.button(RichText::new(format!("Installa v{}", rel.version)).strong());
                let btn = if rel.notes.is_empty() { btn } else { btn.on_hover_text(&rel.notes) };
                if btn.clicked() {
                    action = Some(1);
                }
                ui.hyperlink_to("Novità", &rel.page_url);
                ui.label(RichText::new("Aggiornamento disponibile").color(Color32::LIGHT_GREEN));
            }
            UpdateState::Installing(_) => {
                ui.spinner();
                ui.label("Scarico l'aggiornamento: poi si apre l'installazione e l'app si riavvia...");
            }
            UpdateState::Error(e) => {
                if ui.small_button("Riprova").clicked() {
                    action = Some(0);
                }
                // Il motivo si vede subito (non solo passando col mouse), con la pagina per scaricare a mano.
                let short = "Controllo aggiornamenti non disponibile";
                ui.hyperlink_to("Scarica a mano", format!("https://github.com/{}/releases/latest", update::REPO));
                ui.label(
                    RichText::new(short).color(Color32::LIGHT_RED).small(),
                )
                .on_hover_text(e);
            }
        }
        match action {
            Some(0) => self.check_updates(),
            Some(1) => {
                if let UpdateState::Available(rel) = &self.update {
                    let rel = rel.clone();
                    let (tx, rx) = channel();
                    std::thread::spawn(move || {
                        if let Err(e) = update::install(&rel) {
                            let _ = tx.send(e);
                        }
                    });
                    self.update = UpdateState::Installing(rx);
                }
            }
            _ => {}
        }
    }
}

impl eframe::App for BrisApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let now = ui.input(|i| i.time);
        self.poll_background();

        self.apply_window(&ui.ctx().clone());
        if self.active_game != Some(self.settings.game) {
            self.assistant.vision_active(self.settings.game == Gioco::Briscola, ui.ctx());
            self.scopa.vision_active(self.settings.game == Gioco::Scopa, ui.ctx());
            self.visual.stop();
            self.visual_error = None;
            self.active_game = Some(self.settings.game);
        }
        // La mosca accesa (anche dall'ultima volta) vede le carte con la lettura dello schermo:
        // si accende una volta per gioco, poi l'utente può spegnerla.
        if !self.settings.psy_fly {
            self.fly_reading = None;
        } else if self.fly_reading != Some(self.settings.game) {
            self.vision_on(&ui.ctx().clone());
            self.fly_reading = Some(self.settings.game);
        }
        let ui_ctx = ui.ctx().clone();
        self.ui_main(ui, now);
        if self.active_game != Some(self.settings.game) {
            ui_ctx.request_repaint();
        }
        if let Some(card) = self.visual.ui(&ui_ctx) {
            let result = match self.settings.game {
                Gioco::Scala40 => self.scala40.visual_insert(card),
                Gioco::Burraco => self.burraco.visual_insert(card),
                Gioco::SetteMezzo => self.settemezzo.visual_insert(card),
                Gioco::Blackjack => self.blackjack.visual_insert(card),
                Gioco::PokerIt => self.poker_it.visual_insert(card),
                Gioco::Holdem => self.holdem.visual_insert(card),
                _ => Err("Usa il lettore automatico del gioco selezionato".into()),
            };
            self.visual_error = result.err();
            ui_ctx.request_repaint();
        }
        self.show_fly(&ui_ctx);
    }

    /// Lo sfondo della finestra principale è opaco: la trasparenza serve solo alla Psy Fly.
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        visuals.panel_fill.to_normalized_gamma_f32()
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, "settings", &self.settings);
        eframe::set_value(storage, "scopa", &self.scopa.settings);
        eframe::set_value(storage, "blackjack", &self.blackjack.settings);
        self.poker_it.save(storage);
    }
}

impl BrisApp {
    fn ui_main(&mut self, ui: &mut egui::Ui, now: f64) {
        if self.compact() {
            egui::Panel::top("top").show(ui, |ui| {
                ui.add_space(4.0);
                self.compact_bar(ui);
                ui.add_space(4.0);
            });
            egui::CentralPanel::default().show(ui, |ui| match self.settings.game {
                Gioco::Scopa => self.scopa.compact(ui),
                _ => self.assistant.side_ui(ui, &mut self.settings.assist_worlds, true),
            });
            return;
        }
        egui::Panel::top("top").show(ui, |ui| {
            ui.add_space(6.0);
            self.top_bar(ui);
            ui.add_space(6.0);
        });
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(format!("{} · v{}", self.settings.game.label(), update::CURRENT)).small().color(cards_ui::GOLD));
                ui.separator();
                self.update_widget(ui);
                if let Some(error) = &self.visual_error {
                    ui.colored_label(Color32::LIGHT_RED, error);
                }
            });
        });
        egui::CentralPanel::default().frame(egui::Frame::new().fill(ui.visuals().panel_fill)).show(ui, |ui| {
            // Questi giochi disegnano da soli tutta l'area centrale.
            match self.settings.game {
                Gioco::Scopa => return self.scopa.central(ui),
                Gioco::Burraco => return self.burraco.ui(ui, &mut self.settings.burraco_samples),
                Gioco::Blackjack => return self.blackjack.ui(ui, now),
                Gioco::PokerIt => return self.poker_it.ui(ui),
                _ => {}
            }
            // Tavolo verde a sinistra, pannello informativo a destra.
            let full = ui.available_rect_before_wrap();
            let side_w = match self.settings.game {
                Gioco::SetteMezzo => (full.width() * 0.4).clamp(510.0, 580.0),
                Gioco::Holdem => 400.0,
                _ => (full.width() * 0.4).clamp(440.0, 560.0),
            };
            let split = full.max.x - side_w;
            let left = Rect::from_min_max(full.min, egui::pos2(split, full.max.y));
            let right =
                Rect::from_min_max(egui::pos2(split + 14.0, full.min.y + 10.0), full.max - egui::vec2(10.0, 0.0));
            cards_ui::paint_felt(ui.painter(), left);
            ui.painter().vline(split, full.y_range(), Stroke::new(1.0, Color32::from_gray(60)));
            match self.settings.game {
                Gioco::Scala40 => self.scala40.show(ui, left, right),
                Gioco::SetteMezzo => {
                    let (sm, prefs) = (&mut self.settemezzo, &mut self.settings.settemezzo);
                    ui.scope_builder(egui::UiBuilder::new().max_rect(right), |ui| sm.side_ui(ui, prefs));
                    ui.scope_builder(egui::UiBuilder::new().max_rect(left), |ui| sm.table_ui(ui, prefs));
                }
                Gioco::Holdem => {
                    ui.scope_builder(egui::UiBuilder::new().max_rect(right), |ui| self.holdem.side_ui(ui));
                    ui.scope_builder(egui::UiBuilder::new().max_rect(left), |ui| self.holdem.table_ui(ui));
                }
                _ => {
                    let samples = &mut self.settings.assist_worlds;
                    ui.scope_builder(egui::UiBuilder::new().max_rect(right), |ui| {
                        self.assistant.side_ui(ui, samples, false)
                    });
                    ui.scope_builder(egui::UiBuilder::new().max_rect(left), |ui| self.assistant.table_ui(ui));
                }
            }
        });
    }

}

/// One palette and control hierarchy shared by all eight game assistants.
fn install_theme(ctx: &egui::Context) {
    let mut style = (*ctx.global_style()).clone();
    style.visuals = egui::Visuals::dark();
    style.visuals.panel_fill = Color32::from_rgb(15, 24, 28);
    style.visuals.window_fill = Color32::from_rgb(22, 34, 39);
    style.visuals.extreme_bg_color = Color32::from_rgb(10, 18, 22);
    style.visuals.faint_bg_color = Color32::from_rgb(28, 42, 46);
    style.visuals.selection.bg_fill = Color32::from_rgb(45, 93, 84);
    style.visuals.selection.stroke = Stroke::new(1.0, Color32::from_rgb(238, 222, 170));
    style.visuals.hyperlink_color = Color32::from_rgb(129, 211, 203);
    style.visuals.window_corner_radius = egui::CornerRadius::same(12);
    style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(32, 47, 52);
    style.visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(27, 40, 45);
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(48, 71, 73);
    style.visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(40, 62, 65);
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, cards_ui::GOLD);
    style.visuals.widgets.active.bg_fill = Color32::from_rgb(51, 90, 80);
    for (ts, size) in [(TextStyle::Body, 15.0), (TextStyle::Button, 15.0),
        (TextStyle::Heading, 21.0), (TextStyle::Small, 12.0)] {
        style.text_styles.insert(ts, FontId::proportional(size));
    }
    style.spacing.button_padding = egui::vec2(12.0, 7.0);
    style.spacing.item_spacing = egui::vec2(9.0, 8.0);
    style.spacing.combo_width = 160.0;
    ctx.set_global_style(style);
}
