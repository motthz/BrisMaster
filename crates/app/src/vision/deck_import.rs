//! Preparazione e anteprima di un mazzo locale prima di aggiornare la memoria.
use super::{capture, memory, Img, Memory, CARD_H, CARD_W, WHOLE};
use briscola_core::cards;
use eframe::egui;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;

type Corners = [[f32; 2]; 4];
const RECTANGLE: Corners = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];

/// Da un quadrilatero convesso, in ordine orario a partire dall'angolo alto
/// sinistro, a una carta frontale. Gli angoli sono frazioni del ritaglio.
fn rectify(source: &Img, corners: Corners) -> Option<Img> {
    if source.w < 2 || source.h < 2
        || corners.iter().flatten().any(|x| !(0.0..=1.0).contains(x)) {
        return None;
    }
    let cross = |a: [f32; 2], b: [f32; 2], c: [f32; 2]| {
        (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0])
    };
    if (0..4).any(|i| cross(corners[i], corners[(i+1)%4], corners[(i+2)%4]) <= 0.005) {
        return None;
    }
    let [a, b, c, d] = corners;
    let dx1 = b[0] - c[0];
    let dx2 = d[0] - c[0];
    let dy1 = b[1] - c[1];
    let dy2 = d[1] - c[1];
    let dx3 = a[0] - b[0] + c[0] - d[0];
    let dy3 = a[1] - b[1] + c[1] - d[1];
    let det = dx1 * dy2 - dx2 * dy1;
    if det.abs() < 0.005 { return None; }
    let g = (dx3 * dy2 - dx2 * dy3) / det;
    let h = (dx1 * dy3 - dx3 * dy1) / det;
    if (1.0 + g).min(1.0 + h).min(1.0 + g + h) < 0.05 { return None; }
    let mut result = Img::new(CARD_W, CARD_H);
    for y in 0..CARD_H {
        let v = y as f32 / (CARD_H - 1) as f32;
        for x in 0..CARD_W {
            let u = x as f32 / (CARD_W - 1) as f32;
            let denom = 1.0 + g * u + h * v;
            let coordinate = |axis| {
                (a[axis] + (b[axis] * (1.0 + g) - a[axis]) * u
                    + (d[axis] * (1.0 + h) - a[axis]) * v) / denom
            };
            result.px[y * CARD_W + x] = source.sample(
                coordinate(0).clamp(0.0, 1.0) * (source.w - 1) as f32,
                coordinate(1).clamp(0.0, 1.0) * (source.h - 1) as f32,
            );
        }
    }
    Some(result)
}

fn load(path: &Path) -> Result<[Img; 40], String> {
    let mut list = Vec::with_capacity(40);
    let sidecar = path.join("prospettiva.json");
    let corners: BTreeMap<String, Corners> = match std::fs::read(&sidecar) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| format!("prospettiva.json: {e}"))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
        Err(e) => return Err(e.to_string()),
    };
    let missing: Vec<_> = (0..40).filter_map(|card| {
        let name = format!("{card:02}.png");
        (!path.join(&name).is_file()).then_some(name)
    }).collect();
    if !missing.is_empty() { return Err(format!("File mancanti: {}", missing.join(", "))); }
    for card in 0..40u8 {
        let name = format!("{card:02}.png");
        let img = capture::load_png(&path.join(&name)).map_err(|e| format!("{name}: {e}"))?;
        let rectified = rectify(&img, corners.get(&name).copied().unwrap_or(RECTANGLE))
            .ok_or_else(|| format!("Dimensioni o angoli non validi in {name}"))?;
        if memory::faint(&rectified) { return Err(format!("{name} non ha un disegno leggibile")); }
        if let Some((other, _)) = list.iter().enumerate().find(|(_, other)| {
            memory::similarity(&rectified, other, WHOLE) > 0.995
        }) {
            return Err(format!("Immagini quasi identiche: {other:02}.png e {name}"));
        }
        list.push(rectified);
    }
    list.try_into().map_err(|_| "Il mazzo deve avere 40 carte".into())
}

#[derive(Default)]
pub(super) struct DeckImport {
    open: bool,
    path: String,
    message: String,
    pending: Option<([Img; 40], Vec<egui::TextureHandle>)>,
}

impl DeckImport {
    pub(super) fn controls(&mut self, ui: &mut egui::Ui, memory: &Mutex<Memory>) {
        if ui.small_button("Importa mazzo").clicked() { self.open = true; }
        let ctx = ui.ctx().clone();
        egui::Window::new("Importa mazzo")
            .open(&mut self.open)
            .show(&ctx, |ui| {
                ui.label("Cartella con 00.png–39.png. Ordine dei semi: denari, coppe, spade, bastoni. Ordine dei valori: asso–sette, fante, cavallo, re.");
                if ui.text_edit_singleline(&mut self.path).changed() { self.pending = None; }
                if ui.button("Verifica e mostra le carte").clicked() {
                    self.pending = None;
                    match load(Path::new(self.path.trim())) {
                        Ok(images) => {
                            let textures = images.iter().enumerate().map(|(i, img)| {
                                let pixels: Vec<u8> = img.px.iter().flatten().copied().collect();
                                ctx.load_texture(format!("import_deck_{i}"),
                                    egui::ColorImage::from_rgb([img.w, img.h], &pixels), Default::default())
                            }).collect();
                            self.pending = Some((images, textures));
                            self.message = "Controlla che ogni immagine corrisponda al nome mostrato.".into();
                        }
                        Err(error) => self.message = error,
                    }
                }
                if let Some((images, textures)) = &self.pending {
                    egui::ScrollArea::vertical().max_height(350.0).show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            for (i, texture) in textures.iter().enumerate() {
                                ui.vertical(|ui| {
                                    ui.image((texture.id(), egui::vec2(40.0, 64.0)));
                                    ui.label(cards::short(i as u8));
                                });
                            }
                        });
                    });
                    if ui.button("Confermo le 40 carte e importo").clicked() {
                        let result = memory.lock().unwrap_or_else(|e| e.into_inner()).import_deck(images);
                        match result {
                            Ok(()) => {
                                self.pending = None;
                                self.message = "Mazzo importato.".into();
                            }
                            Err(error) => self.message = error,
                        }
                    }
                }
                ui.label(&self.message);
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_missing_files_and_imports_a_complete_verified_deck() {
        let root = std::env::temp_dir().join(format!("brismaster-deck-import-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let error = load(&root).unwrap_err();
        assert!(error.contains("00.png") && error.contains("39.png"));
        for (card, image) in super::super::reference::reference_cards().into_iter().take(40) {
            image.save_png(&root.join(format!("{card:02}.png"))).unwrap();
        }
        let images = load(&root).expect("mazzo completo");
        let mut memory = Memory::in_memory();
        memory.import_deck(&images).unwrap();
        assert_eq!(memory.len(), 40);
        std::fs::copy(root.join("00.png"), root.join("01.png")).unwrap();
        assert!(load(&root).unwrap_err().contains("identiche"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn perspective_rectification_matches_the_corners() {
        let mut img = Img::new(100, 100);
        for y in 0..100 {
            for x in 0..100 {
                img.px[y * 100 + x] = [x as u8, y as u8, 0];
            }
        }
        let corners = [[0.2, 0.1], [0.8, 0.2], [0.9, 0.9], [0.1, 0.8]];
        let result = rectify(&img, corners).unwrap();
        assert_eq!(result.at(0, 0), [20, 10, 0]);
        assert_eq!(result.at(CARD_W - 1, CARD_H - 1), [89, 89, 0]);
        assert!(rectify(&img, [corners[0], corners[2], corners[1], corners[3]]).is_none());
    }
}
