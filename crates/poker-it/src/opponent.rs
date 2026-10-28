//! Lettura dell'avversario: che cosa fa nelle varie situazioni e che mani mostra.
//!
//! Di ogni mano si contano le scelte dell'avversario per situazione pubblica (giro, posto,
//! puntate già fatte nel giro), quante carte cambia e, quando si arriva allo showdown, quanto
//! era forte la mano rispetto a come l'ha giocata nel secondo giro (chi punta forte con mani
//! deboli bluffa, chi vede tutto con mani deboli non lascia mai).
//!
//! Gli stessi conteggi, misurati in simulazione per ogni stile tipico (`Profile`), danno la
//! probabilità di quello che si è visto sotto ciascuno stile: da lì il peso di ogni stile.

use crate::cards::Card;
use crate::draw::strength;
use crate::game::Action;

/// Situazioni di puntata: giro (2) × posto (2) × puntate già fatte nel giro (0..=3).
pub const BET_CTX: usize = 16;
/// Come ha giocato il secondo giro chi arriva allo showdown: solo parole, ha visto, ha puntato.
pub const SD_CTX: usize = 3;
/// Livelli di forza della mano mostrata.
pub const SD_BINS: usize = 5;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stats {
    /// Lascio, vedo/parola, punto/rilancio per situazione.
    pub bet: [[f32; 3]; BET_CTX],
    /// Carte cambiate (0..=5).
    pub draw: [f32; 6],
    pub showdown: [[f32; SD_BINS]; SD_CTX],
    pub hands: u32,
}

/// Situazione e scelta (0 lascio, 1 vedo, 2 rilancio) di ogni decisione di puntata di `seat`,
/// più le carte cambiate e come ha giocato il secondo giro.
fn replay(history: &[Action], seat: u8) -> (Vec<(usize, usize)>, Option<u8>, usize) {
    let mut out = Vec::new();
    let (mut round, mut to_act, mut bets, mut acted) = (0usize, 0u8, 0usize, 0u8);
    let mut drawn = None;
    let mut line2 = 0;
    for &a in history {
        if let Action::Draw(k) = a {
            if to_act == seat {
                drawn = Some(k);
            }
            to_act = 1 - to_act;
            if to_act == 0 {
                round = 1;
            }
            continue;
        }
        let me = to_act == seat;
        if me {
            let choice = match a {
                Action::Fold => 0,
                Action::Call => 1,
                _ => 2,
            };
            out.push((round * 8 + seat as usize * 4 + bets.min(3), choice));
            if round == 1 {
                line2 = line2.max(match (choice, bets > 0) {
                    (2, _) => 2,
                    (1, true) => 1,
                    _ => 0,
                });
            }
        }
        match a {
            Action::Raise => {
                bets += 1;
                acted += 1;
                to_act = 1 - to_act;
            }
            Action::Call => {
                acted += 1;
                if bets > 0 || acted >= 2 {
                    // Fine del giro: si passa al cambio (o allo showdown), parla il posto 0.
                    bets = 0;
                    acted = 0;
                    to_act = 0;
                } else {
                    to_act = 1 - to_act;
                }
            }
            _ => {}
        }
    }
    (out, drawn, line2)
}

impl Stats {
    /// Aggiunge una mano finita: la storia pubblica, il posto dell'avversario e la sua mano
    /// finale se l'ha mostrata.
    pub fn record(&mut self, history: &[Action], seat: u8, shown: Option<&[Card; 5]>) {
        let (decisions, drawn, line2) = replay(history, seat);
        for (ctx, c) in decisions {
            self.bet[ctx][c] += 1.0;
        }
        if let Some(k) = drawn {
            self.draw[k as usize] += 1.0;
        }
        if let Some(h) = shown {
            let bin = ((strength(h) * SD_BINS as f32) as usize).min(SD_BINS - 1);
            self.showdown[line2][bin] += 1.0;
        }
        self.hands += 1;
    }

    /// Tutti i conteggi in fila (per salvarli), con il numero di mani in fondo.
    pub fn to_vec(&self) -> Vec<f32> {
        let mut v: Vec<f32> = self.bet.iter().flatten().copied().collect();
        v.extend(self.draw);
        v.extend(self.showdown.iter().flatten());
        v.push(self.hands as f32);
        v
    }

    pub fn from_vec(v: &[f32]) -> Option<Stats> {
        if v.len() != BET_CTX * 3 + 6 + SD_CTX * SD_BINS + 1 {
            return None;
        }
        let mut s = Stats::default();
        let mut it = v.iter().copied();
        for x in s.bet.iter_mut().flatten().chain(s.draw.iter_mut()).chain(s.showdown.iter_mut().flatten()) {
            *x = it.next()?;
        }
        s.hands = it.next()? as u32;
        Some(s)
    }

    /// Riduce il peso di quello che si è visto finora (il numero di mani resta).
    pub fn fade(&mut self, f: f32) {
        for x in self.bet.iter_mut().flatten().chain(self.draw.iter_mut()).chain(self.showdown.iter_mut().flatten()) {
            *x *= f;
        }
    }

    pub fn merge(&mut self, o: &Stats) {
        for (a, b) in self.bet.iter_mut().zip(&o.bet) {
            for (x, y) in a.iter_mut().zip(b) {
                *x += y;
            }
        }
        for (x, y) in self.draw.iter_mut().zip(&o.draw) {
            *x += y;
        }
        for (a, b) in self.showdown.iter_mut().zip(&o.showdown) {
            for (x, y) in a.iter_mut().zip(b) {
                *x += y;
            }
        }
        self.hands += o.hands;
    }

    /// Logaritmo della probabilità dei conteggi `self` se l'avversario gioca come `model`
    /// (frequenze di `model` con una pseudo-osservazione per casella).
    pub fn log_likelihood(&self, model: &Stats) -> f64 {
        fn part(obs: &[f32], model: &[f32]) -> f64 {
            let tot: f64 = model.iter().map(|&x| x as f64 + 1.0).sum();
            obs.iter().zip(model).map(|(&o, &m)| if o > 0.0 { o as f64 * ((m as f64 + 1.0) / tot).ln() } else { 0.0 }).sum()
        }
        let mut ll = 0.0;
        for (ctx, o) in self.bet.iter().enumerate() {
            // Con tre puntate fatte non si può rilanciare.
            let n = if ctx % 4 == 3 { 2 } else { 3 };
            ll += part(&o[..n], &model.bet[ctx][..n]);
        }
        ll += part(&self.draw, &model.draw);
        for (o, m) in self.showdown.iter().zip(&model.showdown) {
            ll += part(o, m);
        }
        ll
    }

    /// Frequenza di un insieme di scelte su tutte le situazioni indicate.
    fn freq(&self, ctxs: impl Iterator<Item = usize>, num: &[usize], den: &[usize]) -> Option<f32> {
        let (mut a, mut b) = (0.0, 0.0);
        for c in ctxs {
            a += num.iter().map(|&i| self.bet[c][i]).sum::<f32>();
            b += den.iter().map(|&i| self.bet[c][i]).sum::<f32>();
        }
        (b >= 1.0).then(|| a / b)
    }

    /// Quanto spesso lascia quando gli si punta contro.
    pub fn fold_to_bet(&self) -> Option<f32> {
        self.freq((0..BET_CTX).filter(|c| c % 4 > 0), &[0], &[0, 1, 2])
    }

    /// Quanto spesso punta per primo quando nessuno ha ancora puntato.
    pub fn bet_first(&self) -> Option<f32> {
        self.freq((0..BET_CTX).filter(|c| c % 4 == 0), &[2], &[1, 2])
    }

    /// Quote delle mani deboli (metà bassa) tra quelle mostrate dopo aver puntato nel secondo giro.
    pub fn weak_when_betting(&self) -> Option<f32> {
        let s = &self.showdown[2];
        let tot: f32 = s.iter().sum();
        (tot >= 1.0).then(|| (s[0] + s[1]) / tot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Action::*;

    #[test]
    fn saved_and_loaded() {
        let mut s = Stats::default();
        s.record(&[Raise, Call, Draw(2), Draw(0), Raise, Call], 1, None);
        assert_eq!(Stats::from_vec(&s.to_vec()), Some(s));
    }

    #[test]
    fn contexts_follow_the_betting() {
        // Posto 0 punta, posto 1 rilancia, posto 0 vede; cambio 3 e 1; parola, punta, lascia.
        let h = [Raise, Raise, Call, Draw(3), Draw(1), Call, Raise, Fold];
        let (d0, k0, l0) = replay(&h, 0);
        assert_eq!(d0, vec![(0, 2), (2, 1), (8, 1), (9, 0)]);
        assert_eq!((k0, l0), (Some(3), 0));
        let (d1, k1, l1) = replay(&h, 1);
        assert_eq!(d1, vec![(5, 2), (12, 2)]);
        assert_eq!((k1, l1), (Some(1), 2));
    }
}
