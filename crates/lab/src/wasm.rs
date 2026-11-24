//! Interfaccia WebAssembly per il sito, senza dipendenze: funzioni `extern "C"` e testi
//! scambiati attraverso la memoria. Il JavaScript scrive un testo con `lab_alloc` e lo passa
//! come (puntatore, lunghezza); le risposte si leggono da `lab_out_ptr` / `lab_out_len`.
//! Una sola partita alla volta, tenuta qui dentro: la pagina vede solo la [`crate::Vista`].

use crate::{Tavolo, INTESTAZIONE};
use std::cell::RefCell;

thread_local! {
    static TAVOLO: RefCell<Option<Tavolo>> = const { RefCell::new(None) };
    static OUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

// Il motore usa solo generatori con seme: nessuna sorgente casuale del sistema.
fn nessuna_entropia(_: &mut [u8]) -> Result<(), getrandom::Error> {
    Err(getrandom::Error::UNSUPPORTED)
}
getrandom::register_custom_getrandom!(nessuna_entropia);

fn scrivi(s: String) -> i32 {
    let n = s.len() as i32;
    OUT.with(|o| *o.borrow_mut() = s.into_bytes());
    n
}

unsafe fn testo(ptr: *const u8, len: usize) -> String {
    if ptr.is_null() || len == 0 {
        return String::new();
    }
    String::from_utf8_lossy(std::slice::from_raw_parts(ptr, len)).into_owned()
}

#[no_mangle]
pub extern "C" fn lab_alloc(len: usize) -> *mut u8 {
    let mut v = Vec::<u8>::with_capacity(len.max(1));
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

/// # Safety
/// `ptr` deve venire da `lab_alloc(len)`.
#[no_mangle]
pub unsafe extern "C" fn lab_free(ptr: *mut u8, len: usize) {
    drop(Vec::from_raw_parts(ptr, 0, len.max(1)));
}

#[no_mangle]
pub extern "C" fn lab_out_ptr() -> *const u8 {
    OUT.with(|o| o.borrow().as_ptr())
}

#[no_mangle]
pub extern "C" fn lab_out_len() -> usize {
    OUT.with(|o| o.borrow().len())
}

/// Nuova partita. `meta` è "partita,giocatore,sessione,fonte". Scrive la vista.
///
/// # Safety
/// `meta` deve puntare a `meta_len` byte validi.
#[no_mangle]
pub unsafe extern "C" fn lab_nuova(
    seme_alto: u32,
    seme_basso: u32,
    umano: u32,
    apre: u32,
    meta: *const u8,
    meta_len: usize,
) -> i32 {
    let m = testo(meta, meta_len);
    let p: Vec<&str> = m.splitn(4, ',').collect();
    let get = |i: usize| p.get(i).copied().unwrap_or("");
    let seme = ((seme_alto as u64) << 32) | seme_basso as u64;
    let t = Tavolo::new(
        seme,
        umano as u8,
        apre as u8,
        get(0),
        get(1),
        get(2),
        get(3),
    );
    let v = t.vista().json();
    TAVOLO.with(|x| *x.borrow_mut() = Some(t));
    scrivi(v)
}

/// Scrive la vista attuale (o "null" senza partita).
#[no_mangle]
pub extern "C" fn lab_vista() -> i32 {
    let v = TAVOLO.with(|x| x.borrow().as_ref().map(|t| t.vista().json()));
    scrivi(v.unwrap_or_else(|| "null".into()))
}

/// La persona gioca `carta`; `tempo_ms` < 0 se non misurato. 0 se valida, -1 altrimenti.
#[no_mangle]
pub extern "C" fn lab_gioca(carta: u32, tempo_ms: i32) -> i32 {
    let r = TAVOLO.with(|x| match x.borrow_mut().as_mut() {
        Some(t) => t.gioca_umano(carta as u8, (tempo_ms >= 0).then_some(tempo_ms as u32)),
        None => Err("nessuna partita".into()),
    });
    match r {
        Ok(()) => 0,
        Err(e) => {
            scrivi(e);
            -1
        }
    }
}

/// Massima gioca: restituisce la carta, o -1.
#[no_mangle]
pub extern "C" fn lab_massima() -> i32 {
    let r = TAVOLO.with(|x| match x.borrow_mut().as_mut() {
        Some(t) => t.gioca_massima(),
        None => Err("nessuna partita".into()),
    });
    match r {
        Ok(c) => c as i32,
        Err(e) => {
            scrivi(e);
            -1
        }
    }
}

/// Scrive il CSV della partita (con intestazione se `intestazione` != 0).
#[no_mangle]
pub extern "C" fn lab_csv(intestazione: u32) -> i32 {
    let c = TAVOLO
        .with(|x| x.borrow().as_ref().map(|t| t.csv()))
        .unwrap_or_default();
    scrivi(if intestazione != 0 {
        format!("{INTESTAZIONE}\n{c}")
    } else {
        c
    })
}

/// Valida un CSV e scrive il rapporto (lo usa la pagina dei risultati).
///
/// # Safety
/// `ptr` deve puntare a `len` byte validi.
#[no_mangle]
pub unsafe extern "C" fn lab_valida(ptr: *const u8, len: usize) -> i32 {
    let r = crate::valida(&testo(ptr, len));
    scrivi(r.testo())
}
