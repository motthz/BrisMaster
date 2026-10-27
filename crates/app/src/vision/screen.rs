//! Dove guardare: monitor e area del tavolo scelti dall'utente (o trovati da soli), e la diagnosi
//! da mandare quando la lettura sbaglia.
//!
//! Senza un'area scelta, della schermata si tiene solo la zona verde più grande (il panno del
//! tavolo): così icone, finestre e sfondi del desktop non vengono scambiati per carte.

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

    pub fn save_png(&self, path: &std::path::Path) -> Result<(), String> {
        let raw: Vec<u8> = self.px.iter().flatten().copied().collect();
        let im = image::RgbImage::from_raw(self.w as u32, self.h as u32, raw).ok_or("immagine non valida")?;
        im.save(path).map_err(|e| e.to_string())
    }
}

/// Riquadro della zona verde più grande (passo di 4 pixel), se copre almeno il 3% dello schermo.
pub fn felt_box(img: &Img) -> Option<(usize, usize, usize, usize)> {
    const S: usize = 4;
    let (w, h) = (img.w / S, img.h / S);
    if w == 0 || h == 0 {
        return None;
    }
    let felt: Vec<bool> = (0..w * h).map(|i| detect::is_felt(img.at((i % w) * S, (i / w) * S))).collect();
    let mut seen = vec![false; w * h];
    let mut best: Option<(usize, (usize, usize, usize, usize))> = None;
    let mut stack = Vec::new();
    for s in 0..w * h {
        if !felt[s] || seen[s] {
            continue;
        }
        seen[s] = true;
        stack.push(s);
        let (mut n, mut x0, mut y0, mut x1, mut y1) = (0, usize::MAX, usize::MAX, 0, 0);
        while let Some(i) = stack.pop() {
            n += 1;
            let (x, y) = (i % w, i / w);
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
            for (nx, ny) in [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1)), (x, y + 1)] {
                if nx < w && ny < h {
                    let j = ny * w + nx;
                    if felt[j] && !seen[j] {
                        seen[j] = true;
                        stack.push(j);
                    }
                }
            }
        }
        if best.is_none_or(|(bn, _)| n > bn) {
            best = Some((n, (x0, y0, x1, y1)));
        }
    }
    let (n, (x0, y0, x1, y1)) = best?;
    if n * 100 < 3 * w * h {
        return None;
    }
    Some((x0 * S, y0 * S, ((x1 + 1) * S).min(img.w), ((y1 + 1) * S).min(img.h)))
}

/// La parte di schermata da leggere: l'area scelta dall'utente, altrimenti il panno verde.
pub fn table(img: &Img, area: &Area) -> Option<Img> {
    if let Some([l, t, r, b]) = area.rect {
        let (w, h) = (img.w as f32, img.h as f32);
        return Some(img.crop((l * w) as usize, (t * h) as usize, (r * w) as usize, (b * h) as usize));
    }
    let (x0, y0, x1, y1) = felt_box(img)?;
    Some(img.crop(x0, y0, x1, y1))
}

/// Copia dell'immagine con disegnato quello che la lettura ha trovato: in verde le carte scoperte
/// usate, in rosso le zone chiare scartate, in blu i dorsi.
pub fn diagnose(img: &Img) -> Img {
    let mut out = img.clone();
    for b in detect::blobs(img) {
        let color = if detect::is_face(&b) {
            [0, 220, 0]
        } else if b.white <= 0.3 && b.felt > 0.3 {
            [40, 90, 255]
        } else {
            [230, 0, 0]
        };
        let r = b.rect;
        for k in 0..400 {
            let t = k as f32 / 400.0 * 4.0;
            let (a, bb) = match t as u32 {
                0 => (-r.hw + 2.0 * r.hw * t.fract(), -r.hh),
                1 => (r.hw, -r.hh + 2.0 * r.hh * t.fract()),
                2 => (r.hw - 2.0 * r.hw * t.fract(), r.hh),
                _ => (-r.hw, r.hh - 2.0 * r.hh * t.fract()),
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
        ui.label("Apri la partita nel browser, poi trascina un rettangolo attorno al tavolo verde (carte tue, carte in tavola e mazzo).");
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

    /// Il panno del sito si ritaglia da solo anche dentro uno schermo più grande con altre cose attorno.
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
        let (x0, y0, x1, y1) = felt_box(&screen).expect("tavolo");
        assert!(x0 <= ox + 8 && y0 <= oy + 8 && x1 + 8 >= ox + site.w && y1 + 8 >= oy + site.h, "{x0},{y0},{x1},{y1}");
        assert!(x0 + 40 >= ox && y0 + 40 >= oy, "niente desktop attorno: {x0},{y0}");
        let table = table(&screen, &Area::default()).unwrap();
        let scene = detect::scene(&table);
        assert_eq!(scene.hand.len(), 2);
        assert!(scene.opp_slot.is_some() && scene.my_slot.is_some() && scene.trump.is_some());
    }
}
