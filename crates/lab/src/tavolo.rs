//! Il tavolo: arbitra una partita fra una persona e Massima e registra ogni carta giocata.

use crate::riga::{Origine, Riga, SCHEMA};
use briscola_core::ai::{evaluate_with_log, EngineConfig};
use briscola_core::cards::*;
use briscola_core::game::{Game, Observation, Trick};
use briscola_core::infer::Log;
use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};

/// Iterazioni di Massima: le stesse dell'app (precisione "Massima") e dei test massivi.
pub const ITERAZIONI_MASSIMA: u32 = 20000;

/// Quello che la persona vede: niente mano di Massima, niente ordine del mazzo.
#[derive(Clone, Debug)]
pub struct Vista {
    pub mano: Vec<Card>,
    pub tavolo: Option<Card>,
    pub briscola: Card,
    /// Vero quando la briscola scoperta è già stata pescata.
    pub briscola_pescata: bool,
    pub mazzo_rimasto: u8,
    pub carte_massima: u8,
    pub punti_umano: u8,
    pub punti_massima: u8,
    /// "umano", "massima" o "fine".
    pub tocca: &'static str,
    pub ultima_presa: Option<Trick>,
    pub umano: u8,
}

impl Vista {
    pub fn json(&self) -> String {
        let lista = |v: &[Card]| {
            v.iter()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join(",")
        };
        let opz = |c: Option<Card>| c.map(|c| c.to_string()).unwrap_or_else(|| "null".into());
        let presa =
            match self.ultima_presa {
                None => "null".to_string(),
                Some(t) => {
                    format!(
                "{{\"apre\":\"{}\",\"prima\":{},\"seconda\":{},\"vince\":\"{}\",\"punti\":{}}}",
                if t.leader == self.umano { "umano" } else { "massima" },
                t.lead,
                t.follow,
                if t.winner == self.umano { "umano" } else { "massima" },
                t.points
            )
                }
            };
        format!(
            "{{\"mano\":[{}],\"tavolo\":{},\"briscola\":{},\"briscola_pescata\":{},\"mazzo_rimasto\":{},\"carte_massima\":{},\"punti_umano\":{},\"punti_massima\":{},\"tocca\":\"{}\",\"ultima_presa\":{}}}",
            lista(&self.mano),
            opz(self.tavolo),
            self.briscola,
            self.briscola_pescata,
            self.mazzo_rimasto,
            self.carte_massima,
            self.punti_umano,
            self.punti_massima,
            self.tocca,
            presa
        )
    }
}

pub struct Tavolo {
    g: Game,
    mazzo: Vec<Card>,
    umano: u8,
    storia: Vec<Card>,
    righe: Vec<Riga>,
    /// Registro degli eventi visto da Massima, per la deduzione delle carte della persona.
    log: Log,
    rng: SmallRng,
    ultima_presa: Option<Trick>,
    iterazioni: u32,
    pub partita: String,
    pub giocatore: String,
    pub sessione: String,
    pub fonte: String,
}

impl Tavolo {
    /// Nuova partita: mazzo mescolato dal seme; la persona siede al posto `umano`,
    /// e `apre` gioca la prima carta.
    pub fn new(
        seme: u64,
        umano: u8,
        apre: u8,
        partita: &str,
        giocatore: &str,
        sessione: &str,
        fonte: &str,
    ) -> Tavolo {
        // Mescolata portabile: StdRng (a differenza di SmallRng) è lo stesso a 32 e 64 bit, e gli
        // indici sono a 32 bit, quindi stesso seme = stesso mazzo nel browser e qui.
        let mut rng = rand::rngs::StdRng::seed_from_u64(seme);
        let mut mazzo: Vec<Card> = (0..NUM_CARDS as u8).collect();
        for i in (1..mazzo.len()).rev() {
            let j = rng.gen_range(0..=i as u32) as usize;
            mazzo.swap(i, j);
        }
        Tavolo::da_mazzo(
            &mazzo,
            umano,
            apre,
            seme ^ 0x9e37_79b9_7f4a_7c15,
            partita,
            giocatore,
            sessione,
            fonte,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn da_mazzo(
        mazzo: &[Card],
        umano: u8,
        apre: u8,
        seme_motore: u64,
        partita: &str,
        giocatore: &str,
        sessione: &str,
        fonte: &str,
    ) -> Tavolo {
        Tavolo {
            g: Game::from_order(mazzo, apre & 1),
            mazzo: mazzo.to_vec(),
            umano: umano & 1,
            storia: Vec::new(),
            righe: Vec::new(),
            log: Log::default(),
            rng: SmallRng::seed_from_u64(seme_motore),
            ultima_presa: None,
            iterazioni: ITERAZIONI_MASSIMA,
            partita: partita.into(),
            giocatore: giocatore.into(),
            sessione: sessione.into(),
            fonte: fonte.into(),
        }
    }

    /// Cambia le iterazioni di Massima (solo per i test veloci: il sito usa quelle standard).
    pub fn con_iterazioni(mut self, n: u32) -> Tavolo {
        self.iterazioni = n;
        self
    }

    pub fn motore(&self) -> String {
        format!(
            "massima-{}-ismcts{}-mani{}",
            env!("CARGO_PKG_VERSION"),
            self.iterazioni,
            briscola_core::ai::INFER_HANDS
        )
    }

    pub fn finita(&self) -> bool {
        self.g.is_over()
    }

    pub fn tocca_umano(&self) -> bool {
        !self.g.is_over() && self.g.to_move() == self.umano
    }

    pub fn vista(&self) -> Vista {
        let u = self.umano as usize;
        Vista {
            mano: self.g.hands[u].to_vec(),
            tavolo: self.g.table,
            briscola: self.g.trump_card,
            briscola_pescata: self.g.trump_card_owner.is_some(),
            mazzo_rimasto: self.g.deck_len,
            carte_massima: self.g.hands[1 - u].len() as u8,
            punti_umano: self.g.points[u],
            punti_massima: self.g.points[1 - u],
            tocca: if self.g.is_over() {
                "fine"
            } else if self.g.to_move() == self.umano {
                "umano"
            } else {
                "massima"
            },
            ultima_presa: self.ultima_presa,
            umano: self.umano,
        }
    }

    /// La persona gioca `c` dopo averci pensato `tempo_ms` millisecondi.
    pub fn gioca_umano(&mut self, c: Card, tempo_ms: Option<u32>) -> Result<(), String> {
        if !self.tocca_umano() {
            return Err("non tocca a te".into());
        }
        if !self.g.legal_moves().contains(c) {
            return Err("carta non in mano".into());
        }
        self.gioca(c, Origine::Umano, tempo_ms);
        Ok(())
    }

    /// Massima sceglie e gioca la sua carta, che viene restituita.
    pub fn gioca_massima(&mut self) -> Result<Card, String> {
        if self.g.is_over() || self.g.to_move() == self.umano {
            return Err("non tocca a Massima".into());
        }
        let io = 1 - self.umano;
        let obs = Observation::of(&self.g, io);
        let mut cfg = EngineConfig::strength(3000);
        cfg.ismcts = Some(self.iterazioni);
        let c = evaluate_with_log(&obs, &cfg, &self.log.events, &mut self.rng)[0].card;
        self.gioca(c, Origine::Massima, None);
        Ok(c)
    }

    fn gioca(&mut self, c: Card, origine: Origine, tempo_ms: Option<u32>) {
        let p = self.g.to_move();
        let mossa = self.storia.len() as u8;
        self.righe.push(Riga {
            schema: SCHEMA.into(),
            partita: self.partita.clone(),
            giocatore: self.giocatore.clone(),
            sessione: self.sessione.clone(),
            regole: crate::REGOLE.into(),
            fonte: self.fonte.clone(),
            origine,
            mossa,
            presa: mossa / 2,
            posto: p,
            apre: self.g.leader,
            briscola: self.g.trump_card,
            mano: self.g.hands[p as usize].to_vec(),
            tavolo: self.g.table,
            mazzo_rimasto: self.g.deck_len,
            punti_miei: self.g.points[p as usize],
            punti_avversario: self.g.points[1 - p as usize],
            legali: self.g.legal_moves().to_vec(),
            scelta: c,
            storia: self.storia.clone(),
            consigli_visibili: false,
            tempo_ms,
            qualita: "exact".into(),
            mazzo: if mossa == 0 {
                Some(self.mazzo.clone())
            } else {
                None
            },
            motore: self.motore(),
        });
        self.log.before_play(&self.g, 1 - self.umano, c);
        self.storia.push(c);
        if let Some(t) = self.g.play(c) {
            self.ultima_presa = Some(t);
        }
    }

    pub fn righe(&self) -> &[Riga] {
        &self.righe
    }

    /// Le righe CSV della partita, senza intestazione, una per riga.
    pub fn csv(&self) -> String {
        self.righe.iter().map(|r| r.to_csv() + "\n").collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{valida, INTESTAZIONE};

    /// Una partita intera: la "persona" gioca a caso, Massima con poche iterazioni.
    fn partita(seme: u64, umano: u8, apre: u8) -> Tavolo {
        let mut t = Tavolo::new(seme, umano, apre, &format!("g{seme}"), "p1", "s1", "test")
            .con_iterazioni(300);
        let mut r = SmallRng::seed_from_u64(seme + 1);
        while !t.finita() {
            if t.tocca_umano() {
                let mano = t.vista().mano;
                let c = mano[r.gen_range(0..mano.len())];
                t.gioca_umano(c, Some(1000)).unwrap();
            } else {
                t.gioca_massima().unwrap();
            }
        }
        t
    }

    #[test]
    fn partita_completa_e_valida() {
        let mut csv = String::from(INTESTAZIONE) + "\n";
        for s in 0..4u64 {
            let t = partita(s, (s % 2) as u8, ((s / 2) % 2) as u8);
            assert_eq!(t.righe().len(), 40);
            let v = t.vista();
            assert_eq!(v.punti_umano as u32 + v.punti_massima as u32, 120);
            assert_eq!(v.tocca, "fine");
            csv += &t.csv();
        }
        let r = valida(&csv);
        assert_eq!(r.scartate, vec![], "{r:?}");
        assert_eq!(r.partite_valide, 4);
        assert_eq!(r.decisioni_umane, 80);
    }

    #[test]
    fn la_persona_non_puo_giocare_carte_non_sue_o_fuori_turno() {
        let mut t = Tavolo::new(7, 0, 1, "g", "p", "s", "test").con_iterazioni(100);
        let mano = t.vista().mano;
        assert!(t.gioca_umano(mano[0], None).is_err(), "apre Massima");
        t.gioca_massima().unwrap();
        let altra = (0..40)
            .find(|c| !mano.contains(c) && Some(*c) != t.vista().tavolo)
            .unwrap();
        assert!(t.gioca_umano(altra, None).is_err());
        assert!(t.gioca_umano(mano[0], None).is_ok());
    }

    #[test]
    fn la_vista_non_mostra_le_carte_di_massima() {
        let t = Tavolo::new(3, 0, 0, "g", "p", "s", "test");
        let j = t.vista().json();
        let sue = t.g.hands[1].to_vec();
        assert_eq!(t.vista().mano.len(), 3);
        assert!(sue.iter().all(|c| !t.vista().mano.contains(c)));
        assert!(j.contains("\"carte_massima\":3"));
    }
}
