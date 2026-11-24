//! Una riga del CSV `human_decision_v1`: una carta giocata, con quello che vedeva chi l'ha giocata.
//!
//! Le carte sono numeri da 0 a 39 come in `briscola_core::cards` (seme = c / 10 con
//! 0 Denari, 1 Coppe, 2 Spade, 3 Bastoni; valore = c % 10 con 0 Asso, 1 Due, 2 Tre, ...,
//! 6 Sette, 7 Fante, 8 Cavallo, 9 Re). Gli elenchi di carte sono separati da spazi.

use briscola_core::cards::Card;

pub const SCHEMA: &str = "human_decision_v1";

/// Colonne del CSV, in ordine.
pub const COLONNE: [&str; 25] = [
    "schema",
    "partita",
    "giocatore",
    "sessione",
    "regole",
    "fonte",
    "origine",
    "mossa",
    "presa",
    "posto",
    "apre",
    "briscola",
    "mano",
    "tavolo",
    "mazzo_rimasto",
    "punti_miei",
    "punti_avversario",
    "legali",
    "scelta",
    "storia",
    "consigli_visibili",
    "tempo_ms",
    "qualita",
    "mazzo",
    "motore",
];

pub const INTESTAZIONE: &str = "schema,partita,giocatore,sessione,regole,fonte,origine,mossa,presa,posto,apre,briscola,mano,tavolo,mazzo_rimasto,punti_miei,punti_avversario,legali,scelta,storia,consigli_visibili,tempo_ms,qualita,mazzo,motore";

/// Chi ha giocato la carta.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origine {
    Umano,
    Massima,
}

impl Origine {
    pub fn nome(self) -> &'static str {
        match self {
            Origine::Umano => "umano",
            Origine::Massima => "massima",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Riga {
    pub schema: String,
    pub partita: String,
    /// Pseudonimo del giocatore umano della partita (mai nome, email o IP).
    pub giocatore: String,
    pub sessione: String,
    pub regole: String,
    pub fonte: String,
    pub origine: Origine,
    /// Numero della carta giocata nella partita, da 0 a 39.
    pub mossa: u8,
    /// Numero della presa, da 0 a 19.
    pub presa: u8,
    /// Posto di chi gioca la carta (0 o 1).
    pub posto: u8,
    /// Posto di chi apre la presa.
    pub apre: u8,
    /// Briscola scoperta (la carta in fondo al mazzo).
    pub briscola: Card,
    /// Mano di chi gioca, prima di giocare.
    pub mano: Vec<Card>,
    /// Carta già sul tavolo, se chi gioca risponde.
    pub tavolo: Option<Card>,
    pub mazzo_rimasto: u8,
    pub punti_miei: u8,
    pub punti_avversario: u8,
    pub legali: Vec<Card>,
    pub scelta: Card,
    /// Tutte le carte giocate prima di questa, in ordine.
    pub storia: Vec<Card>,
    pub consigli_visibili: bool,
    /// Tempo della decisione, solo se misurato davvero (per Massima resta vuoto).
    pub tempo_ms: Option<u32>,
    pub qualita: String,
    /// Ordine completo del mazzo, solo sulla riga della mossa 0 (`order[0]` = briscola,
    /// si pesca da `order[39]` in giù, come `Game::from_order`).
    pub mazzo: Option<Vec<Card>>,
    /// Versione e impostazioni di Massima che ha giocato la partita.
    pub motore: String,
}

/// Toglie dai testi liberi i caratteri che romperebbero il CSV.
pub fn pulisci(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c == ',' || c == '"' || c == '\n' || c == '\r' {
                '_'
            } else {
                c
            }
        })
        .take(80)
        .collect()
}

fn lista(v: &[Card]) -> String {
    v.iter()
        .map(|c| c.to_string())
        .collect::<Vec<_>>()
        .join(" ")
}

fn leggi_lista(s: &str) -> Result<Vec<Card>, String> {
    s.split_whitespace().map(leggi_carta).collect()
}

fn leggi_carta(s: &str) -> Result<Card, String> {
    let c: u8 = s
        .trim()
        .parse()
        .map_err(|_| format!("carta non valida '{s}'"))?;
    if c >= 40 {
        return Err(format!("carta fuori dal mazzo '{s}'"));
    }
    Ok(c)
}

fn leggi_n<T: std::str::FromStr>(s: &str, campo: &str) -> Result<T, String> {
    s.trim()
        .parse()
        .map_err(|_| format!("{campo} non valido '{s}'"))
}

impl Riga {
    pub fn to_csv(&self) -> String {
        let campi = [
            pulisci(&self.schema),
            pulisci(&self.partita),
            pulisci(&self.giocatore),
            pulisci(&self.sessione),
            pulisci(&self.regole),
            pulisci(&self.fonte),
            self.origine.nome().to_string(),
            self.mossa.to_string(),
            self.presa.to_string(),
            self.posto.to_string(),
            self.apre.to_string(),
            self.briscola.to_string(),
            lista(&self.mano),
            self.tavolo.map(|c| c.to_string()).unwrap_or_default(),
            self.mazzo_rimasto.to_string(),
            self.punti_miei.to_string(),
            self.punti_avversario.to_string(),
            lista(&self.legali),
            self.scelta.to_string(),
            lista(&self.storia),
            self.consigli_visibili.to_string(),
            self.tempo_ms.map(|t| t.to_string()).unwrap_or_default(),
            pulisci(&self.qualita),
            self.mazzo.as_deref().map(lista).unwrap_or_default(),
            pulisci(&self.motore),
        ];
        campi.join(",")
    }

    /// Legge una riga CSV con le colonne nell'ordine dato da `indici` (posizione di ogni
    /// colonna di [`COLONNE`] nella riga), così l'ordine delle colonne nel file può cambiare.
    pub fn from_csv(linea: &str, indici: &[usize; 25]) -> Result<Riga, String> {
        let celle: Vec<&str> = linea.trim_end_matches(['\r', '\n']).split(',').collect();
        let get = |k: usize| -> Result<&str, String> {
            celle
                .get(indici[k])
                .copied()
                .ok_or_else(|| format!("manca la colonna {}", COLONNE[k]))
        };
        let origine = match get(6)?.trim() {
            "umano" => Origine::Umano,
            "massima" => Origine::Massima,
            o => return Err(format!("origine sconosciuta '{o}'")),
        };
        let tavolo = match get(13)?.trim() {
            "" => None,
            t => Some(leggi_carta(t)?),
        };
        let tempo_ms = match get(21)?.trim() {
            "" => None,
            t => Some(leggi_n(t, "tempo_ms")?),
        };
        let mazzo = match get(23)?.trim() {
            "" => None,
            m => Some(leggi_lista(m)?),
        };
        let consigli_visibili = match get(20)?.trim() {
            "true" => true,
            "false" => false,
            v => return Err(format!("consigli_visibili non valido '{v}'")),
        };
        Ok(Riga {
            schema: get(0)?.to_string(),
            partita: get(1)?.to_string(),
            giocatore: get(2)?.to_string(),
            sessione: get(3)?.to_string(),
            regole: get(4)?.to_string(),
            fonte: get(5)?.to_string(),
            origine,
            mossa: leggi_n(get(7)?, "mossa")?,
            presa: leggi_n(get(8)?, "presa")?,
            posto: leggi_n(get(9)?, "posto")?,
            apre: leggi_n(get(10)?, "apre")?,
            briscola: leggi_carta(get(11)?)?,
            mano: leggi_lista(get(12)?)?,
            tavolo,
            mazzo_rimasto: leggi_n(get(14)?, "mazzo_rimasto")?,
            punti_miei: leggi_n(get(15)?, "punti_miei")?,
            punti_avversario: leggi_n(get(16)?, "punti_avversario")?,
            legali: leggi_lista(get(17)?)?,
            scelta: leggi_carta(get(18)?)?,
            storia: leggi_lista(get(19)?)?,
            consigli_visibili,
            tempo_ms,
            qualita: get(22)?.to_string(),
            mazzo,
            motore: get(24)?.to_string(),
        })
    }
}

/// Posizione di ogni colonna richiesta nell'intestazione del file.
/// Le colonne in più (per esempio `codice_ip` aggiunta dal server) sono ignorate.
pub fn indici(intestazione: &str) -> Result<[usize; 25], String> {
    let nomi: Vec<&str> = intestazione
        .trim_start_matches('\u{feff}')
        .trim_end()
        .split(',')
        .map(str::trim)
        .collect();
    let mut out = [0usize; 25];
    for (k, col) in COLONNE.iter().enumerate() {
        out[k] = nomi
            .iter()
            .position(|n| n == col)
            .ok_or_else(|| format!("manca la colonna '{col}' nell'intestazione"))?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intestazione_e_colonne_coincidono() {
        assert_eq!(INTESTAZIONE, COLONNE.join(","));
    }

    #[test]
    fn riga_va_e_torna() {
        let r = Riga {
            schema: SCHEMA.into(),
            partita: "g1".into(),
            giocatore: "p,1".into(),
            sessione: "s1".into(),
            regole: crate::REGOLE.into(),
            fonte: "lab".into(),
            origine: Origine::Umano,
            mossa: 0,
            presa: 0,
            posto: 0,
            apre: 0,
            briscola: 12,
            mano: vec![1, 2, 3],
            tavolo: None,
            mazzo_rimasto: 34,
            punti_miei: 0,
            punti_avversario: 0,
            legali: vec![1, 2, 3],
            scelta: 2,
            storia: vec![],
            consigli_visibili: false,
            tempo_ms: Some(1500),
            qualita: "exact".into(),
            mazzo: Some((0..40).collect()),
            motore: "m".into(),
        };
        let idx = indici(INTESTAZIONE).unwrap();
        let back = Riga::from_csv(&r.to_csv(), &idx).unwrap();
        assert_eq!(back.giocatore, "p_1");
        assert_eq!(
            Riga {
                giocatore: "p,1".into(),
                ..back
            },
            r
        );
    }

    #[test]
    fn colonne_in_piu_e_in_altro_ordine() {
        let h = format!(
            "codice_ip,{}",
            COLONNE.iter().rev().copied().collect::<Vec<_>>().join(",")
        );
        let idx = indici(&h).unwrap();
        assert_eq!(idx[0], 25);
        assert_eq!(idx[24], 1);
    }
}
