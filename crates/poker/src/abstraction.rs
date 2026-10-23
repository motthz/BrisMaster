//! Astrazione del gioco per il motore: poche misure di puntata e mani raggruppate per forza ("bucket").
//!
//! L'albero delle azioni è costruito con le regole vere (`Hand`), quindi le chip di ogni nodo
//! sono esatte. Le posizioni sono fisse: il giocatore 0 è il bottone (piccolo buio).

use crate::game::{Action, Hand};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AbsAction {
    Fold,
    Call,
    /// Rilancio di `frac` volte il piatto dopo aver pagato.
    Raise(f32),
    AllIn,
}

impl AbsAction {
    /// Frazione di piatto equivalente (per confrontare rilanci veri con quelli dell'albero).
    pub fn label(self) -> String {
        match self {
            AbsAction::Fold => "Fold".into(),
            AbsAction::Call => "Check/Call".into(),
            AbsAction::Raise(f) => format!("Punta/rilancia {:.0}% piatto", f * 100.0),
            AbsAction::AllIn => "All-in".into(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Decision,
    /// Mano finita con fold del giocatore indicato.
    Fold(u8),
    Showdown,
}

#[derive(Clone, Debug)]
pub struct Node {
    pub kind: Kind,
    pub player: u8,
    pub street: u8,
    pub committed: [u32; 2],
    pub actions: Vec<AbsAction>,
    /// Frazione di piatto di ciascuna azione di rilancio (all-in compreso), per la traduzione.
    pub fracs: Vec<f32>,
    pub children: Vec<u32>,
    /// Inizio delle righe di questo nodo nelle tabelle della strategia.
    pub offset: usize,
}

/// Misure dei rilanci (frazioni di piatto) per giro e numero di rilanci già fatti nel giro.
pub fn raise_sizes(street: u8, raises: u8) -> &'static [f32] {
    match (street, raises) {
        (0, 0) => &[0.75, 1.5],   // apertura a 2,5 o 4 grandi bui
        (0, 1) => &[1.0],         // 3-bet a 3 volte
        (0, 2) => &[0.6],         // 4-bet a circa 2,2 volte
        (_, 0) => &[0.33, 0.75, 1.25],
        (_, 1) => &[0.8],
        _ => &[],
    }
}

/// Numero di bucket per giro (preflop: le 169 classi di mani).
pub const BUCKETS: [usize; 4] = [169, 50, 50, 50];

pub struct Tree {
    pub nodes: Vec<Node>,
    pub size: usize,
}

/// Azioni astratte possibili in uno stato vero, con la frazione di piatto dei rilanci.
pub fn abstract_actions(h: &Hand) -> Vec<(AbsAction, Action, f32)> {
    let mut out = Vec::new();
    let to_call = h.to_call();
    if to_call > 0 {
        out.push((AbsAction::Fold, Action::Fold, 0.0));
    }
    out.push((AbsAction::Call, Action::Call, 0.0));
    if let Some((lo, hi)) = h.raise_range() {
        let cur = h.street_bet[0].max(h.street_bet[1]);
        let base = (h.pot() + to_call) as f32;
        let mut last = 0;
        if h.raises_this_street() < 4 {
            for &f in raise_sizes(h.street, h.raises_this_street()) {
                let t = ((cur as f32 + f * base).round() as u32).max(lo);
                if t as f32 >= 0.85 * hi as f32 || t <= last {
                    continue;
                }
                last = t;
                out.push((AbsAction::Raise(f), Action::RaiseTo(t), (t - cur) as f32 / base));
            }
            out.push((AbsAction::AllIn, Action::RaiseTo(hi), (hi - cur) as f32 / base));
        }
    }
    out
}

impl Tree {
    pub fn build() -> Tree {
        let mut t = Tree { nodes: Vec::new(), size: 0 };
        let h = Hand::with_cards([[0, 1], [2, 3]], [4, 5, 6, 7, 8], 0);
        t.add(&h);
        t
    }

    fn add(&mut self, h: &Hand) -> u32 {
        let id = self.nodes.len() as u32;
        let kind = match (h.over, h.folded) {
            (true, Some(f)) => Kind::Fold(f),
            (true, None) => Kind::Showdown,
            _ => Kind::Decision,
        };
        self.nodes.push(Node {
            kind,
            player: h.to_act,
            street: h.street,
            committed: h.committed,
            actions: Vec::new(),
            fracs: Vec::new(),
            children: Vec::new(),
            offset: 0,
        });
        if kind != Kind::Decision {
            return id;
        }
        let acts = abstract_actions(h);
        let offset = self.size;
        self.size += BUCKETS[h.street as usize] * acts.len();
        let mut children = Vec::new();
        for &(_, a, _) in &acts {
            let mut c = h.clone();
            c.apply(a);
            children.push(self.add(&c));
        }
        let n = &mut self.nodes[id as usize];
        n.actions = acts.iter().map(|x| x.0).collect();
        n.fracs = acts.iter().map(|x| x.2).collect();
        n.children = children;
        n.offset = offset;
        id
    }

    pub fn decision_count(&self) -> usize {
        self.nodes.iter().filter(|n| n.kind == Kind::Decision).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::STACK;

    #[test]
    fn tree_is_sane() {
        let t = Tree::build();
        assert!(t.nodes.len() > 1000);
        for n in &t.nodes {
            if n.kind == Kind::Decision {
                assert!(n.actions.len() >= 2);
                assert!(n.committed[0] <= STACK && n.committed[1] <= STACK);
            }
        }
        let root = &t.nodes[0];
        assert_eq!(root.actions, vec![AbsAction::Fold, AbsAction::Call, AbsAction::Raise(0.75), AbsAction::Raise(1.5), AbsAction::AllIn]);
        // L'apertura a 0,75 piatto è 2,5 grandi bui.
        let open = &t.nodes[root.children[2] as usize];
        assert_eq!(open.committed[0], 250);
    }
}

#[cfg(test)]
mod size {
    #[test]
    #[ignore]
    fn print_size() {
        let t = super::Tree::build();
        let mut per = [0usize; 4];
        for n in &t.nodes {
            if n.kind == super::Kind::Decision {
                per[n.street as usize] += 1;
            }
        }
        println!("nodi {} decisioni {:?} righe {}", t.nodes.len(), per, t.size);
    }
}
