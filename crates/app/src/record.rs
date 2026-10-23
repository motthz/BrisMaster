//! Registrazione delle partite giocate con l'assistente: un file JSON per partita,
//! riscritto a ogni carta inserita, così resta anche se la partita non viene finita.

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
}

impl Default for Recorder {
    fn default() -> Self {
        Recorder { events: Vec::new(), dir: dir(), file: None, error: None }
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

    /// Scrive il file, ma solo da quando è stata giocata almeno una carta.
    pub fn write(&mut self, pos: &Position) {
        if !self.events.iter().any(|e| matches!(e, Event::Giocata { .. })) {
            // Annullate tutte le giocate: niente file per una partita mai cominciata.
            if let Some((_, p)) = &self.file {
                let _ = std::fs::remove_file(p);
            }
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
