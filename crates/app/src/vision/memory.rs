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
    pending: Vec<Template>,
    #[serde(skip)]
    path: Option<PathBuf>,
    #[serde(skip)]
    save_error: Option<String>,
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

// Il confronto delle strisce coperte usa la stessa griglia della miniatura.
fn visible_columns(cols: [f32; 2]) -> (usize, usize) {
    let margin = CARD_W as f32 / 12.0;
    let width = CARD_W as f32 - margin * 2.0;
    let x = |v: f32| ((v * CARD_W as f32 - margin) * TW as f32 / width).clamp(0.0, TW as f32);
    (x(cols[0]).ceil() as usize, x(cols[1]).floor() as usize)
}

fn ncc_rect(a: &[u8], b: &[u8], (r0, r1): (usize, usize), cols: (usize, usize)) -> f32 {
    let (left, right) = cols;
    if right <= left || (right - left) * (r1 - r0) < TW * TH / 3 {
        return 0.0;
    }
    if cols == (0, TW) {
        return ncc(a, b, (r0, r1));
    }
    let (mut sx, mut sy, mut xx, mut yy, mut xy) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for row in r0..r1 {
        let begin = (row * TW + left) * 3;
        let end = (row * TW + right) * 3;
        for (&x, &y) in a[begin..end].iter().zip(&b[begin..end]) {
            let (x, y) = (f64::from(x), f64::from(y));
            sx += x;
            sy += y;
            xx += x * x;
            yy += y * y;
            xy += x * y;
        }
    }
    let n = ((right - left) * (r1 - r0) * 3) as f64;
    let variance = (xx - sx * sx / n) * (yy - sy * sy / n);
    if variance <= 0.0 { 0.0 } else { ((xy - sx * sy / n) / variance.sqrt()) as f32 }
}

/// Correlazione normalizzata fra due miniature, sulle righe `rows`.
fn ncc(a: &[u8], b: &[u8], (r0, r1): (usize, usize)) -> f32 {
    // Meno di un quarto di carta non basta per dire che è la stessa.
    if r1 < r0 + TH / 4 {
        return 0.0;
    }
    let (a, b) = (&a[r0 * TW * 3..r1 * TW * 3], &b[r0 * TW * 3..r1 * TW * 3]);
    correlation(a, b)
}

fn correlation(a: &[u8], b: &[u8]) -> f32 {
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

/// La memoria di base, compresa nell'app: carte di mazzi forniti dall'utente, raddrizzate come le
/// vede l'app (vedi `crea_memoria_base`). Vale insieme alle carte ricordate sul computer, ma non
/// si cambia, non si dimentica e non si salva. File `assets/memoria_base.bin`: per ogni immagine
/// un byte con la carta, la fascia vista (due f32 little endian) e la miniatura (TW x TH x 3 byte).
fn base() -> &'static [Template] {
    static BASE: OnceLock<Vec<Template>> = OnceLock::new();
    BASE.get_or_init(|| {
        let bytes: &[u8] = include_bytes!("../../assets/memoria_base.bin");
        let size = 1 + 8 + TW * TH * 3;
        bytes
            .chunks_exact(size)
            .filter(|r| r[0] < 40)
            .map(|r| {
                let f = |i: usize| f32::from_le_bytes([r[i], r[i + 1], r[i + 2], r[i + 3]]);
                Template { card: r[0], vis: [f(1), f(5)], px: r[9..].to_vec() }
            })
            .collect()
    })
}

/// Le immagini con cui si confronta: quelle di base (se non spente nelle prove) e quelle ricordate.
fn all<'a>(own: &'a [Template]) -> impl Iterator<Item = &'a Template> {
    let base: &'static [Template] = if cfg!(test) && std::env::var_os("SENZA_MEMORIA_BASE").is_some() { &[] } else { base() };
    base.iter().chain(own.iter())
}

pub(super) struct Matches {
    shipped: [Option<f32>; 40],
    local: [Option<f32>; 40],
}

impl Default for Matches {
    fn default() -> Self {
        Self { shipped: [None; 40], local: [None; 40] }
    }
}

impl Matches {
    pub(super) fn combined(&self) -> [Option<f32>; 40] {
        std::array::from_fn(|i| match (self.shipped[i], self.local[i]) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        })
    }

    pub(super) fn conflict(&self) -> bool {
        let winner = |scores: &[Option<f32>; 40]| {
            let (card, best) = scores.iter().copied().enumerate()
                .filter_map(|(card, score)| Some((card, score?)))
                .max_by(|a, b| a.1.total_cmp(&b.1))?;
            let runner = scores.iter().enumerate().filter_map(|(i, &s)| (i != card).then_some(s).flatten())
                .fold(f32::NEG_INFINITY, f32::max);
            (best >= SURE && best - runner >= SURE_GAP).then_some(card)
        };
        matches!((winner(&self.shipped), winner(&self.local)), (Some(a), Some(b)) if a != b)
    }
}

/// Somiglianza oltre la quale la carta è considerata la stessa (fra carte diverse dello stesso
/// mazzo si arriva al massimo a 0.8: il sei e il sette di denari).
pub const SAME: f32 = 0.9;
/// Somiglianza oltre la quale l'immagine ricordata è proprio la stessa carta dello stesso sito.
pub const SURE: f32 = 0.95;
/// Quanto deve staccare le altre carte ricordate per essere decisiva.
pub const SURE_GAP: f32 = 0.04;

/// Somiglianza fra due carte raddrizzate, sulla fascia visibile `vis`.
/// Poco contrasto: fra i punti più chiari e quelli più scuri della carta c'è poca differenza.
pub fn faint(img: &Img) -> bool {
    let mut g: Vec<u8> = img.px.iter().step_by(3).map(|p| ((p[0] as u32 * 3 + p[1] as u32 * 6 + p[2] as u32) / 10) as u8).collect();
    if g.len() < 20 {
        return false;
    }
    g.sort_unstable();
    let (lo, hi) = (g[g.len() * 5 / 100], g[g.len() * 95 / 100]);
    hi.saturating_sub(lo) < 110
}

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

    /// Copia per una ricostruzione provvisoria: le ipotesi non devono insegnare
    /// carte alla memoria della partita reale, né scrivere sul disco.
    pub fn isolated(&self) -> Memory {
        Memory { version: self.version, templates: self.templates.clone(), pending: Vec::new(), path: None, save_error: None }
    }

    pub fn len(&self) -> usize {
        let mut cards: Vec<Card> = self.templates.iter().map(|t| t.card).collect();
        cards.sort();
        cards.dedup();
        cards.len()
    }

    /// Per ogni carta la migliore corrispondenza fra gli esempi disponibili.
    pub fn similarity(&self, img: &Img, vis: [f32; 2]) -> [Option<f32>; 40] {
        self.visible_matches(img, vis, WHOLE).combined()
    }

    pub(super) fn visible_matches(&self, img: &Img, vis: [f32; 2], cols: [f32; 2]) -> Matches {
        let mut matches = Matches::default();
        if (cfg!(test) && std::env::var_os("SENZA_MEMORIA").is_some()) || all(&self.templates).next().is_none() {
            return matches;
        }
        let columns = visible_columns(cols);
        let thumbs: Vec<Vec<u8>> = SHIFTS.iter().map(|&o| thumb_at(img, o)).collect();
        let shipped = if cfg!(test) && std::env::var_os("SENZA_MEMORIA_BASE").is_some() { &[] } else { base() };
        for (templates, scores) in [(shipped, &mut matches.shipped), (&self.templates[..], &mut matches.local)] {
            for tpl in templates {
                let rows = common(vis, tpl.vis);
                let score = thumbs.iter().map(|image| {
                    if cols == WHOLE { ncc(image, &tpl.px, rows) }
                    else { ncc_rect(image, &tpl.px, rows, columns) }
                }).fold(f32::NEG_INFINITY, f32::max);
                let slot = &mut scores[tpl.card as usize];
                if slot.is_none_or(|old| score > old) {
                    *slot = Some(score);
                }
            }
        }
        matches
    }

    /// Una lettura automatica entra in memoria solo dopo una seconda
    /// osservazione compatibile dello stesso volto in un altro evento.
    pub fn observe(&mut self, img: &Img, vis: [f32; 2], card: Card) {
        if vis != WHOLE || faint(img) { return; }
        let pixels = thumb(img);
        let similar = |tpl: &Template| ncc(&pixels, &tpl.px, rows(WHOLE));
        if self.pending.iter().any(|tpl| tpl.card != card && similar(tpl) > 0.95) {
            self.pending.retain(|tpl| similar(tpl) <= 0.95);
            return;
        }
        if let Some(index) = self.pending.iter().position(|tpl| {
            tpl.card == card && (0.90..0.995).contains(&similar(tpl))
        }) {
            self.pending.remove(index);
            self.remember(img, vis, card);
        } else if !self.pending.iter().any(|tpl| tpl.card == card && similar(tpl) >= 0.995) {
            if self.pending.len() >= 80 { self.pending.remove(0); }
            self.pending.push(Template { card, px: pixels, vis });
        }
    }

    /// Applica il mazzo confermato con un solo salvataggio atomico.
    pub(super) fn import_deck(&mut self, deck: &[Img; 40]) -> Result<(), String> {
        let mut staged = self.isolated();
        for (card, img) in deck.iter().enumerate() {
            staged.remember(img, WHOLE, card as Card);
        }
        if let Some(path) = &self.path {
            save_atomic(path, &staged).map_err(|err| err.to_string())?;
        }
        self.templates = staged.templates;
        self.pending.clear();
        self.save_error = None;
        Ok(())
    }

    /// Ricorda una carta, vista nella fascia `vis`. Tiene al massimo 4 immagini per carta (luci,
    /// rotazioni e parti viste diverse).
    pub fn remember(&mut self, img: &Img, vis: [f32; 2], card: Card) {
        // Un'immagine sbiadita (una carta che il sito fa svanire, mezza trasparente sul tavolo)
        // non si ricorda: senza bianco né inchiostro scuro somiglia a troppe carte.
        if faint(img) {
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
        self.pending.retain(|tpl| ncc(&t, &tpl.px, rows(tpl.vis)) <= 0.97);
        let before = self.templates.len();
        self.templates.retain(|tpl| ncc(&t, &tpl.px, rows(tpl.vis)) <= 0.97);
        if self.templates.len() != before {
            self.save();
        }
    }

    pub(crate) fn samples(&self) -> Vec<(Card, Img, [f32; 2])> {
        self.templates.iter().filter_map(|sample| {
            if sample.card >= 40 || sample.px.len() != TW * TH * 3 {
                return None;
            }
            let img = Img {
                w: TW,
                h: TH,
                px: sample.px.chunks_exact(3).map(|p| [p[0], p[1], p[2]]).collect(),
            };
            Some((sample.card, img, sample.vis))
        }).collect()
    }

    pub(crate) fn correct_sample(&mut self, img: &Img, vis: [f32; 2], card: Card) -> Result<(), String> {
        if card >= 40 || faint(img) {
            return Err("L’immagine non è abbastanza nitida per la memoria.".into());
        }
        let pixels = thumb(img);
        let mut staged = self.isolated();
        staged.templates.retain(|sample| ncc(&pixels, &sample.px, rows(sample.vis)) <= 0.97);
        staged.remember(img, vis, card);
        self.commit_correction(staged)
    }

    pub(crate) fn relabel_sample(&mut self, old: Card, image: &Img, vis: [f32; 2], card: Card) -> Result<(), String> {
        if card >= 40 {
            return Err("Carta non valida.".into());
        }
        let mut staged = self.isolated();
        let pixels: Vec<_> = image.px.iter().flatten().copied().collect();
        let Some(index) = staged.templates.iter().position(|sample| sample.card == old && sample.px == pixels && sample.vis == vis) else {
            return Err("Il campione è cambiato. Aggiorna l’elenco.".into());
        };
        let mut sample = staged.templates.remove(index);
        sample.card = card;
        if staged.templates.iter().filter(|sample| sample.card == card).count() >= 4 {
            if let Some(index) = staged.templates.iter().position(|sample| sample.card == card) {
                staged.templates.remove(index);
            }
        }
        staged.templates.push(sample);
        self.commit_correction(staged)
    }

    /// Toglie dall'archivio un'immagine ricordata (per esempio un pezzo di tavolo o una scritta
    /// salvati come carta). Le immagini di base comprese nell'app non sono nell'archivio e restano.
    pub(crate) fn delete_sample(&mut self, card: Card, image: &Img, vis: [f32; 2]) -> Result<(), String> {
        let mut staged = self.isolated();
        let pixels: Vec<_> = image.px.iter().flatten().copied().collect();
        let Some(index) = staged.templates.iter().position(|sample| sample.card == card && sample.px == pixels && sample.vis == vis) else {
            return Err("Il campione è cambiato. Aggiorna l’elenco.".into());
        };
        staged.templates.remove(index);
        self.commit_correction(staged)
    }

    fn commit_correction(&mut self, staged: Self) -> Result<(), String> {
        if let Some(path) = &self.path {
            save_atomic(path, &staged).map_err(|err| err.to_string())?;
        }
        self.templates = staged.templates;
        self.pending.clear();
        self.save_error = None;
        Ok(())
    }

    pub(crate) fn export(&self, path: &std::path::Path) -> std::io::Result<()> {
        save_atomic(path, self)
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
    fn a_sample_deleted_from_the_archive_is_gone_and_saved() {
        let dir = std::env::temp_dir().join(format!("brismaster-delete-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("carte_viste.json");
        let mut memory = Memory::in_memory();
        memory.path = Some(path.clone());
        let card = crate::vision::reference::card_image(0, 12);
        memory.remember(&card, WHOLE, 12);
        memory.remember(&crate::vision::reference::card_image(0, 25), WHOLE, 25);
        let samples = memory.samples();
        assert_eq!(samples.len(), 2);
        let (c, img, vis) = samples.iter().find(|s| s.0 == 12).unwrap().clone();
        memory.delete_sample(c, &img, vis).unwrap();
        assert!(memory.samples().iter().all(|s| s.0 != 12));
        assert_eq!(memory.samples().len(), 1);
        // Già tolto: l'elenco va aggiornato.
        assert!(memory.delete_sample(c, &img, vis).is_err());
        let saved: Memory = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved.templates.len(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn does_not_learn_the_same_automatic_crop_twice() {
        let (card, image) = crate::vision::reference::reference_cards().remove(0);
        let mut memory = Memory::in_memory();
        memory.observe(&image, WHOLE, card);
        memory.observe(&image, WHOLE, card);
        assert_eq!(memory.len(), 0);
        assert_eq!(memory.pending.len(), 1);
        memory.remember(&image, WHOLE, card);
        assert_eq!(memory.len(), 1);
    }

    #[test]
    fn ignores_hidden_columns_and_rejects_a_tiny_patch() {
        let reference: Vec<u8> = (0..TW * TH * 3).map(|i| (i * 13 % 251) as u8).collect();
        let mut covered = reference.clone();
        for row in covered.chunks_exact_mut(TW * 3) {
            row[..8 * 3].fill(0);
        }
        let score = ncc_rect(&covered, &reference, (0, TH), (8, TW));
        assert!((score - 1.0).abs() < 0.001);
        assert_eq!(ncc_rect(&covered, &reference, (0, 4), (8, TW)), 0.0);
    }

    #[test]
    fn separates_conflicting_memory_sources() {
        let mut matches = Matches::default();
        matches.shipped[4] = Some(0.98);
        matches.local[5] = Some(0.96);
        assert!(matches.conflict());
        matches.local[4] = Some(0.99);
        assert!(!matches.conflict());
    }

    /// Scrive la memoria di base da una cartella di carte raddrizzate dall'app (100x160, file
    /// `NN_qualcosa.png` con NN il numero della carta):
    /// `BASE=cartella cargo test --release -p brismaster crea_memoria_base -- --ignored`
    #[test]
    #[ignore]
    fn crea_memoria_base() {
        let dir = std::path::PathBuf::from(std::env::var("BASE").expect("BASE=cartella"));
        let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "png")).collect();
        files.sort();
        let mut out = Vec::new();
        for f in &files {
            let card: u8 = f.file_name().unwrap().to_string_lossy()[..2].parse().unwrap();
            let img = crate::vision::capture::load_png(f).unwrap();
            assert!(img.w == CARD_W && img.h == CARD_H && card < 40, "{f:?}");
            out.push(card);
            out.extend(0.0f32.to_le_bytes());
            out.extend(1.0f32.to_le_bytes());
            out.extend(thumb(&img));
        }
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/memoria_base.bin");
        std::fs::write(&path, &out).unwrap();
        println!("{} immagini in {path:?}", files.len());
    }

    #[test]
    fn base_memory_recognizes_its_own_cards() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/memoria_base");
        let mut n = 0;
        for f in std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.path()) {
            let card: Card = f.file_name().unwrap().to_string_lossy()[..2].parse().unwrap();
            let img = crate::vision::capture::load_png(&f).unwrap();
            let sims = Memory::in_memory().similarity(&img, WHOLE);
            let best = (0..40).max_by(|&a, &b| sims[a].unwrap_or(-1.0).total_cmp(&sims[b].unwrap_or(-1.0))).unwrap();
            assert_eq!(best as Card, card, "{f:?}");
            n += 1;
        }
        assert!(n >= 39 && base().len() == n);
    }

    /// Un'immagine di base non deve far leggere un'altra carta su un mazzo di riferimento: per ogni
    /// carta di ogni stile, se la memoria di base ha un'immagine quasi uguale (SAME), è della carta
    /// giusta.
    #[test]
    fn base_memory_never_relabels_reference_cards() {
        let m = Memory::in_memory();
        for style in 0..3 {
            for c in 0..40u8 {
                let img = crate::vision::reference::card_image(style, c);
                let sims = m.similarity(&img, WHOLE);
                for (other, s) in sims.iter().enumerate() {
                    if other as u8 != c {
                        assert!(s.unwrap_or(-1.0) < SAME, "stile {style}, carta {c}: somiglia a {other} ({s:?})");
                    }
                }
            }
        }
    }

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
