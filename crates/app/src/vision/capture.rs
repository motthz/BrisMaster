//! Cattura dello schermo. Su Windows si fotografano i monitor; si tiene quello dove si vede il
//! tavolo verde. Altrove (e nei test) si legge un'immagine indicata da BRISMASTER_SCHERMO.

use super::Img;

/// Fotografa il monitor `preferred` (o, se non c'è, il primo). Restituisce anche quanti monitor ci sono.
pub fn grab(preferred: usize) -> Result<(Img, usize), String> {
    if let Some(path) = std::env::var_os("BRISMASTER_SCHERMO") {
        return load_png(std::path::Path::new(&path)).map(|i| (i, 1));
    }
    grab_monitor(preferred)
}

pub fn load_png(path: &std::path::Path) -> Result<Img, String> {
    let im = image::open(path).map_err(|e| e.to_string())?.to_rgba8();
    Ok(Img::from_rgba(im.width() as usize, im.height() as usize, im.as_raw()))
}

#[cfg(windows)]
fn grab_monitor(preferred: usize) -> Result<(Img, usize), String> {
    let monitors = xcap::Monitor::all().map_err(|e| e.to_string())?;
    let m = monitors.get(preferred).or(monitors.first()).ok_or("nessun monitor trovato")?;
    let im = m.capture_image().map_err(|e| e.to_string())?;
    Ok((Img::from_rgba(im.width() as usize, im.height() as usize, im.as_raw()), monitors.len()))
}

#[cfg(not(windows))]
fn grab_monitor(_preferred: usize) -> Result<(Img, usize), String> {
    Err("la lettura dello schermo funziona su Windows".into())
}
