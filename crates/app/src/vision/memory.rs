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
// La normalizzazione del bianco è cambiata; le vecchie immagini possono anche
// contenere etichette apprese prima delle correzioni del recupero.
const VERSION: u32 = 4;

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
    #[serde(skip)]
    save_error: Option<String>,
}

fn valid_view(img: &Img, vis: [f32; 2]) -> bool {
    img.w == CARD_W && img.h == CARD_H && img.px.len() == CARD_W * CARD_H
        && vis.iter().all(|x| x.is_finite())
        && 0.0 <= vis[0] && vis[0] < vis[1] && vis[1] <= 1.0
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
    // Integer moments need one pass and avoid cancellation for near-constant images.
    let (mut sa, mut sb, mut aa, mut bb, mut ab) = (0i64, 0i64, 0i64, 0i64, 0i64);
    for (&a, &b) in a.iter().zip(b) {
        let (a, b) = (i64::from(a), i64::from(b));
        sa += a;
        sb += b;
        aa += a * a;
        bb += b * b;
        ab += a * b;
    }
    let n = a.len() as i64;
    let (da, db) = (n * aa - sa * sa, n * bb - sb * sb);
    if da <= 0 || db <= 0 {
        0.0
    } else {
        ((n * ab - sa * sb) as f64 / ((da as f64) * (db as f64)).sqrt()) as f32
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
        // Memory is editable/importable JSON: validate before any pixel slicing/indexing.
        let before = m.templates.len();
        m.templates.retain(|t| {
            t.card < 40
                && t.px.len() == TW * TH * 3
                && t.vis.iter().all(|x| x.is_finite())
                && 0.0 <= t.vis[0]
                && t.vis[0] < t.vis[1]
                && t.vis[1] <= 1.0
        });
        let rejected = before - m.templates.len();
        if m.version != VERSION {
            // Immagini di una versione precedente: si mettono da parte e si ricomincia.
            if !m.templates.is_empty() {
                let _ = std::fs::rename(&path, path.with_extension("vecchio.json"));
            }
            m = Memory { version: VERSION, ..Default::default() };
        }
        if rejected > 0 {
            m.save_error = Some(format!("Ignorati {rejected} esempi di carte danneggiati nella memoria"));
        }
        m.path = Some(path);
        m
    }

    /// Copia per una ricostruzione provvisoria: le ipotesi non devono insegnare
    /// carte alla memoria della partita reale, né scrivere sul disco.
    pub fn isolated(&self) -> Memory {
        Memory { version: self.version, templates: self.templates.clone(), path: None, save_error: None }
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
        if self.templates.is_empty() || !valid_view(img, vis) {
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
        if card >= 40 || !valid_view(img, vis) {
            self.save_error = Some("Esempio di carta non valido: memoria invariata".into());
            return;
        }
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

    pub fn last_error(&self) -> Option<&str> {
        self.save_error.as_deref()
    }

    fn save(&mut self) {
        self.save_error = self.path.as_ref().and_then(|p| save_atomic(p, self).err()).map(|e| e.to_string());
    }
}

fn save_atomic(path: &std::path::Path, memory: &Memory) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension(format!("{}.tmp", std::process::id()));
    let result = (|| {
        let bytes = serde_json::to_vec(memory)?;
        let mut file = std::fs::File::create(&temp)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        replace(&temp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

#[cfg(not(windows))]
fn replace(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    std::fs::rename(from, to)
}

#[cfg(windows)]
fn replace(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH};
    let wide = |p: &std::path::Path| p.as_os_str().encode_wide().chain(Some(0)).collect::<Vec<_>>();
    if unsafe {
        MoveFileExW(wide(from).as_ptr(), wide(to).as_ptr(), MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH)
    } == 0
    {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(test)]
impl Memory {
    pub fn in_memory() -> Memory {
        Memory { version: VERSION, ..Default::default() }
    }
}

#[cfg(test)]
mod persistence_tests {
    use super::*;

    #[test]
    fn atomic_save_replaces_existing_json_and_reports_errors() {
        let dir = std::env::temp_dir().join(format!("brismaster-memory-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cards.json");
        std::fs::write(&path, b"old").unwrap();
        let mut memory = Memory::in_memory();
        memory.path = Some(path.clone());
        memory.save();
        assert!(memory.last_error().is_none());
        let loaded: Memory = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(loaded.version, VERSION);
        // A file cannot be used as the parent directory; preserve it on failure.
        memory.path = Some(path.join("cannot-write.json"));
        memory.save();
        assert!(memory.last_error().is_some());
        assert!(serde_json::from_slice::<Memory>(&std::fs::read(&path).unwrap()).is_ok());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
