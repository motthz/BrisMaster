//! Registrazione delle partite giocate con l'assistente: un file JSON per partita,
//! riscritto a ogni carta inserita, così resta anche se la partita non viene finita.
//! Si salva solo la partita per cui l'utente accende la registrazione (spenta di default).

use crate::assistant::{Position, ME};
use briscola_core::ai::MoveEval;
use briscola_core::cards::{self, Card};
use serde::Serialize;
use std::path::PathBuf;

/// Cartella delle partite: Documenti\BrisMaster\Partite (o la home se Documenti non c'è).
pub fn dir() -> PathBuf {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from);
    let base = match home {
        Some(h) if h.join("Documents").is_dir() => h.join("Documents"),
        Some(h) => h,
        None => std::env::temp_dir(),
    };
    base.join("BrisMaster").join("Partite")
}

/// Apre la cartella delle partite in Esplora risorse.
pub fn open_dir() {
    let d = dir();
    let _ = std::fs::create_dir_all(&d);
    #[cfg(windows)]
    let _ = std::process::Command::new("explorer").arg(&d).spawn();
    #[cfg(not(windows))]
    let _ = std::process::Command::new("xdg-open").arg(&d).spawn();
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct CardRec {
    pub id: Card,
    pub nome: String,
}

impl CardRec {
    pub fn of(c: Card) -> CardRec {
        CardRec { id: c, nome: cards::name(c) }
    }
}

#[derive(Serialize, Clone, Debug)]
pub struct EvalRec {
    pub carta: CardRec,
    pub vittoria: f64,
    pub punti_attesi: f64,
}

/// Consiglio del motore al momento della giocata.
#[derive(Serialize, Clone, Debug)]
pub struct Advice {
    pub mondi: u32,
    /// Dalla migliore alla peggiore.
    pub valutazioni: Vec<EvalRec>,
}

impl Advice {
    pub fn new(evals: &[MoveEval], samples: u32) -> Advice {
        Advice {
            mondi: samples,
            valutazioni: evals
                .iter()
                .map(|e| EvalRec { carta: CardRec::of(e.card), vittoria: e.win_prob, punti_attesi: e.exp_points })
                .collect(),
        }
    }
}

/// Stato visto dal giocatore prima di una sua giocata.
#[derive(Serialize, Clone, Debug)]
pub struct Seen {
    pub mano: Vec<CardRec>,
    pub tavolo: Option<CardRec>,
    pub punti: [u8; 2],
    pub mazzo: u8,
    pub carte_avversario: u8,
}

#[derive(Serialize, Clone, Debug)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum Event {
    Briscola {
        carta: CardRec,
    },
    PrimoDiMano {
        chi: &'static str,
    },
    Pescata {
        carta: CardRec,
    },
    Giocata {
        presa: u8,
        chi: &'static str,
        carta: CardRec,
        /// Solo per le mie giocate.
        #[serde(skip_serializing_if = "Option::is_none")]
        prima: Option<Seen>,
        /// Assente se ho giocato prima che il motore finisse o se avevo una sola carta.
        #[serde(skip_serializing_if = "Option::is_none")]
        consiglio: Option<Advice>,
        #[serde(skip_serializing_if = "Option::is_none")]
        seguito_consiglio: Option<bool>,
    },
}

pub fn who(p: u8) -> &'static str {
    if p == ME {
        "io"
    } else {
        "avversario"
    }
}

impl Event {
    pub fn play(pos: &Position, c: Card, advice: Option<Advice>) -> Event {
        let p = pos.to_move();
        let prima = (p == ME).then(|| Seen {
            mano: pos.hand.iter().map(CardRec::of).collect(),
            tavolo: pos.table.map(CardRec::of),
            punti: pos.points,
            mazzo: pos.deck_len,
            carte_avversario: pos.opp_hand_len,
        });
        let seguito_consiglio = advice.as_ref().map(|a| a.valutazioni[0].carta.id == c);
        Event::Giocata {
            presa: pos.tricks + 1,
            chi: who(p),
            carta: CardRec::of(c),
            prima,
            consiglio: advice,
            seguito_consiglio,
        }
    }
}

#[derive(Serialize, Debug)]
pub struct Outcome {
    pub punti_miei: u8,
    pub punti_avversario: u8,
    pub esito: &'static str,
}

#[derive(Serialize)]
struct File<'a> {
    versione_app: &'a str,
    inizio: &'a str,
    finita: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    risultato: Option<Outcome>,
    eventi: &'a [Event],
}

/// Registro della partita in corso.
pub struct Recorder {
    pub events: Vec<Event>,
    pub dir: PathBuf,
    /// Ora di inizio e file, fissati alla prima carta giocata.
    file: Option<(String, PathBuf)>,
    pub error: Option<String>,
    /// Esito dell'ultima esportazione (Prepara file da inviare).
    pub export_msg: Option<String>,
    /// Se questa partita va salvata: la decide l'utente, ogni partita parte spenta.
    pub enabled: bool,
}

impl Default for Recorder {
    fn default() -> Self {
        Recorder { events: Vec::new(), dir: dir(), file: None, error: None, export_msg: None, enabled: false }
    }
}

impl Recorder {
    /// Il file della partita in corso, o la cartella se non è ancora cominciata.
    pub fn path(&self) -> &std::path::Path {
        self.file.as_ref().map_or(&self.dir, |(_, p)| p)
    }

    pub fn to_json(&self, pos: &Position) -> String {
        let finita = pos.tricks >= 20;
        let risultato = finita.then(|| {
            let mine = pos.points[ME as usize];
            Outcome {
                punti_miei: mine,
                punti_avversario: pos.points[1 - ME as usize],
                esito: match mine {
                    61.. => "vinta",
                    60 => "pareggio",
                    _ => "persa",
                },
            }
        });
        let inizio = self.file.as_ref().map_or("", |(s, _)| s);
        let f = File { versione_app: crate::update::CURRENT, inizio, finita, risultato, eventi: &self.events };
        serde_json::to_string_pretty(&f).unwrap()
    }

    /// Accende o spegne la registrazione di questa partita: accesa la salva tutta, anche
    /// le carte inserite prima; spenta cancella il suo file.
    pub fn set_enabled(&mut self, on: bool, pos: &Position) {
        self.enabled = on;
        self.write(pos);
    }

    /// Scrive il file, ma solo se la registrazione è accesa e da quando è stata giocata
    /// almeno una carta.
    pub fn write(&mut self, pos: &Position) {
        if !self.enabled || !self.events.iter().any(|e| matches!(e, Event::Giocata { .. })) {
            // Registrazione spenta, o annullate tutte le giocate: niente file.
            if let Some((_, p)) = self.file.take() {
                let _ = std::fs::remove_file(p);
            }
            self.error = None;
            return;
        }
        if self.file.is_none() {
            let now = chrono::Local::now();
            let mut path = self.dir.join(format!("partita_{}.json", now.format("%Y-%m-%d_%H-%M-%S")));
            // Due partite nello stesso secondo (Nuova partita subito dopo un annullamento).
            for n in 2.. {
                if !path.exists() {
                    break;
                }
                path = self.dir.join(format!("partita_{}_{n}.json", now.format("%Y-%m-%d_%H-%M-%S")));
            }
            self.file = Some((now.format("%Y-%m-%d %H:%M:%S").to_string(), path));
        }
        let path = &self.file.as_ref().unwrap().1;
        let res = std::fs::create_dir_all(&self.dir).and_then(|_| std::fs::write(path, self.to_json(pos)));
        self.error = res.err().map(|e| format!("Impossibile salvare la partita: {e}"));
    }
}

/// Nome del file che raccoglie tutte le partite salvate, da inviare per allenare il motore.
pub const EXPORT_NAME: &str = "partite-brismaster.json";

/// Raccoglie tutte le partite salvate in `dir` in un solo file (accanto alla cartella delle
/// partite, in `dir/..`), compatto, da allegare in un messaggio. Restituisce il file e
/// quante partite contiene. I file illeggibili vengono saltati.
pub fn export(dir: &std::path::Path) -> std::io::Result<(PathBuf, usize)> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension().is_some_and(|x| x == "json")
                && p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("partita_"))
        })
        .collect();
    files.sort();
    let mut partite = Vec::new();
    for f in &files {
        let Ok(text) = std::fs::read_to_string(f) else { continue };
        let Ok(mut v) = serde_json::from_str::<serde_json::Value>(&text) else { continue };
        if let Some(o) = v.as_object_mut() {
            let name = f.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            o.insert("file".into(), name.into());
        }
        partite.push(v);
    }
    let out = dir.parent().unwrap_or(dir).join(EXPORT_NAME);
    let n = partite.len();
    let bundle = serde_json::json!({
        "versione_app": crate::update::CURRENT,
        "esportato": chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        "partite": partite,
    });
    std::fs::write(&out, serde_json::to_string(&bundle).unwrap())?;
    Ok((out, n))
}

/// Mostra il file in Esplora risorse, già selezionato, pronto da trascinare nel messaggio.
pub fn show_file(path: &std::path::Path) {
    #[cfg(windows)]
    let _ = std::process::Command::new("explorer").arg(format!("/select,{}", path.display())).spawn();
    #[cfg(not(windows))]
    let _ = std::process::Command::new("xdg-open").arg(path.parent().unwrap_or(path)).spawn();
}

/// Segno che le partite salvate quando la registrazione era sempre accesa sono già state cancellate.
const CLEANED_MARK: &str = ".registrazione-a-scelta";

/// Una volta sola: cancella le partite salvate quando la registrazione era automatica (fino alla
/// 0.5.x) e il vecchio file da inviare, perché l'utente vuole tenere solo le partite che sceglie.
/// Restituisce quante partite ha cancellato.
pub fn remove_automatic_once(dir: &std::path::Path) -> usize {
    let base = dir.parent().unwrap_or(dir);
    let mark = base.join(CLEANED_MARK);
    if mark.exists() {
        return 0;
    }
    let mut n = 0;
    if let Ok(rd) = std::fs::read_dir(dir) {
        for p in rd.filter_map(|e| e.ok().map(|e| e.path())) {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            if name.starts_with("partita_") && name.ends_with(".json") && std::fs::remove_file(&p).is_ok() {
                n += 1;
            }
        }
    }
    let _ = std::fs::remove_file(base.join(EXPORT_NAME));
    let _ = std::fs::create_dir_all(base);
    let _ = std::fs::write(&mark, "Partite salvate in automatico cancellate; ora si salva solo quando la registrazione è accesa.\n");
    n
}
