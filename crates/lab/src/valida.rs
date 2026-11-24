//! Validatore dei file `human_decision_v1`: rigioca ogni partita dall'ordine del mazzo e
//! controlla che ogni riga dica il vero. Le partite incoerenti sono scartate con il motivo,
//! mai corrette in silenzio.

use crate::riga::{indici, Origine, Riga, SCHEMA};
use briscola_core::cards::*;
use briscola_core::game::Game;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Default, PartialEq)]
pub struct Rapporto {
    /// Righe di dati lette (senza intestazioni).
    pub righe: usize,
    pub righe_illeggibili: Vec<(usize, String)>,
    pub partite_valide: usize,
    /// Partite comparse più volte identiche: contate una volta sola.
    pub duplicate: usize,
    pub scartate: Vec<(String, String)>,
    /// Carte giocate dalle persone nelle partite valide.
    pub decisioni_umane: usize,
    /// Decisioni umane con una sola carta in mano (fine partita): da escludere dalle metriche.
    pub decisioni_obbligate: usize,
    pub giocatori: BTreeSet<String>,
    /// Partite valide vinte dalla persona, pareggiate, perse.
    pub vinte: usize,
    pub pari: usize,
    pub perse: usize,
    /// Partite valide in ordine di lettura, ognuna con le sue 40 righe.
    pub partite: Vec<Vec<Riga>>,
}

impl Rapporto {
    pub fn testo(&self) -> String {
        let mut s = format!(
            "Righe lette: {}\nRighe illeggibili: {}\nPartite valide: {} (duplicate ignorate: {})\nPartite scartate: {}\nGiocatori diversi: {}\nDecisioni umane: {} (di cui obbligate: {})\nEsito per le persone: {} vinte, {} pari, {} perse\n",
            self.righe,
            self.righe_illeggibili.len(),
            self.partite_valide,
            self.duplicate,
            self.scartate.len(),
            self.giocatori.len(),
            self.decisioni_umane,
            self.decisioni_obbligate,
            self.vinte,
            self.pari,
            self.perse
        );
        for (n, m) in self.righe_illeggibili.iter().take(20) {
            s += &format!("  riga {n}: {m}\n");
        }
        for (p, m) in &self.scartate {
            s += &format!("  scartata {p}: {m}\n");
        }
        s
    }
}

/// Valida un CSV (anche più file concatenati: ogni riga che comincia con `schema,` è
/// un'intestazione e vale per le righe che seguono).
pub fn valida(testo: &str) -> Rapporto {
    let mut r = Rapporto::default();
    let mut idx = None;
    let mut per_partita: BTreeMap<String, Vec<Riga>> = BTreeMap::new();
    let mut ordine: Vec<String> = Vec::new();
    for (n, linea) in testo.lines().enumerate() {
        let l = linea.trim_start_matches('\u{feff}');
        if l.trim().is_empty() {
            continue;
        }
        if l.split(',').any(|c| c.trim() == "schema") {
            match indici(l) {
                Ok(i) => idx = Some(i),
                Err(e) => r.righe_illeggibili.push((n + 1, e)),
            }
            continue;
        }
        r.righe += 1;
        let Some(i) = idx.as_ref() else {
            r.righe_illeggibili
                .push((n + 1, "riga prima dell'intestazione".into()));
            continue;
        };
        match Riga::from_csv(l, i) {
            Ok(riga) => {
                if !per_partita.contains_key(&riga.partita) {
                    ordine.push(riga.partita.clone());
                }
                per_partita
                    .entry(riga.partita.clone())
                    .or_default()
                    .push(riga);
            }
            Err(e) => r.righe_illeggibili.push((n + 1, e)),
        }
    }
    for id in ordine {
        let mut righe = per_partita.remove(&id).unwrap();
        // Una partita inviata due volte identica conta una volta.
        if righe.len() > 40 && righe.len().is_multiple_of(40) {
            let copie = righe.len() / 40;
            if (1..copie).all(|k| righe[k * 40..(k + 1) * 40] == righe[..40]) {
                righe.truncate(40);
                r.duplicate += copie - 1;
            }
        }
        match controlla(&mut righe) {
            Ok(esito) => {
                r.partite_valide += 1;
                let umane: Vec<&Riga> = righe
                    .iter()
                    .filter(|x| x.origine == Origine::Umano)
                    .collect();
                r.decisioni_umane += umane.len();
                r.decisioni_obbligate += umane.iter().filter(|x| x.legali.len() == 1).count();
                r.giocatori.insert(righe[0].giocatore.clone());
                match esito {
                    e if e > 60 => r.vinte += 1,
                    60 => r.pari += 1,
                    _ => r.perse += 1,
                }
                r.partite.push(righe);
            }
            Err(m) => r.scartate.push((id, m)),
        }
    }
    r
}

/// Controlla una partita; restituisce i punti finali della persona.
fn controlla(righe: &mut [Riga]) -> Result<u8, String> {
    righe.sort_by_key(|x| x.mossa);
    if righe.len() != 40 {
        return Err(format!(
            "{} carte giocate invece di 40 (partita incompleta o ripetuta)",
            righe.len()
        ));
    }
    for (k, x) in righe.iter().enumerate() {
        if x.mossa as usize != k {
            return Err(format!("mossa {k} mancante o ripetuta"));
        }
    }
    let r0 = &righe[0];
    if r0.schema != SCHEMA {
        return Err(format!("schema '{}' sconosciuto", r0.schema));
    }
    if r0.regole != crate::REGOLE {
        return Err(format!(
            "regole '{}' diverse da {}",
            r0.regole,
            crate::REGOLE
        ));
    }
    for x in righe.iter() {
        if x.giocatore != r0.giocatore
            || x.sessione != r0.sessione
            || x.regole != r0.regole
            || x.motore != r0.motore
        {
            return Err(format!(
                "mossa {}: giocatore, sessione, regole o motore cambiano a metà partita",
                x.mossa
            ));
        }
        if x.consigli_visibili {
            return Err(format!(
                "mossa {}: consigli visibili (partita assistita)",
                x.mossa
            ));
        }
        if x.qualita != "exact" {
            return Err(format!(
                "mossa {}: qualità '{}' non esatta",
                x.mossa, x.qualita
            ));
        }
        if x.mossa > 0 && x.mazzo.is_some() {
            return Err(format!("mossa {}: ordine del mazzo fuori posto", x.mossa));
        }
    }
    let mazzo = r0
        .mazzo
        .clone()
        .ok_or("manca l'ordine del mazzo sulla mossa 0")?;
    if mazzo.len() != 40 || mazzo.iter().copied().collect::<CardSet>().len() != 40 {
        return Err("l'ordine del mazzo non contiene le 40 carte una volta sola".into());
    }
    let umano = righe
        .iter()
        .find(|x| x.origine == Origine::Umano)
        .map(|x| x.posto)
        .ok_or("nessuna carta giocata da una persona")?;
    let mut g = Game::from_order(&mazzo, r0.apre & 1);
    if r0.apre > 1 {
        return Err("posto di apertura non valido".into());
    }
    let mut storia: Vec<Card> = Vec::new();
    for x in righe.iter() {
        let m = x.mossa;
        let p = g.to_move();
        let atteso = if p == umano {
            Origine::Umano
        } else {
            Origine::Massima
        };
        if x.posto != p || x.origine != atteso {
            return Err(format!(
                "mossa {m}: gioca il posto {} ({}) ma toccava al posto {p}",
                x.posto,
                x.origine.nome()
            ));
        }
        if x.apre != g.leader {
            return Err(format!("mossa {m}: chi apre la presa non torna"));
        }
        if x.presa != m / 2 {
            return Err(format!("mossa {m}: numero della presa sbagliato"));
        }
        if x.briscola != g.trump_card {
            return Err(format!("mossa {m}: briscola diversa dal mazzo"));
        }
        let mano: CardSet = x.mano.iter().copied().collect();
        if mano.len() as usize != x.mano.len() || mano != g.hands[p as usize] {
            return Err(format!("mossa {m}: la mano registrata non è quella vera"));
        }
        let legali: CardSet = x.legali.iter().copied().collect();
        if legali.len() as usize != x.legali.len() || legali != g.legal_moves() {
            return Err(format!("mossa {m}: le carte giocabili non tornano"));
        }
        if !mano.contains(x.scelta) {
            return Err(format!("mossa {m}: carta giocata non in mano"));
        }
        if x.tavolo != g.table {
            return Err(format!("mossa {m}: carta sul tavolo sbagliata"));
        }
        if x.mazzo_rimasto != g.deck_len {
            return Err(format!("mossa {m}: carte nel mazzo sbagliate"));
        }
        if x.punti_miei != g.points[p as usize] || x.punti_avversario != g.points[1 - p as usize] {
            return Err(format!("mossa {m}: punti sbagliati"));
        }
        if x.storia != storia {
            return Err(format!("mossa {m}: storia delle carte giocate sbagliata"));
        }
        if x.origine == Origine::Massima && x.tempo_ms.is_some() {
            return Err(format!("mossa {m}: tempo registrato per Massima"));
        }
        storia.push(x.scelta);
        g.play(x.scelta);
    }
    if !g.is_over() || g.points[0] as u32 + g.points[1] as u32 != 120 {
        return Err("a fine partita i punti non fanno 120".into());
    }
    Ok(g.points[umano as usize])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Tavolo, INTESTAZIONE};

    fn una_partita() -> String {
        let mut t = Tavolo::new(11, 1, 0, "g11", "p9", "s9", "test").con_iterazioni(200);
        while !t.finita() {
            if t.tocca_umano() {
                let c = t.vista().mano[0];
                t.gioca_umano(c, Some(800)).unwrap();
            } else {
                t.gioca_massima().unwrap();
            }
        }
        t.csv()
    }

    fn con_riga_cambiata(csv: &str, n: usize, f: impl Fn(&mut Vec<String>)) -> String {
        let mut righe: Vec<String> = csv.lines().map(String::from).collect();
        let mut celle: Vec<String> = righe[n].split(',').map(String::from).collect();
        f(&mut celle);
        righe[n] = celle.join(",");
        righe.join("\n") + "\n"
    }

    #[test]
    fn accetta_una_partita_vera_e_ignora_i_doppioni() {
        let p = una_partita();
        let r = valida(&format!("{INTESTAZIONE}\n{p}{p}"));
        assert_eq!(r.partite_valide, 1, "{}", r.testo());
        assert_eq!(r.duplicate, 1);
        assert_eq!(r.giocatori.len(), 1);
        assert_eq!(r.vinte + r.pari + r.perse, 1);
    }

    #[test]
    fn scarta_le_partite_impossibili_spiegando_perche() {
        let p = una_partita();
        let casi: Vec<(usize, usize, &str, &str)> = vec![
            (5, 18, "39", "carta giocata non in mano"),
            (5, 12, "0 1 2", "mano registrata"),
            (7, 15, "99", "punti sbagliati"),
            (3, 20, "true", "consigli visibili"),
            (9, 13, "", "tavolo"),
        ];
        for (riga, col, val, attesa) in casi {
            let cambiata = con_riga_cambiata(&p, riga, |c| c[col] = val.to_string());
            if cambiata == p {
                continue;
            }
            let r = valida(&format!("{INTESTAZIONE}\n{cambiata}"));
            assert_eq!(r.partite_valide, 0, "{attesa}: {}", r.testo());
            assert_eq!(r.scartate.len(), 1);
            assert!(
                r.scartate[0].1.contains(attesa),
                "{attesa}: {}",
                r.scartate[0].1
            );
        }
        // Partita a metà.
        let meta: String = p.lines().take(30).map(|l| l.to_string() + "\n").collect();
        let r = valida(&format!("{INTESTAZIONE}\n{meta}"));
        assert!(r.scartate[0].1.contains("30 carte"), "{}", r.testo());
    }

    #[test]
    fn righe_senza_intestazione_o_rotte() {
        let r = valida("ciao,mondo\n");
        assert_eq!(r.righe_illeggibili.len(), 1);
        let r = valida(&format!("{INTESTAZIONE}\nhuman_decision_v1,g,p\n"));
        assert_eq!(r.righe_illeggibili.len(), 1);
    }
}
