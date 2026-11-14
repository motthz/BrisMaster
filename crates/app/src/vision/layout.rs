//! La disposizione del tavolo del sito su cui si gioca, imparata dalle letture: dove stanno le
//! carte sullo schermo (la zona da leggere) e dove cadono la carta dell'avversario e la mia. Resta
//! solo su questo computer, in Documenti\BrisMaster\tavolo.json: non viene mai inviata da nessuna
//! parte.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Quello che si sa del tavolo, per una misura dello schermo letto.
#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq)]
pub struct Layout {
    /// Misura (larghezza, altezza) dell'immagine letta.
    pub size: (usize, usize),
    /// Zone delle carte delle ultime letture complete (frazioni: sinistra, alto, destra, basso).
    pub boxes: Vec<[f32; 4]>,
    /// Se si è già vista una carta nella casella dell'avversario e nella mia.
    pub seen_opp: bool,
    pub seen_mine: bool,
    /// Centro della casella dell'avversario e della mia (frazioni), e su quante prese.
    pub opp: Option<[f32; 2]>,
    pub mine: Option<[f32; 2]>,
    pub samples: u32,
}

#[derive(Serialize, Deserialize, Default)]
struct File {
    tables: Vec<Layout>,
}

fn path() -> PathBuf {
    crate::record::dir().parent().map(|p| p.to_path_buf()).unwrap_or_default().join("tavolo.json")
}

/// La disposizione salvata per uno schermo di questa misura (nelle prove non si legge niente).
pub fn load(size: (usize, usize)) -> Option<Layout> {
    if cfg!(test) {
        return None;
    }
    let f: File = std::fs::read_to_string(path()).ok().and_then(|s| serde_json::from_str(&s).ok())?;
    f.tables.into_iter().find(|t| t.size == size)
}

/// Salva la disposizione (al posto di quella della stessa misura; al più otto misure).
pub fn save(layout: &Layout) {
    if cfg!(test) {
        return;
    }
    let p = path();
    let mut f: File = std::fs::read_to_string(&p).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    f.tables.retain(|t| t.size != layout.size);
    f.tables.insert(0, layout.clone());
    f.tables.truncate(8);
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let _ = std::fs::write(p, serde_json::to_string(&f).unwrap_or_default());
}
