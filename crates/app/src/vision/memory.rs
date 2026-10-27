//! Le carte già riconosciute restano in memoria come piccole immagini: la volta dopo la stessa
//! carta si riconosce confrontando i pixel, senza dubbi su figure e semi. Si salvano da sole,
//! in Documenti\BrisMaster\carte_viste.json, a ogni carta inserita nella partita.

use super::{Img, CARD_H, CARD_W};
use briscola_core::cards::Card;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

const TW: usize = 25;
const TH: usize = 40;

#[derive(Serialize, Deserialize, Clone)]
struct Template {
    card: Card,
    /// TW x TH x 3 valori RGB, senza il bordo della carta.
    px: Vec<u8>,
}

#[derive(Serialize, Deserialize, Default)]
pub struct Memory {
    templates: Vec<Template>,
    #[serde(skip)]
    path: Option<PathBuf>,
}

/// Miniatura della carta raddrizzata, senza il bordo.
fn thumb(img: &Img) -> Vec<u8> {
    let m = CARD_W as f32 / 12.0;
    let (w, h) = (CARD_W as f32 - 2.0 * m, CARD_H as f32 - 2.0 * m);
    let mut out = Vec::with_capacity(TW * TH * 3);
    for j in 0..TH {
        for i in 0..TW {
            // Media di un blocchetto di pixel.
            let (x0, y0) = (m + i as f32 * w / TW as f32, m + j as f32 * h / TH as f32);
            let mut acc = [0u32; 3];
            let mut n = 0;
            let (bw, bh) = ((w / TW as f32).ceil() as usize, (h / TH as f32).ceil() as usize);
            for dy in 0..bh {
                for dx in 0..bw {
                    let (x, y) = ((x0 as usize + dx).min(CARD_W - 1), (y0 as usize + dy).min(CARD_H - 1));
                    let p = img.at(x, y);
                    for c in 0..3 {
                        acc[c] += p[c] as u32;
                    }
                    n += 1;
                }
            }
            out.extend(acc.iter().map(|a| (a / n) as u8));
        }
    }
    out
}

/// Righe della miniatura dentro la fascia visibile della carta (frazioni dell'altezza).
fn rows(vis: [f32; 2]) -> (usize, usize) {
    let r0 = ((vis[0] * TH as f32).ceil() as usize).min(TH);
    let r1 = ((vis[1] * TH as f32).floor() as usize).clamp(r0, TH);
    (r0, r1)
}

/// Correlazione normalizzata fra due miniature, sulle righe `rows`.
fn ncc(a: &[u8], b: &[u8], (r0, r1): (usize, usize)) -> f32 {
    let (a, b) = (&a[r0 * TW * 3..r1 * TW * 3], &b[r0 * TW * 3..r1 * TW * 3]);
    let n = a.len();
    if n == 0 {
        return 0.0;
    }
    let ma = a.iter().map(|&v| v as f32).sum::<f32>() / n as f32;
    let mb = b.iter().map(|&v| v as f32).sum::<f32>() / n as f32;
    let (mut num, mut da, mut db) = (0.0, 0.0, 0.0);
    for i in 0..n {
        let (x, y) = (a[i] as f32 - ma, b[i] as f32 - mb);
        num += x * y;
        da += x * x;
        db += y * y;
    }
    if da == 0.0 || db == 0.0 {
        0.0
    } else {
        num / (da * db).sqrt()
    }
}

/// Somiglianza oltre la quale la carta è considerata la stessa.
pub const SAME: f32 = 0.9;

/// Somiglianza fra due carte raddrizzate, sulla fascia visibile `vis`.
pub fn similarity(a: &Img, b: &Img, vis: [f32; 2]) -> f32 {
    ncc(&thumb(a), &thumb(b), rows(vis))
}

impl Memory {
    pub fn path() -> PathBuf {
        crate::record::dir().parent().map(|p| p.to_path_buf()).unwrap_or_default().join("carte_viste.json")
    }

    /// La memoria comune a tutti i giochi (stesse carte), caricata la prima volta che serve.
    pub fn shared() -> Arc<Mutex<Memory>> {
        static SHARED: OnceLock<Arc<Mutex<Memory>>> = OnceLock::new();
        SHARED.get_or_init(|| Arc::new(Mutex::new(Memory::load()))).clone()
    }

    pub fn load() -> Memory {
        let path = Memory::path();
        let mut m: Memory =
            std::fs::read_to_string(&path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        m.path = Some(path);
        m
    }

    pub fn len(&self) -> usize {
        let mut cards: Vec<Card> = self.templates.iter().map(|t| t.card).collect();
        cards.sort();
        cards.dedup();
        cards.len()
    }

    /// Per ogni carta la somiglianza migliore con le immagini ricordate (None se mai vista),
    /// sulla fascia visibile `vis`.
    pub fn similarity(&self, img: &Img, vis: [f32; 2]) -> [Option<f32>; 40] {
        let t = thumb(img);
        let rows = rows(vis);
        let mut out = [None; 40];
        for tpl in &self.templates {
            let s = ncc(&t, &tpl.px, rows);
            let o = &mut out[tpl.card as usize];
            if o.is_none_or(|v| s > v) {
                *o = Some(s);
            }
        }
        out
    }

    /// Ricorda una carta vista intera. Tiene al massimo 3 immagini per carta (luci e rotazioni diverse).
    pub fn remember(&mut self, img: &Img, card: Card) {
        let t = thumb(img);
        let same: Vec<usize> = (0..self.templates.len()).filter(|&i| self.templates[i].card == card).collect();
        if same.iter().any(|&i| ncc(&t, &self.templates[i].px, (0, TH)) > 0.97) {
            return;
        }
        if same.len() >= 3 {
            self.templates.remove(same[0]);
        }
        self.templates.push(Template { card, px: t });
        self.save();
    }

    /// Dimentica le immagini uguali a questa (la carta era stata riconosciuta male).
    pub fn forget(&mut self, img: &Img) {
        let t = thumb(img);
        let before = self.templates.len();
        self.templates.retain(|tpl| ncc(&t, &tpl.px, (0, TH)) <= 0.97);
        if self.templates.len() != before {
            self.save();
        }
    }

    fn save(&self) {
        if let Some(p) = &self.path {
            if let Some(d) = p.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            let _ = std::fs::write(p, serde_json::to_string(self).unwrap_or_default());
        }
    }
}

#[cfg(test)]
impl Memory {
    pub fn in_memory() -> Memory {
        Memory::default()
    }
}
