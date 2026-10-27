//! Le carte già riconosciute restano in memoria come piccole immagini: la volta dopo la stessa
//! carta si riconosce confrontando i pixel con quelli dello stesso sito, molto più sicuri del
//! confronto con le carte di riferimento (che hanno colori e disegno un po' diversi). Si ricordano
//! anche le carte viste in parte (la briscola sotto il mazzo), con la fascia che si vedeva. Si
//! salvano da sole, in Documenti\BrisMaster\carte_viste.json, a ogni carta inserita nella partita.

use super::{Img, CARD_H, CARD_W, WHOLE};
use briscola_core::cards::Card;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

const TW: usize = 25;
const TH: usize = 40;

/// Versione del modo di raddrizzare le carte: le immagini ricordate con un modo diverso non
/// sarebbero confrontabili e si ricomincia da capo.
const VERSION: u32 = 2;

fn whole() -> [f32; 2] {
    WHOLE
}

#[derive(Serialize, Deserialize, Clone)]
struct Template {
    card: Card,
    /// TW x TH x 3 valori RGB, senza il bordo della carta.
    px: Vec<u8>,
    /// Fascia della carta che si vedeva (frazioni dell'altezza dalla testa).
    #[serde(default = "whole")]
    vis: [f32; 2],
}

#[derive(Serialize, Deserialize, Default)]
pub struct Memory {
    #[serde(default)]
    version: u32,
    templates: Vec<Template>,
    #[serde(skip)]
    path: Option<PathBuf>,
}

/// Spostamenti (in pixel della carta raddrizzata) provati nel confronto: le carte viste in parte
/// non sono mai raddrizzate al pixel.
const SHIFTS: [(f32, f32); 9] =
    [(0.0, 0.0), (-3.0, 0.0), (3.0, 0.0), (0.0, -3.0), (0.0, 3.0), (-3.0, -3.0), (3.0, -3.0), (-3.0, 3.0), (3.0, 3.0)];

/// Miniatura della carta raddrizzata, senza il bordo.
fn thumb(img: &Img) -> Vec<u8> {
    thumb_at(img, (0.0, 0.0))
}

/// Miniatura della carta raddrizzata spostata di `off` pixel.
fn thumb_at(img: &Img, off: (f32, f32)) -> Vec<u8> {
    let m = CARD_W as f32 / 12.0;
    let (w, h) = (CARD_W as f32 - 2.0 * m, CARD_H as f32 - 2.0 * m);
    let mut out = Vec::with_capacity(TW * TH * 3);
    for j in 0..TH {
        for i in 0..TW {
            // Media di un blocchetto di pixel.
            let (x0, y0) = (m + off.0 + i as f32 * w / TW as f32, m + off.1 + j as f32 * h / TH as f32);
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

/// Righe della miniatura dentro le fasce visibili in tutte e due le carte.
fn common(a: [f32; 2], b: [f32; 2]) -> (usize, usize) {
    rows([a[0].max(b[0]), a[1].min(b[1])])
}

/// Righe della miniatura dentro la fascia visibile della carta (frazioni dell'altezza).
fn rows(vis: [f32; 2]) -> (usize, usize) {
    let r0 = ((vis[0] * TH as f32).ceil() as usize).min(TH);
    let r1 = ((vis[1] * TH as f32).floor() as usize).clamp(r0, TH);
    (r0, r1)
}

/// Correlazione normalizzata fra due miniature, sulle righe `rows`.
fn ncc(a: &[u8], b: &[u8], (r0, r1): (usize, usize)) -> f32 {
    // Meno di un quarto di carta non basta per dire che è la stessa.
    if r1 < r0 + TH / 4 {
        return 0.0;
    }
    let (a, b) = (&a[r0 * TW * 3..r1 * TW * 3], &b[r0 * TW * 3..r1 * TW * 3]);
    let n = a.len();
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

/// Somiglianza oltre la quale la carta è considerata la stessa (fra carte diverse dello stesso
/// mazzo si arriva al massimo a 0.8: il sei e il sette di denari).
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
        if m.version != VERSION {
            // Immagini di una versione precedente: si mettono da parte e si ricomincia.
            if !m.templates.is_empty() {
                let _ = std::fs::rename(&path, path.with_extension("vecchio.json"));
            }
            m = Memory { version: VERSION, ..Default::default() };
        }
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
    /// sulla fascia visibile `vis` (e su quella che si vedeva nell'immagine ricordata).
    pub fn similarity(&self, img: &Img, vis: [f32; 2]) -> [Option<f32>; 40] {
        let mut out = [None; 40];
        if self.templates.is_empty() {
            return out;
        }
        let thumbs: Vec<Vec<u8>> = SHIFTS.iter().map(|&o| thumb_at(img, o)).collect();
        for tpl in &self.templates {
            let rows = common(vis, tpl.vis);
            let s = thumbs.iter().map(|t| ncc(t, &tpl.px, rows)).fold(f32::MIN, f32::max);
            let o = &mut out[tpl.card as usize];
            if o.is_none_or(|v| s > v) {
                *o = Some(s);
            }
        }
        out
    }

    /// Ricorda una carta, vista nella fascia `vis`. Tiene al massimo 4 immagini per carta (luci,
    /// rotazioni e parti viste diverse).
    pub fn remember(&mut self, img: &Img, vis: [f32; 2], card: Card) {
        let t = thumb(img);
        let same: Vec<usize> = (0..self.templates.len()).filter(|&i| self.templates[i].card == card).collect();
        let covered = |i: usize| {
            let tpl = &self.templates[i];
            tpl.vis[0] <= vis[0] + 0.02 && tpl.vis[1] >= vis[1] - 0.02 && ncc(&t, &tpl.px, rows(vis)) > 0.97
        };
        if same.iter().any(|&i| covered(i)) {
            return;
        }
        if same.len() >= 4 {
            self.templates.remove(same[0]);
        }
        self.templates.push(Template { card, px: t, vis });
        self.save();
    }

    /// Dimentica le immagini uguali a questa (la carta era stata riconosciuta male).
    pub fn forget(&mut self, img: &Img) {
        let t = thumb(img);
        let before = self.templates.len();
        self.templates.retain(|tpl| ncc(&t, &tpl.px, rows(tpl.vis)) <= 0.97);
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
        Memory { version: VERSION, ..Default::default() }
    }
}
