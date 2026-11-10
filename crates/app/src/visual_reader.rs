//! Confirmed screen input for the games without a complete automatic state reconstructor.
use crate::{psyfly::{AdviceId, Guide, Sight}, vision::{self, memory::Memory, universal::{self, Cards, DeckCard}, Reader, Reading}};
use eframe::egui::{self, RichText};
use std::{sync::{Arc, Mutex}, time::Duration};

#[derive(Default)]
pub struct Hint { pub text: String, pub cards: Vec<DeckCard> }
impl Hint {
    pub fn text(text: impl Into<String>) -> Self { Self { text: text.into(), cards: Vec::new() } }
    pub fn card(text: impl Into<String>, card: DeckCard) -> Self { Self { text: text.into(), cards: vec![card] } }
}

#[derive(Default)]
pub struct VisualReader {
    enabled: bool,
    review: bool,
    naples: bool,
    reader: Option<Reader<Cards>>,
    scene: Option<(Cards, u64)>,
    memory: Option<Arc<Mutex<Memory>>>,
    textures: Vec<egui::TextureHandle>,
    labels: Vec<u8>,
    error: Option<String>,
    area_picker: vision::screen::AreaPicker,
}
impl VisualReader {
    pub fn stop(&mut self) { *self = Self::default(); }
    pub fn set_enabled(&mut self, enabled: bool, naples: bool, ctx: &egui::Context) {
        if enabled && self.reader.is_none() {
            self.naples = naples;
            let memory = if naples { Memory::shared() } else { Memory::french_shared() };
            self.reader = Some(Reader::start(memory.clone(), ctx.clone(), Duration::from_millis(350),
                if naples { universal::read_naples } else { universal::read_french }));
            self.memory = Some(memory);
            self.review = true;
        } else if !enabled { self.stop(); }
        self.enabled = enabled;
    }
    pub fn controls(&mut self, ui: &mut egui::Ui, naples: bool) {
        let mut enabled = self.enabled;
        if ui.checkbox(&mut enabled, "Leggi carte dallo schermo").on_hover_text("Rileva le carte; conferma il ruolo nell'assistente prima di inserirle").changed() {
            self.set_enabled(enabled, naples, ui.ctx());
        }
        if self.enabled && ui.small_button("Carte rilevate").clicked() { self.review = true; }
    }
    fn poll(&mut self, ctx: &egui::Context) {
        let Some(reader) = &self.reader else { return };
        while let Ok(reading) = reader.rx.try_recv() {
            match reading {
                Reading::Scene(scene, id) => {
                    self.textures = scene.cards.iter().enumerate().map(|(i, card)| {
                        let raw: Vec<u8> = card.image.px.iter().flatten().copied().collect();
                        ctx.load_texture(format!("visual-{id}-{i}"), egui::ColorImage::from_rgb([card.image.w, card.image.h], &raw), egui::TextureOptions::LINEAR)
                    }).collect();
                    self.labels = scene.cards.iter().map(|v| match v.card { DeckCard::Naples(c) | DeckCard::French(c) => c, DeckCard::Unknown => 255 }).collect();
                    self.scene = Some((scene, id)); self.error = None;
                }
                Reading::NoTable => { self.scene = None; self.textures.clear(); self.labels.clear(); }
                Reading::Error(error) => { self.scene = None; self.textures.clear(); self.labels.clear(); self.error = Some(error); }
            }
        }
    }
    pub fn ui(&mut self, ctx: &egui::Context) -> Option<DeckCard> {
        self.poll(ctx);
        if !self.enabled { return None; }
        let mut chosen = None;
        let mut open = self.review;
        egui::Window::new("Carte dallo schermo").id(egui::Id::unique("visual-review")).open(&mut open)
            .default_width(510.0).show(ctx, |ui| {
                ui.label("Scegli nell'assistente dove inserire la carta, poi conferma qui. Le azioni e i turni restano sotto il tuo controllo.");
                let summary = self.scene.as_ref().map(|(s,_)| s.cards.iter().map(|c| c.card.label()).collect::<Vec<_>>().join(", ")).unwrap_or_default();
                self.area_picker.controls(ui, self.reader.as_ref().and_then(|r| r.last_frame()), &summary);
                if let Some(e) = &self.error { ui.colored_label(egui::Color32::LIGHT_RED, e); }
                if let Some(memory) = &self.memory {
                    if let Some(e) = memory.lock().unwrap_or_else(|e| e.into_inner()).last_error() { ui.colored_label(egui::Color32::LIGHT_RED, e); }
                }
                let Some((scene, id)) = &self.scene else { ui.label("Cerco carte visibili sul tavolo…"); return; };
                let fresh = self.reader.as_ref().is_some_and(|r| !r.placement(*id).hidden);
                if !fresh { ui.label(RichText::new("Lettura scaduta: attendo una nuova immagine.").color(egui::Color32::YELLOW)); }
                egui::ScrollArea::vertical().max_height(380.0).show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for (i, card) in scene.cards.iter().enumerate() {
                            ui.vertical(|ui| {
                                ui.image((self.textures[i].id(), egui::vec2(60.0, 96.0)));
                                let mk = |c| if c == 255 { DeckCard::Unknown } else if self.naples { DeckCard::Naples(c) } else { DeckCard::French(c) };
                                egui::ComboBox::from_id_salt(("visual-label", i)).width(110.0)
                                    .selected_text(mk(self.labels[i]).label()).show_ui(ui, |ui| {
                                        for c in 0..if self.naples { 40 } else { 54 } {
                                            ui.selectable_value(&mut self.labels[i], c, mk(c).label());
                                        }
                                    });
                                ui.label(RichText::new(format!("Distacco {:.2}", card.margin)).small())
                                    .on_hover_text("Differenza di somiglianza fra le prime due carte; non è una probabilità di correttezza.");
                                if ui.add_enabled(fresh && self.labels[i] != 255, egui::Button::new("Inserisci")).clicked() {
                                    chosen = Some(mk(self.labels[i]));
                                    if let Some(mem) = &self.memory {
                                        // Remove a conflicting learned label before learning the user's correction.
                                        let mut mem = mem.lock().unwrap_or_else(|e| e.into_inner());
                                        if mk(self.labels[i]) != card.card { mem.forget(&card.image); }
                                        mem.remember(&card.image, vision::WHOLE, self.labels[i]);
                                    }
                                }
                            });
                        }
                    });
                });
            });
        self.review = open;
        chosen
    }
    pub fn sight(&self, game: u8, hint: Hint) -> Sight {
        let (Some(reader), Some((scene, id))) = (&self.reader, &self.scene) else { return Sight::off() };
        let placement = reader.placement(*id);
        if placement.hidden { return Sight::off(); }
        let mut targets = Vec::new();
        let complete = !hint.cards.is_empty() && hint.cards.iter().all(|&wanted| {
            let found: Vec<_> = scene.cards.iter().filter(|c| c.card == wanted).collect();
            if found.len() != 1 { return false; }
            let Some(at) = placement.on_desktop(found[0].at) else { return false; };
            targets.push(at); true
        });
        let guide = if complete {
            let card = targets.remove(0);
            let identity = |c| match c { DeckCard::Naples(c) | DeckCard::French(c) => c, DeckCard::Unknown => 0 };
            Guide::Select { id: AdviceId { card: identity(hint.cards[0]),
                take: hint.cards.iter().fold(0, |mask, &c| mask | (1u64 << identity(c))) },
                card, take: targets, text: hint.text.clone() }
        } else if hint.text.is_empty() { Guide::Waiting } else { Guide::Decision(hint.text.clone()) };
        use std::hash::{Hash, Hasher};
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        hint.text.hash(&mut hash);
        Sight { monitor: placement.monitor, hand: Vec::new(), guide, context: (game, hash.finish()), played_card: None }
    }
}
