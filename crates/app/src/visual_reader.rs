//! Confirmed screen input for the games without a complete automatic state reconstructor.
use crate::{
    psyfly::{AdviceId, Guide, Sight},
    vision::{
        self,
        memory::Memory,
        universal::{self, Cards, DeckCard},
        Reader, Reading,
    },
};
use eframe::egui::{self, RichText};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

#[derive(Default)]
pub struct Hint {
    pub text: String,
    pub cards: Vec<DeckCard>,
}
impl Hint {
    pub fn text(text: impl Into<String>) -> Self {
        Self { text: text.into(), cards: Vec::new() }
    }
    pub fn card(text: impl Into<String>, card: DeckCard) -> Self {
        Self { text: text.into(), cards: vec![card] }
    }
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
    own_hand: Vec<bool>,
    last_image: Option<vision::Img>,
    scene_area: Option<vision::screen::Area>,
    error: Option<String>,
    area_picker: vision::screen::AreaPicker,
    layout: vision::autosync::Layout,
    required_roles: Vec<vision::autosync::Role>,
    automatic: bool,
    auto_monitor: Option<vision::capture::Geom>,
    synchronizer: vision::autosync::Reconciler,
    auto_status: String,
    edit_zones: bool,
    zone_role: vision::autosync::Role,
    zone_drag: Option<(egui::Pos2, egui::Pos2)>,
    table_texture: Option<egui::TextureHandle>,
}
impl VisualReader {
    pub fn stop(&mut self) {
        let layout = std::mem::take(&mut self.layout);
        let required = std::mem::take(&mut self.required_roles);
        *self = Self::default();
        self.layout = layout;
        self.required_roles = required;
    }
    pub fn require_zones(&mut self, roles: &[vision::autosync::Role]) {
        self.required_roles = roles.to_vec();
    }
    pub fn automatic(&self) -> bool {
        self.automatic
    }
    pub fn layout(&self) -> vision::autosync::Layout {
        self.layout.clone()
    }
    pub fn set_layout(&mut self, layout: vision::autosync::Layout) {
        self.layout = layout;
        self.automatic = false;
        self.synchronizer = Default::default();
    }
    pub fn set_enabled(&mut self, enabled: bool, naples: bool, ctx: &egui::Context) {
        if enabled && self.reader.is_none() {
            self.naples = naples;
            let memory = if naples { Memory::shared() } else { Memory::french_shared() };
            self.reader = Some(Reader::start(
                memory.clone(),
                ctx.clone(),
                Duration::from_millis(350),
                if naples { universal::read_naples } else { universal::read_french },
            ));
            self.memory = Some(memory);
            self.review = true;
        } else if !enabled {
            self.stop();
        }
        self.enabled = enabled;
    }
    pub fn controls(&mut self, ui: &mut egui::Ui, naples: bool) {
        let mut enabled = self.enabled;
        if ui
            .checkbox(&mut enabled, "Leggi carte dallo schermo")
            .on_hover_text("Rileva le carte; conferma il ruolo nell'assistente prima di inserirle")
            .changed()
        {
            self.set_enabled(enabled, naples, ui.ctx());
        }
        if self.enabled && ui.small_button("Carte rilevate").clicked() {
            self.review = true;
        }
    }
    fn poll(&mut self, ctx: &egui::Context) {
        let Some(reader) = &self.reader else { return };
        while let Ok(reading) = reader.rx.try_recv() {
            match reading {
                Reading::Scene(scene, id) => {
                    let area = *vision::screen::area().lock().unwrap_or_else(|e| e.into_inner());
                    self.last_image = reader.last_frame();
                    self.table_texture = self.last_image.as_ref().map(|im| table_texture(ctx, im));
                    let own_hand = scene
                        .cards
                        .iter()
                        .map(|new| {
                            self.scene
                                .as_ref()
                                .filter(|_| self.scene_area == Some(area))
                                .and_then(|(old, _)| {
                                    old.cards
                                        .iter()
                                        .enumerate()
                                        .find(|(_, old)| same_observation(old, new))
                                        .and_then(|(i, _)| self.own_hand.get(i).copied())
                                })
                                .unwrap_or(false)
                        })
                        .collect();
                    let labels = scene
                        .cards
                        .iter()
                        .map(|new| {
                            self.scene
                                .as_ref()
                                .filter(|_| self.scene_area == Some(area))
                                .and_then(|(old, _)| {
                                    old.cards
                                        .iter()
                                        .enumerate()
                                        .find(|(_, old)| same_observation(old, new))
                                        .and_then(|(i, _)| self.labels.get(i).copied())
                                })
                                .unwrap_or(match new.card {
                                    DeckCard::Naples(c) | DeckCard::French(c) => c,
                                    DeckCard::Unknown => 255,
                                })
                        })
                        .collect();
                    self.textures = scene
                        .cards
                        .iter()
                        .enumerate()
                        .map(|(i, card)| {
                            let raw: Vec<u8> = card.image.px.iter().flatten().copied().collect();
                            ctx.load_texture(
                                format!("visual-{id}-{i}"),
                                egui::ColorImage::from_rgb([card.image.w, card.image.h], &raw),
                                egui::TextureOptions::LINEAR,
                            )
                        })
                        .collect();
                    self.labels = labels;
                    self.own_hand = own_hand;
                    self.scene_area = Some(area);
                    match vision::autosync::Observation::from_scene(&scene, &self.layout, area) {
                        Ok(observation) if !reader.placement(id).hidden => {
                            let placement = reader.placement(id);
                            if let Some(captured) = placement.captured_at().filter(|_| !placement.hidden) {
                                self.synchronizer.observe(observation, id, captured);
                            } else {
                                self.synchronizer.interrupt();
                            }
                            if self.automatic {
                                self.auto_status = "Attendo letture coerenti o nuove carte".into();
                            }
                        }
                        Ok(_) => self.synchronizer.interrupt(),
                        Err(e) => {
                            self.synchronizer.interrupt();
                            if self.automatic {
                                self.auto_status = e;
                            }
                        }
                    }
                    self.scene = Some((scene, id));
                    self.error = None;
                    if self.automatic {
                        let source = reader.placement(id).monitor;
                        if self.auto_monitor.is_some() && source != self.auto_monitor {
                            self.automatic = false;
                            self.synchronizer.interrupt();
                            self.error = Some(
                                "Monitor o scala cambiati: verifica le zone e riattiva la sincronizzazione".into(),
                            );
                        } else {
                            self.auto_monitor = source;
                        }
                    }
                }
                Reading::NoTable => {
                    self.synchronizer.interrupt();
                    self.last_image = reader.last_frame();
                    self.table_texture = self.last_image.as_ref().map(|im| table_texture(ctx, im));
                    self.scene = None;
                    self.textures.clear();
                    self.labels.clear();
                    self.own_hand.clear();
                }
                Reading::Error(error) => {
                    self.synchronizer.interrupt();
                    self.scene = None;
                    self.textures.clear();
                    self.labels.clear();
                    self.own_hand.clear();
                    self.error = Some(error);
                }
            }
        }
    }
    pub fn ui(&mut self, ctx: &egui::Context) -> Option<DeckCard> {
        self.poll(ctx);
        if !self.enabled {
            return None;
        }
        let mut chosen = None;
        let mut open = self.review;
        egui::Window::new("Carte dallo schermo").id(egui::Id::unique("visual-review")).open(&mut open)
            .default_width(570.0)
            .default_height(640.0).max_height((ctx.viewport_rect().height()-40.0).max(240.0)).vscroll(true).show(ctx, |ui| {
                ui.label("Scegli nell'assistente dove inserire la carta, poi conferma qui. Le azioni e i turni restano sotto il tuo controllo.");
                ui.label("Per Psy Fly, segna «La mia mano» sulle tue carte: le carte del tavolo non saranno indicate come scarti.");
                let summary = self.scene.as_ref().map(|(s,_)| s.cards.iter().map(|c| c.card.label()).collect::<Vec<_>>().join(", ")).unwrap_or_default();
                self.area_picker.controls_ref(ui, self.last_image.as_ref(), &summary);
                if let Some(e) = &self.error { ui.colored_label(egui::Color32::LIGHT_RED, e); }
                if let Some(memory) = &self.memory {
                    if let Some(e) = memory.lock().unwrap_or_else(|e| e.into_inner()).last_error() { ui.colored_label(egui::Color32::LIGHT_RED, e); }
                }
                self.calibration_ui(ui);
                let auto_changed=ui.checkbox(&mut self.automatic,"Sincronizza automaticamente le carte nelle zone").changed();
                if auto_changed {
                    self.synchronizer.interrupt(); self.auto_status.clear(); self.error=None;
                    self.auto_monitor=if self.automatic {self.scene.as_ref().and_then(|(_,id)|self.reader.as_ref().and_then(|r|r.placement(*id).monitor))} else {None};
                }
                if self.automatic { ui.label(if self.auto_status.is_empty() { "Attendo due letture coerenti…" } else { &self.auto_status }); }
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
                                ui.checkbox(&mut self.own_hand[i], "La mia mano");
                                ui.label(RichText::new(format!("Distacco {:.2}", card.margin)).small())
                                    .on_hover_text("Differenza di somiglianza fra le prime due carte; non è una probabilità di correttezza.");
                                if ui.add_enabled(fresh && self.labels[i] != 255, egui::Button::new("Inserisci")).clicked() {
                                    chosen = Some(mk(self.labels[i]));
                                    if let Some(mem) = &self.memory {
                                        // Remove a conflicting learned label before learning the user's correction.
                                        let mut mem = mem.lock().unwrap_or_else(|e| e.into_inner());
                                        if mk(self.labels[i]) != card.card { mem.forget(&card.image); }
                                        mem.remember(&card.image, card.visible, self.labels[i]);
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
    fn calibration_ui(&mut self, ui: &mut egui::Ui) {
        use vision::autosync::{Role, Zone};
        ui.horizontal_wrapped(|ui| {
            if ui.button("Definisci zone del tavolo").clicked() {
                self.edit_zones = !self.edit_zones;
            }
            ui.label(format!("{} zone", self.layout.zones.len()));
        });
        if !self.required_roles.is_empty() {
            ui.label(format!(
                "Zone richieste: {}",
                self.required_roles.iter().map(|r| r.label()).collect::<Vec<_>>().join(", ")
            ));
        }
        if !self.edit_zones {
            return;
        }
        ui.label("Scegli il ruolo e trascina sulla foto un rettangolo attorno a tutte le carte di quella zona. Le zone non devono sovrapporsi.");
        egui::ComboBox::from_id_salt("zone-role").selected_text(self.zone_role.label()).show_ui(ui, |ui| {
            for role in Role::ALL {
                ui.selectable_value(&mut self.zone_role, role, role.label());
            }
        });
        let area = *vision::screen::area().lock().unwrap_or_else(|e| e.into_inner());
        if let (Some(image), Some(texture)) = (&self.last_image, &self.table_texture) {
            let scale = (ui.available_width() / image.w as f32).min(280.0 / image.h as f32);
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(image.w as f32 * scale, image.h as f32 * scale), egui::Sense::drag());
            ui.painter().image(
                texture.id(),
                rect,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
            let screen_rect = |r: [f32; 4]| {
                egui::Rect::from_min_max(
                    rect.min + egui::vec2(r[0] * rect.width(), r[1] * rect.height()),
                    rect.min + egui::vec2(r[2] * rect.width(), r[3] * rect.height()),
                )
            };
            if self.layout.area == Some(area) {
                for zone in &self.layout.zones {
                    let r = screen_rect(zone.rect);
                    ui.painter().rect_stroke(
                        r,
                        0.0,
                        egui::Stroke::new(2.0, egui::Color32::GOLD),
                        egui::StrokeKind::Inside,
                    );
                    ui.painter().text(
                        r.left_top(),
                        egui::Align2::LEFT_TOP,
                        zone.role.label(),
                        egui::FontId::proportional(12.0),
                        egui::Color32::GOLD,
                    );
                }
            }
            if response.drag_started() {
                self.zone_drag = response.interact_pointer_pos().map(|p| (p, p));
            }
            if response.dragged() {
                if let (Some((start, _)), Some(p)) = (self.zone_drag, response.interact_pointer_pos()) {
                    self.zone_drag = Some((start, rect.clamp(p)));
                }
            }
            if let Some((start, end)) = self.zone_drag {
                let selection = egui::Rect::from_two_pos(start, end);
                ui.painter().rect_stroke(
                    selection,
                    0.0,
                    egui::Stroke::new(2.0, egui::Color32::LIGHT_GREEN),
                    egui::StrokeKind::Inside,
                );
                if response.drag_stopped() {
                    self.zone_drag = None;
                    if selection.width() > 8.0 && selection.height() > 8.0 {
                        if self.layout.area != Some(area) {
                            self.layout.zones.clear();
                        }
                        self.layout.area = Some(area);
                        self.layout.zones.retain(|z| z.role != self.zone_role);
                        self.layout.zones.push(Zone {
                            role: self.zone_role,
                            rect: [
                                (selection.min.x - rect.min.x) / rect.width(),
                                (selection.min.y - rect.min.y) / rect.height(),
                                (selection.max.x - rect.min.x) / rect.width(),
                                (selection.max.y - rect.min.y) / rect.height(),
                            ],
                        });
                        self.synchronizer = Default::default();
                    }
                }
            }
        }
        let mut remove = None;
        for (i, zone) in self.layout.zones.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(zone.role.label());
                if ui.small_button("Rimuovi").clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            self.layout.zones.remove(i);
            self.synchronizer = Default::default();
        }
        if let Err(e) = self.layout.validate(area) {
            ui.colored_label(egui::Color32::YELLOW, e);
        }
    }
    pub fn automatic_batch(&mut self, request: Option<&vision::autosync::Request>) -> Option<Vec<DeckCard>> {
        if !self.automatic {
            return None;
        }
        let Some(request) = request else {
            self.auto_status = "Completa le azioni del turno nell'assistente".into();
            return None;
        };
        let Some((_, id)) = &self.scene else {
            return None;
        };
        if self.reader.as_ref().is_none_or(|r| r.placement(*id).hidden) {
            self.synchronizer.interrupt();
            return None;
        }
        let area = *vision::screen::area().lock().unwrap_or_else(|e| e.into_inner());
        if self.scene_area != Some(area) {
            self.synchronizer.interrupt();
            return None;
        }
        if let Err(e) = self.layout.validate(area) {
            self.auto_status = e;
            return None;
        }
        let missing: Vec<_> =
            self.required_roles.iter().filter(|role| !self.layout.zones.iter().any(|z| z.role == **role)).collect();
        if !missing.is_empty() {
            self.auto_status =
                format!("Definisci le zone: {}", missing.iter().map(|r| r.label()).collect::<Vec<_>>().join(", "));
            return None;
        }
        if !self.layout.zones.iter().any(|z| z.role == request.role) {
            self.auto_status = format!("Definisci la zona «{}»", request.role.label());
            return None;
        }
        match self.synchronizer.plan(request) {
            Ok(cards) if !cards.is_empty() => Some(cards),
            Ok(_) => {
                self.synchronizer.acknowledge_existing(request);
                None
            }
            Err(e) => {
                self.automatic = false;
                self.synchronizer.interrupt();
                self.error = Some(format!("Sincronizzazione sospesa: {e}"));
                None
            }
        }
    }
    pub fn acknowledge_known(&mut self, known: &[vision::autosync::Request]) {
        if !self.automatic {
            return;
        }
        if let (Some(reader), Some((_, id))) = (&self.reader, &self.scene) {
            let area = *vision::screen::area().lock().unwrap_or_else(|e| e.into_inner());
            if !reader.placement(*id).hidden && self.scene_area == Some(area) && self.layout.validate(area).is_ok() {
                for request in known {
                    self.synchronizer.acknowledge_existing(request);
                }
            }
        }
    }
    pub fn complete_automatic(&mut self, role: vision::autosync::Role, result: Result<(), String>) {
        match result {
            Ok(()) => {
                self.synchronizer.commit(role);
                self.auto_status = "Carte sincronizzate".into();
                self.error = None;
            }
            Err(e) => {
                self.automatic = false;
                self.synchronizer.interrupt();
                self.error = Some(format!("Sincronizzazione sospesa: {e}"));
            }
        }
    }
    pub fn sight(&self, game: u8, hint: Hint) -> Sight {
        let (Some(reader), Some((scene, id))) = (&self.reader, &self.scene) else { return Sight::off() };
        let area = *vision::screen::area().lock().unwrap_or_else(|e| e.into_inner());
        if self.scene_area.is_some_and(|old| old != area) {
            return Sight::off();
        }
        let zones_valid = self.layout.validate(area).is_ok();
        let placement = reader.placement(*id);
        if placement.hidden || self.error.is_some() {
            return Sight::off();
        }
        if self.automatic
            && (self.synchronizer.has_uncommitted()
                || self.required_roles.iter().any(|role| !self.layout.zones.iter().any(|z| z.role == *role)))
        {
            return Sight {
                monitor: placement.monitor,
                hand: Vec::new(),
                guide: Guide::Decision("Attendo carte coerenti e azioni del turno".into()),
                context: (game, 0),
                played_card: None,
            };
        }
        let mut targets = Vec::new();
        let complete = !hint.cards.is_empty()
            && hint.cards.iter().all(|&wanted| {
                let found: Vec<_> = scene
                    .cards
                    .iter()
                    .enumerate()
                    .filter(|(i, c)| {
                        (self.own_hand.get(*i) == Some(&true)
                            || (zones_valid && self.layout.role_at(c.at) == Some(vision::autosync::Role::Hand)))
                            && c.card == wanted
                    })
                    .map(|(_, c)| c)
                    .collect();
                if found.len() != 1 {
                    return false;
                }
                let Some(at) = placement.on_desktop(found[0].at) else {
                    return false;
                };
                targets.push(at);
                true
            });
        let guide = if complete {
            let card = targets.remove(0);
            let identity = |c| match c {
                DeckCard::Naples(c) | DeckCard::French(c) => c,
                DeckCard::Unknown => 0,
            };
            Guide::Select {
                id: AdviceId {
                    card: identity(hint.cards[0]),
                    take: hint.cards.iter().fold(0, |mask, &c| mask | (1u64 << identity(c))),
                },
                card,
                take: targets,
                text: hint.text.clone(),
            }
        } else if hint.text.is_empty() {
            Guide::Waiting
        } else {
            Guide::Decision(hint.text.clone())
        };
        use std::hash::{Hash, Hasher};
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        hint.text.hash(&mut hash);
        Sight {
            monitor: placement.monitor,
            hand: scene
                .cards
                .iter()
                .enumerate()
                .filter(|(i, c)| {
                    self.own_hand.get(*i) == Some(&true)
                        || (zones_valid && self.layout.role_at(c.at) == Some(vision::autosync::Role::Hand))
                })
                .filter_map(|(_, c)| placement.on_desktop(c.at))
                .collect(),
            guide,
            context: (game, hash.finish()),
            played_card: None,
        }
    }
}

fn same_observation(a: &universal::SeenCard, b: &universal::SeenCard) -> bool {
    a.at.iter().zip(b.at).all(|(a, b)| (*a - b).abs() < 0.02)
        && vision::memory::similarity(&a.image, &b.image, vision::WHOLE) > 0.97
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn switching_capture_monitor_suspends_automatic_input_and_guidance() {
        let area = *vision::screen::area().lock().unwrap_or_else(|e| e.into_inner());
        let card = universal::SeenCard {
            card: DeckCard::French(12),
            image: universal::reference_image(12),
            at: [0.2, 0.2, 0.4, 0.5],
            margin: 0.2,
            visible: vision::WHOLE,
        };
        let mut visual = VisualReader {
            reader: Some(Reader::with_readings(vec![Reading::Scene(Cards { cards: vec![card] }, 2)], 2)),
            layout: vision::autosync::Layout { area: Some(area), zones: vec![] },
            automatic: true,
            auto_monitor: Some(vision::capture::Geom { x: 0.0, y: 0.0, w: 1600.0, h: 900.0, scale: 1.25 }),
            ..Default::default()
        };
        // The replay helper reports the other monitor at x=-1600.
        visual.poll(&egui::Context::default());
        assert!(!visual.automatic());
        assert!(visual.error.as_ref().unwrap().contains("Monitor"));
        assert!(visual.sight(5, Hint::text("Stai")).monitor.is_none());
    }

    #[test]
    fn delayed_delivery_does_not_fake_capture_stability() {
        use vision::autosync::{Layout, Observation, Role, Zone};
        let area = *vision::screen::area().lock().unwrap_or_else(|e| e.into_inner());
        let layout = Layout { area: Some(area), zones: vec![Zone { role: Role::Dealer, rect: [0.1, 0.1, 0.9, 0.9] }] };
        let card = universal::SeenCard {
            card: DeckCard::French(12),
            image: universal::reference_image(12),
            at: [0.2, 0.2, 0.4, 0.5],
            margin: 0.2,
            visible: vision::WHOLE,
        };
        let scene = Cards { cards: vec![card] };
        let now = std::time::Instant::now();
        // Images captured 30 ms apart, delivered roughly a second later.
        let mut visual = VisualReader {
            reader: Some(Reader::with_readings_at(
                vec![Reading::Scene(scene.clone(), 2)],
                2,
                now - Duration::from_millis(970),
            )),
            layout: layout.clone(),
            automatic: true,
            ..Default::default()
        };
        visual.synchronizer.observe(
            Observation::from_scene(&scene, &layout, area).unwrap(),
            1,
            now - Duration::from_secs(1),
        );
        visual.poll(&egui::Context::default());
        let game = crate::blackjack::BlackjackUi::new(Default::default());
        assert!(visual.automatic_batch(game.auto_request().as_ref()).is_none());
    }

    #[test]
    fn stable_zones_feed_blackjack_without_card_clicks_and_keep_manual_undo() {
        use vision::autosync::{Layout, Observation, Role, Zone};
        let area = *vision::screen::area().lock().unwrap_or_else(|e| e.into_inner());
        let layout = Layout {
            area: Some(area),
            zones: vec![
                Zone { role: Role::Dealer, rect: [0.35, 0.05, 0.55, 0.4] },
                Zone { role: Role::Hand, rect: [0.2, 0.5, 0.8, 0.95] },
            ],
        };
        let make = |c, at| universal::SeenCard {
            card: DeckCard::French(c),
            image: universal::reference_image(c),
            at,
            margin: 0.2,
            visible: vision::WHOLE,
        };
        let scene = Cards {
            cards: vec![make(12, [0.4, 0.1, 0.5, 0.35]), make(0, [0.3, 0.6, 0.4, 0.9]), make(8, [0.5, 0.6, 0.6, 0.9])],
        };
        let mut visual = VisualReader {
            reader: Some(Reader::with_readings(vec![Reading::Scene(scene.clone(), 2)], 2)),
            layout: layout.clone(),
            automatic: true,
            ..Default::default()
        };
        visual.synchronizer.observe(
            Observation::from_scene(&scene, &layout, area).unwrap(),
            1,
            std::time::Instant::now() - Duration::from_millis(250),
        );
        visual.poll(&egui::Context::default());
        let mut game = crate::blackjack::BlackjackUi::new(Default::default());
        visual.require_zones(&[Role::Dealer, Role::Hand, Role::Board]);
        assert!(visual.automatic_batch(game.auto_request().as_ref()).is_none(), "a required zone is missing");
        visual.require_zones(&[Role::Dealer, Role::Hand]);
        for expected in [Role::Dealer, Role::Hand] {
            let request = game.auto_request().unwrap();
            assert_eq!(request.role, expected);
            let batch = visual.automatic_batch(Some(&request)).expect("stable automatic input");
            let result = game.visual_batch(&batch);
            assert!(result.is_ok());
            visual.complete_automatic(request.role, result);
        }
        assert_eq!(game.visual_hint().text, "Stai");
        assert_eq!(visual.sight(5, Hint::text("Stai")).hand.len(), 2);
        let request = game.auto_request().unwrap();
        assert!(visual.automatic_batch(Some(&request)).is_none(), "same snapshot never repeats");
    }

    #[test]
    fn fly_only_selects_cards_explicitly_marked_as_our_hand() {
        let card = universal::SeenCard {
            card: DeckCard::French(0),
            image: universal::reference_image(0),
            at: [0.2, 0.1, 0.3, 0.3],
            margin: 0.5,
            visible: vision::WHOLE,
        };
        let mut visual = VisualReader {
            reader: Some(Reader::with_readings(Vec::new(), 7)),
            scene: Some((Cards { cards: vec![card.clone()] }, 7)),
            own_hand: vec![false],
            ..Default::default()
        };
        assert!(matches!(visual.sight(2, Hint::card("Scarta", card.card)).guide, Guide::Decision(_)));
        visual.own_hand[0] = true;
        let sight = visual.sight(2, Hint::card("Scarta", card.card));
        assert!(matches!(sight.guide, Guide::Select { card, .. } if card[0] < 0.0));
        visual.scene.as_mut().unwrap().0.cards.push(card.clone());
        visual.own_hand.push(true);
        assert!(matches!(visual.sight(2, Hint::card("Scarta", card.card)).guide, Guide::Decision(_)));
        visual.scene.as_mut().unwrap().1 = 6; // previous scene cannot revive a highlight
        assert!(visual.sight(2, Hint::card("Scarta", card.card)).monitor.is_none());
    }

    #[test]
    fn pending_label_and_hand_role_survive_a_fresh_read_of_the_same_artwork() {
        let ctx = egui::Context::default();
        let card = universal::SeenCard {
            card: DeckCard::Unknown,
            image: universal::reference_image(0),
            at: [0.2, 0.6, 0.3, 0.8],
            margin: 0.0,
            visible: vision::WHOLE,
        };
        let scene = Cards { cards: vec![card] };
        let mut visual = VisualReader {
            reader: Some(Reader::with_readings(vec![Reading::Scene(scene.clone(), 1)], 1)),
            ..Default::default()
        };
        visual.poll(&ctx);
        visual.labels[0] = 0;
        visual.own_hand[0] = true;
        visual.reader = Some(Reader::with_readings(vec![Reading::Scene(scene, 2)], 2));
        visual.poll(&ctx);
        assert_eq!(visual.labels, vec![0]);
        assert_eq!(visual.own_hand, vec![true]);
    }

    #[test]
    fn corrections_follow_the_same_card_but_not_replaced_artwork() {
        let a = universal::SeenCard {
            card: DeckCard::Unknown,
            image: universal::reference_image(0),
            at: [0.1, 0.1, 0.2, 0.3],
            margin: 0.0,
            visible: vision::WHOLE,
        };
        let mut b = a.clone();
        b.at[0] += 0.005;
        assert!(same_observation(&a, &b));
        b.image = universal::reference_image(12);
        assert!(!same_observation(&a, &b));
        b = a.clone();
        b.at[0] += 0.1;
        assert!(!same_observation(&a, &b));
    }
}

fn table_texture(ctx: &egui::Context, image: &vision::Img) -> egui::TextureHandle {
    let raw: Vec<u8> = image.px.iter().flatten().copied().collect();
    ctx.load_texture(
        "calibration-table",
        egui::ColorImage::from_rgb([image.w, image.h], &raw),
        egui::TextureOptions::LINEAR,
    )
}
