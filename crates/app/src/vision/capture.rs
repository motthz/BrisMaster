//! Cattura dello schermo. Su Windows si fotografano i monitor; si tiene quello dove si vede il
//! tavolo verde. Altrove (e nei test) si legge un'immagine indicata da BRISMASTER_SCHERMO.

use super::Img;

/// Dove sta il monitor fotografato sul desktop: posizione e misure in pixel veri, e quanti pixel
/// veri fanno un punto dell'interfaccia (il ridimensionamento di Windows, 1.25 = 125%).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Geom {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub scale: f32,
}

/// Fotografa il monitor `preferred` (o, se non c'è, il primo). Restituisce anche quanti monitor ci sono.
pub fn grab(preferred: usize) -> Result<(Img, usize), String> {
    grab_geom(preferred).map(|(img, n, _)| (img, n))
}

/// Come `grab`, con in più dove sta il monitor sul desktop.
pub fn grab_geom(preferred: usize) -> Result<(Img, usize, Geom), String> {
    if let Some(path) = std::env::var_os("BRISMASTER_SCHERMO") {
        let img = load_png(std::path::Path::new(&path))?;
        let g = Geom { x: 0.0, y: 0.0, w: img.w as f32, h: img.h as f32, scale: 1.0 };
        return Ok((img, 1, g));
    }
    grab_monitor(preferred)
}

pub fn load_png(path: &std::path::Path) -> Result<Img, String> {
    let im = image::open(path).map_err(|e| e.to_string())?.to_rgba8();
    Ok(Img::from_rgba(im.width() as usize, im.height() as usize, im.as_raw()))
}

#[cfg(windows)]
fn grab_monitor(preferred: usize) -> Result<(Img, usize, Geom), String> {
    let monitors = xcap::Monitor::all().map_err(|e| e.to_string())?;
    let m = monitors.get(preferred).or(monitors.first()).ok_or("nessun monitor trovato")?;
    let im = m.capture_image().map_err(|e| e.to_string())?;
    let img = Img::from_rgba(im.width() as usize, im.height() as usize, im.as_raw());
    let g = Geom {
        x: m.x().unwrap_or(0) as f32,
        y: m.y().unwrap_or(0) as f32,
        // Le misure dalla foto: sono sempre in pixel veri.
        w: img.w as f32,
        h: img.h as f32,
        scale: m.scale_factor().unwrap_or(1.0).max(0.5),
    };
    Ok((img, monitors.len(), g))
}

#[cfg(not(windows))]
fn grab_monitor(_preferred: usize) -> Result<(Img, usize, Geom), String> {
    Err("la lettura dello schermo funziona su Windows".into())
}
