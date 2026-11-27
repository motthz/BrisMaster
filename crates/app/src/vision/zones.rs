use super::{capture::{self, Geom}, screen::{self, Area}, CardView, Scene};
use eframe::egui::{self, pos2, vec2, Color32, Rect, Sense, Stroke, StrokeKind};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::{Mutex, OnceLock}};

type Bounds = [f32; 4];

const LABELS: [&str; 4] = ["Briscola", "Tua mano", "Tua giocata", "Giocata avversaria"];
const COLORS: [Color32; 4] = [
    Color32::from_rgb(255, 208, 88), Color32::from_rgb(130, 202, 255),
    Color32::from_rgb(99, 219, 170), Color32::from_rgb(255, 141, 141),
];

#[derive(Clone, Copy, Deserialize, PartialEq, Serialize)]
struct Source {
    area: Area,
    monitor: [f32; 5],
}

impl Source {
    fn new(area: Area, g: Geom) -> Self {
        Self { area, monitor: [g.x, g.y, g.w, g.h, g.scale] }
    }
}

#[derive(Clone, Copy, Deserialize, Serialize)]
struct Profile {
    source: Source,
    zones: [Option<Bounds>; 4],
}

fn path() -> PathBuf {
    crate::record::dir().with_file_name("zone-briscola.json")
}

fn profile() -> &'static Mutex<Option<Profile>> {
    static CURRENT: OnceLock<Mutex<Option<Profile>>> = OnceLock::new();
    CURRENT.get_or_init(|| {
        let saved = std::fs::read(path()).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok());
        Mutex::new(saved)
    })
}

fn save(next: Option<Profile>) -> std::io::Result<()> {
    let mut current = profile().lock().unwrap_or_else(|error| error.into_inner());
    super::memory::save_atomic(&path(), &next)?;
    *current = next;
    Ok(())
}

pub(super) fn apply(scene: &mut Scene, area: Area, geom: Geom) {
    if cfg!(test) {
        return;
    }
    let current = *profile().lock().unwrap_or_else(|error| error.into_inner());
    if let Some(current) = current.filter(|current| current.source == Source::new(area, geom)) {
        assign(scene, current.zones);
    }
}

pub(crate) fn assign(scene: &mut Scene, zones: [Option<Bounds>; 4]) {
    let groups = [scene.trump.as_slice(), scene.hand.as_slice(), scene.my_slot.as_slice(), scene.opp_slot.as_slice()];
    let mut by_role: [Vec<&CardView>; 4] = Default::default();
    let mut changed = false;
    for (role, cards) in groups.into_iter().enumerate() {
        for card in cards {
            let [l, t, r, b] = card.at;
            let (x, y) = ((l + r) / 2.0, (t + b) / 2.0);
            let mut matches = zones.iter().enumerate().filter_map(|(i, bounds)| {
                let [left, top, right, bottom] = (*bounds)?;
                (l < r && t < b && (left..=right).contains(&x) && (top..=bottom).contains(&y)).then_some(i)
            });
            let target = matches.next().unwrap_or(role);
            if matches.next().is_some() {
                return;
            }
            changed |= role != target;
            by_role[target].push(card);
        }
    }
    // Mantieni le dimensioni della mano e della presa. Una giocata solitaria
    // può cambiare lato, purché ciascuna casella contenga al massimo una carta.
    if !changed || by_role[0].len() != groups[0].len() || by_role[1].len() != groups[1].len()
        || by_role[2].len() > 1 || by_role[3].len() > 1
    {
        return;
    }
    let [trump, hand, mine, opponent] = by_role;
    let mut hand: Vec<_> = hand.into_iter().cloned().collect();
    if hand.as_slice().iter().zip(&scene.hand).any(|(new, old)| new.at != old.at) {
        hand.sort_by(|a, b| a.at[0].total_cmp(&b.at[0]));
    }
    *scene = Scene {
        trump: trump.first().map(|&card| card.clone()),
        hand,
        my_slot: mine.first().map(|&card| card.clone()),
        opp_slot: opponent.first().map(|&card| card.clone()),
    };
}

#[derive(Default)]
pub struct Editor {
    open: bool,
    selected: usize,
    draft: Option<Profile>,
    shot: Option<egui::TextureHandle>,
    drag: Option<egui::Pos2>,
    error: Option<String>,
}

impl Editor {
    fn capture(&mut self, ctx: &egui::Context, keep_changes: bool) {
        let area = *screen::area().lock().unwrap_or_else(|error| error.into_inner());
        self.shot = None;
        self.drag = None;
        match capture::grab_geom(area.monitor) {
            Ok((image, _, geom)) => {
                let source = Source::new(area, geom);
                let current = *profile().lock().unwrap_or_else(|error| error.into_inner());
                let draft = self.draft.filter(|draft| keep_changes && draft.source == source);
                self.draft = Some(draft.or(current.filter(|saved| saved.source == source))
                    .unwrap_or(Profile { source, zones: [None; 4] }));
                let image = screen::table(&image, &area);
                let pixels = image.px.iter().map(|p| Color32::from_rgb(p[0], p[1], p[2])).collect();
                self.shot = Some(ctx.load_texture("briscola_editor_zone",
                    egui::ColorImage::new([image.w, image.h], pixels), Default::default()));
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub fn controls(&mut self, ui: &mut egui::Ui) {
        if ui.small_button("Modifica zone").clicked() {
            self.open = true;
            self.capture(ui.ctx(), false);
        }
        if !self.open {
            return;
        }
        let mut open = true;
        egui::Window::new("Zone del tavolo di Briscola")
            .open(&mut open)
            .default_size([800.0, 600.0])
            .show(ui.ctx(), |ui| self.window(ui));
        self.open &= open;
    }

    fn window(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            for (i, label) in LABELS.into_iter().enumerate() {
                if ui.selectable_label(self.selected == i, egui::RichText::new(label).color(COLORS[i])).clicked() {
                    self.selected = i;
                }
            }
            if ui.button("Aggiorna foto").clicked() {
                self.capture(ui.ctx(), true);
            }
        });
        ui.label("Scegli un ruolo e ridisegna il riquadro trascinando sulla foto. Salva per applicare le modifiche.");
        if let Some(error) = &self.error {
            ui.colored_label(Color32::LIGHT_RED, error);
        }
        if let (Some(shot), Some(draft)) = (&self.shot, &mut self.draft) {
            let available = (ui.available_size() - vec2(0.0, 42.0)).max(vec2(120.0, 100.0));
            let size = shot.size_vec2();
            let (rect, response) = ui.allocate_exact_size(size * (available.x / size.x).min(available.y / size.y).min(1.0), Sense::drag());
            ui.painter().image(shot.id(), rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            for (bounds, color) in draft.zones.iter().zip(COLORS) {
                if let Some([l, t, r, b]) = *bounds {
                    let point = |x, y| rect.min + vec2(x * rect.width(), y * rect.height());
                    ui.painter().rect_stroke(Rect::from_min_max(point(l, t), point(r, b)),
                        2.0, Stroke::new(2.0, color), StrokeKind::Inside);
                }
            }
            let pointer = response.interact_pointer_pos();
            if response.drag_started() {
                self.drag = pointer.map(|p| rect.clamp(p));
            }
            if let (Some(start), Some(end)) = (self.drag, pointer) {
                let selection = Rect::from_two_pos(start, rect.clamp(end));
                ui.painter().rect_stroke(selection, 2.0, Stroke::new(2.0, COLORS[self.selected]), StrokeKind::Inside);
                if response.drag_stopped() && selection.width() >= 6.0 && selection.height() >= 6.0 {
                    let normalize = |p: egui::Pos2| (p - rect.min) / rect.size();
                    let a = normalize(selection.min);
                    let b = normalize(selection.max);
                    draft.zones[self.selected] = Some([a.x, a.y, b.x, b.y]);
                }
            }
            if response.drag_stopped() {
                self.drag = None;
            }
        }
        ui.horizontal_wrapped(|ui| {
            if ui.button("Cancella zona selezionata").clicked() {
                if let Some(draft) = &mut self.draft {
                    draft.zones[self.selected] = None;
                }
            }
            let save_draft = ui.add_enabled(self.shot.is_some(), egui::Button::new("Salva zone")).clicked();
            let automatic = ui.button("Ripristina automatico").clicked();
            if save_draft || automatic {
                let next = self.draft.filter(|draft| !automatic && draft.zones.iter().any(Option::is_some));
                match save(next) {
                    Ok(()) => self.open = false,
                    Err(error) => self.error = Some(format!("Salvataggio non riuscito: {error}")),
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vision::{Img, WHOLE};

    fn card(x: f32) -> CardView {
        let mut view = CardView::new(Img::new(1, 1), WHOLE, [0.0; 40]);
        view.at = [x - 0.02, 0.4, x + 0.02, 0.6];
        view
    }

    #[test]
    fn manual_zones_correct_all_four_roles() {
        let mut scene = Scene {
            trump: Some(card(0.3)),
            hand: vec![card(0.1)],
            my_slot: Some(card(0.7)),
            opp_slot: Some(card(0.5)),
        };
        let zones = [
            Some([0.0, 0.3, 0.2, 0.7]), Some([0.2, 0.3, 0.4, 0.7]),
            Some([0.4, 0.3, 0.6, 0.7]), Some([0.6, 0.3, 0.8, 0.7]),
        ];
        assign(&mut scene, zones);
        assert_eq!(scene.trump.unwrap().at, card(0.1).at);
        assert_eq!(scene.hand[0].at, card(0.3).at);
        assert_eq!(scene.my_slot.unwrap().at, card(0.5).at);
        assert_eq!(scene.opp_slot.unwrap().at, card(0.7).at);
    }

    #[test]
    fn ambiguous_zones_and_missing_roles_keep_the_automatic_reading() {
        let mut scene = Scene { my_slot: Some(card(0.5)), ..Scene::default() };
        let overlapping = Some([0.3, 0.3, 0.7, 0.7]);
        assign(&mut scene, [None, None, overlapping, overlapping]);
        assert!(scene.my_slot.is_some() && scene.opp_slot.is_none());
        assign(&mut scene, [None, overlapping, None, None]);
        assert!(scene.my_slot.is_some() && scene.opp_slot.is_none());
        assign(&mut scene, [None, None, None, overlapping]);
        assert!(scene.my_slot.is_none() && scene.opp_slot.is_some());
    }
}
