//! Dove guardare: monitor e area del tavolo scelti dall'utente (o tutto il monitor), e la diagnosi
//! da mandare quando la lettura sbaglia.

use super::detect;
use super::Img;
use eframe::egui::{self, Color32, RichText, Sense, Stroke, StrokeKind};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

/// Monitor e area del tavolo (frazioni dello schermo: sinistra, alto, destra, basso).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct Area {
    pub monitor: usize,
    pub rect: Option<[f32; 4]>,
}

fn path() -> PathBuf {
    crate::record::dir().parent().map(|p| p.to_path_buf()).unwrap_or_default().join("schermo.json")
}

/// L'area in uso, condivisa da tutti i giochi e salvata in Documenti\BrisMaster\schermo.json.
pub fn area() -> &'static Mutex<Area> {
    static AREA: OnceLock<Mutex<Area>> = OnceLock::new();
    AREA.get_or_init(|| {
        let a = std::fs::read_to_string(path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        Mutex::new(a)
    })
}

fn set_area(a: Area) {
    *area().lock().unwrap() = a;
    let p = path();
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let _ = std::fs::write(p, serde_json::to_string(&a).unwrap_or_default());
}

impl Img {
    /// Ritaglio [x0, x1) x [y0, y1), limitato all'immagine.
    pub fn crop(&self, x0: usize, y0: usize, x1: usize, y1: usize) -> Img {
        let (x1, y1) = (x1.min(self.w), y1.min(self.h));
        let (x0, y0) = (x0.min(x1), y0.min(y1));
        let mut px = Vec::with_capacity((x1 - x0) * (y1 - y0));
        for y in y0..y1 {
            px.extend_from_slice(&self.px[y * self.w + x0..y * self.w + x1]);
        }
        Img { w: x1 - x0, h: y1 - y0, px }
    }

    /// Rimpicciolisce di `k` volte (media dei blocchi k x k).
    pub fn shrink(&self, k: usize) -> Img {
        let (w, h) = (self.w / k, self.h / k);
        let mut px = Vec::with_capacity(w * h);
        for y in 0..h {
            for x in 0..w {
                let mut acc = [0u32; 3];
                for dy in 0..k {
                    for dx in 0..k {
                        let p = self.at(x * k + dx, y * k + dy);
                        for c in 0..3 {
                            acc[c] += p[c] as u32;
                        }
                    }
                }
                px.push(acc.map(|a| (a / (k * k) as u32) as u8));
            }
        }
        Img { w, h, px }
    }

    pub fn save_png(&self, path: &std::path::Path) -> Result<(), String> {
        let raw: Vec<u8> = self.px.iter().flatten().copied().collect();
        let im = image::RgbImage::from_raw(self.w as u32, self.h as u32, raw).ok_or("immagine non valida")?;
        im.save(path).map_err(|e| e.to_string())
    }
}

/// La parte di schermata da leggere: l'area scelta dall'utente, altrimenti tutto il monitor (le
/// carte si riconoscono dal disegno, il resto del desktop si scarta da solo). Gli schermi molto
/// grandi si rimpiccioliscono: le carte restano abbastanza grandi e la lettura è più veloce.
pub fn table(img: &Img, area: &Area) -> Img {
    let img = match area.rect {
        Some([l, t, r, b]) => {
            let (w, h) = (img.w as f32, img.h as f32);
            img.crop((l * w) as usize, (t * h) as usize, (r * w) as usize, (b * h) as usize)
        }
        None => img.clone(),
    };
    let k = img.w.div_ceil(2000).max(img.h.div_ceil(1300));
    if k > 1 {
        img.shrink(k)
    } else {
        img
    }
}

/// Copia dell'immagine con disegnato quello che la lettura ha trovato: in verde le carte
/// riconosciute (intere, anche dove sono coperte), in rosso le zone chiare scartate, in blu i dorsi.
pub fn diagnose(img: &Img) -> Img {
    let mut out = img.clone();
    let a = detect::analyze(img);
    let mut draw = |r: &detect::RotRect, color: [u8; 3]| {
        for k in 0..400 {
            let t = k as f32 / 400.0 * 4.0;
            let (hw, hh) = (r.hw, r.hh);
            let (a, bb) = match t as u32 {
                0 => (-hw + 2.0 * hw * t.fract(), -hh),
                1 => (hw, -hh + 2.0 * hh * t.fract()),
                2 => (hw - 2.0 * hw * t.fract(), hh),
                _ => (-hw, hh - 2.0 * hh * t.fract()),
            };
            for d in -1..=1 {
                let x = r.cx + r.u.0 * a + r.v.0 * bb + d as f32;
                let y = r.cy + r.u.1 * a + r.v.1 * bb;
                if x >= 0.0 && y >= 0.0 && (x as usize) < out.w && (y as usize) < out.h {
                    let i = y as usize * out.w + x as usize;
                    out.px[i] = color;
                }
            }
        }
    };
    for b in &a.blobs {
        if b.light() {
            draw(&b.rect, [230, 0, 0]);
        } else if b.bg > 0.3 {
            draw(&b.rect, [40, 90, 255]);
        }
    }
    for c in &a.cards {
        draw(&c.rect, [0, 220, 0]);
    }
    out
}

/// Salva in Documenti\BrisMaster\Diagnosi la schermata letta e la stessa con i riquadri trovati,
/// più un testo con quello che è stato riconosciuto. Restituisce la cartella.
pub fn save_diagnosis(img: &Img, summary: &str) -> Result<PathBuf, String> {
    let dir = crate::record::dir().parent().map(|p| p.to_path_buf()).unwrap_or_default().join("Diagnosi");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let stamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
    img.save_png(&dir.join(format!("schermo_{stamp}.png")))?;
    diagnose(img).save_png(&dir.join(format!("riquadri_{stamp}.png")))?;
    std::fs::write(dir.join(format!("letto_{stamp}.txt")), summary).map_err(|e| e.to_string())?;
    Ok(dir)
}

/// Finestra per scegliere monitor e area del tavolo trascinando un rettangolo sulla schermata.
#[derive(Default)]
pub struct AreaPicker {
    open: bool,
    monitor: usize,
    monitors: usize,
    shot: Option<(Img, egui::TextureHandle)>,
    error: Option<String>,
    drag: Option<(egui::Pos2, egui::Pos2)>,
}

impl AreaPicker {
    fn grab(&mut self, ctx: &egui::Context) {
        match super::capture::grab(self.monitor) {
            Ok((img, n)) => {
                self.monitors = n;
                let rgba: Vec<u8> = img.px.iter().flat_map(|p| [p[0], p[1], p[2], 255]).collect();
                let tex = ctx.load_texture(
                    "area_picker",
                    egui::ColorImage::from_rgba_unmultiplied([img.w, img.h], &rgba),
                    Default::default(),
                );
                self.shot = Some((img, tex));
                self.error = None;
            }
            Err(e) => self.error = Some(e),
        }
        self.drag = None;
    }

    /// Pulsanti da mettere sotto l'interruttore della lettura dello schermo. `last` è l'ultima
    /// schermata letta (per la diagnosi), `summary` cosa ci è stato riconosciuto.
    pub fn controls(&mut self, ui: &mut egui::Ui, last: Option<Img>, summary: &str) {
        let a = *area().lock().unwrap();
        ui.horizontal_wrapped(|ui| {
            let label = if a.rect.is_some() {
                format!("Area del tavolo: scelta (monitor {})", a.monitor + 1)
            } else {
                "Area del tavolo: automatica".to_string()
            };
            ui.label(RichText::new(label).small().weak());
            if ui
                .small_button("Scegli area")
                .on_hover_text("Fotografa lo schermo e trascina un rettangolo attorno al tavolo del sito")
                .clicked()
            {
                self.open = true;
                self.monitor = a.monitor;
                self.grab(ui.ctx());
            }
            if a.rect.is_some() && ui.small_button("Automatica").clicked() {
                set_area(Area { monitor: a.monitor, rect: None });
            }
            let diag = ui
                .add_enabled(last.is_some(), egui::Button::new("Salva diagnosi").small())
                .on_hover_text("Salva quello che l'app vede, da mandare se la lettura sbaglia");
            if diag.clicked() {
                if let Some(img) = &last {
                    match save_diagnosis(img, summary) {
                        Ok(dir) => {
                            self.error = Some(format!("Diagnosi salvata in {}", dir.display()));
                            open_dir(&dir);
                        }
                        Err(e) => self.error = Some(format!("Diagnosi non salvata: {e}")),
                    }
                }
            }
        });
        if !self.open {
            if let Some(e) = &self.error {
                ui.label(RichText::new(e).small());
            }
            return;
        }
        let ctx = ui.ctx().clone();
        let mut open = self.open;
        egui::Window::new("Scegli l'area del tavolo")
            .open(&mut open)
            .default_size([900.0, 620.0])
            .show(&ctx, |ui| self.window(ui));
        self.open &= open;
    }

    fn window(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Monitor:");
            for m in 0..self.monitors.max(1) {
                if ui.selectable_label(self.monitor == m, format!("{}", m + 1)).clicked() && self.monitor != m {
                    self.monitor = m;
                    self.grab(ui.ctx());
                }
            }
            if ui.button("Rifai la foto").clicked() {
                self.grab(ui.ctx());
            }
        });
        ui.label("Apri la partita nel browser, poi trascina un rettangolo attorno al tavolo (carte tue, carte in tavola e mazzo).");
        if let Some(e) = &self.error {
            ui.label(RichText::new(e).color(Color32::LIGHT_RED));
        }
        let Some((img, tex)) = &self.shot else { return };
        let avail = ui.available_size() - egui::vec2(0.0, 40.0);
        let scale = (avail.x / img.w as f32).min(avail.y / img.h as f32).min(1.0);
        let size = egui::vec2(img.w as f32 * scale, img.h as f32 * scale);
        let (rect, resp) = ui.allocate_exact_size(size, Sense::drag());
        ui.painter().image(
            tex.id(),
            rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
        if resp.drag_started() {
            if let Some(p) = resp.interact_pointer_pos() {
                self.drag = Some((p, p));
            }
        }
        if resp.dragged() {
            if let (Some((s, _)), Some(p)) = (self.drag, resp.interact_pointer_pos()) {
                self.drag = Some((s, rect.clamp(p)));
            }
        }
        let sel = self.drag.map(|(a, b)| egui::Rect::from_two_pos(a, b));
        if let Some(s) = sel {
            ui.painter().rect_stroke(s, 0.0, Stroke::new(2.0, Color32::YELLOW), StrokeKind::Outside);
        }
        let (w, h) = (img.w, img.h);
        ui.horizontal(|ui| {
            let ok = sel.is_some_and(|s| s.width() > 40.0 && s.height() > 40.0);
            if ui.add_enabled(ok, egui::Button::new("Usa quest'area")).clicked() {
                let s = sel.unwrap();
                let f = |p: egui::Pos2| ((p.x - rect.min.x) / rect.width(), (p.y - rect.min.y) / rect.height());
                let (l, t) = f(s.min);
                let (r, b) = f(s.max);
                set_area(Area { monitor: self.monitor, rect: Some([l, t, r, b]) });
                self.open = false;
            }
            if ui.button("Tutto il monitor (automatica)").clicked() {
                set_area(Area { monitor: self.monitor, rect: None });
                self.open = false;
            }
            ui.label(RichText::new(format!("{w}×{h} pixel")).small().weak());
        });
    }
}

fn open_dir(d: &std::path::Path) {
    #[cfg(windows)]
    let _ = std::process::Command::new("explorer").arg(d).spawn();
    #[cfg(not(windows))]
    let _ = std::process::Command::new("xdg-open").arg(d).spawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Il tavolo del sito si legge anche dentro uno schermo grande con altre cose attorno.
    #[test]
    fn finds_the_table_on_a_busy_screen() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/sito_briscola.png");
        let site = super::super::capture::load_png(&path).unwrap();
        // Desktop 2× più grande: sfondo grigio con rettangoli bianchi (finestre, icone).
        let mut screen = Img::new(site.w * 2, site.h + 300);
        for p in screen.px.iter_mut() {
            *p = [90, 90, 100];
        }
        for (x0, y0) in [(100, 80), (400, 900), (2500, 100), (2600, 1100)] {
            for y in y0..y0 + 160 {
                for x in x0..x0 + 100 {
                    if x < screen.w && y < screen.h {
                        screen.px[y * screen.w + x] = [250, 250, 250];
                    }
                }
            }
        }
        let (ox, oy) = (900, 200);
        for y in 0..site.h {
            for x in 0..site.w {
                screen.px[(y + oy) * screen.w + x + ox] = site.at(x, y);
            }
        }
        // Una carta uguale fuori dal tavolo (un'immagine in un'altra finestra) non conta.
        for y in 0..186 {
            for x in 0..125 {
                screen.px[(1120 + y) * screen.w + 100 + x] = site.at(585 + x, 787 + y);
            }
        }
        let table = table(&screen, &Area::default());
        assert!(table.w <= 2000, "schermo rimpicciolito");
        let scene = detect::scene(&table);
        assert_eq!(scene.hand.len(), 2);
        assert!(scene.opp_slot.is_some() && scene.my_slot.is_some() && scene.trump.is_some());
        let asso = scene.hand[1].best(briscola_core::cards::CardSet::FULL).unwrap().0;
        assert_eq!(asso, briscola_core::cards::make(1, 0));
    }
}
