//! Aggiornamento automatico dalle GitHub Releases del repository pubblico.
//!
//! All'avvio l'app interroga l'ultima release; se il numero di versione è più alto di quello in
//! esecuzione, su richiesta scarica l'installatore `BrisMaster-Setup.exe`, lo avvia e si chiude:
//! l'installatore sostituisce l'app nella sua cartella e la riavvia. L'app non sostituisce più sé
//! stessa mentre è in esecuzione: è il comportamento che gli antivirus (Bitdefender in testa)
//! trattano come quello di un programma malevolo.

use serde::Deserialize;
use std::io::Read;

pub const REPO: &str = "motthz/BrisMaster";
pub const ASSET_NAME: &str = "BrisMaster-Setup.exe";
pub const CURRENT: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Debug)]
pub struct Release {
    pub version: semver::Version,
    pub notes: String,
    pub asset_url: String,
    pub page_url: String,
}

#[derive(Deserialize)]
struct ApiAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Deserialize)]
struct ApiRelease {
    tag_name: String,
    #[serde(default)]
    body: Option<String>,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    assets: Vec<ApiAsset>,
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(10))
        .timeout_read(std::time::Duration::from_secs(60))
        .user_agent(&format!("BrisMaster/{CURRENT}"))
        .build()
}

/// Restituisce la release più recente se è più nuova della versione in uso.
pub fn check() -> Result<Option<Release>, String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let resp = match agent().get(&url).set("Accept", "application/vnd.github+json").call() {
        Ok(r) => r,
        // Nessuna release ancora pubblicata.
        Err(ureq::Error::Status(404, _)) => return Ok(None),
        Err(e) => return Err(format!("controllo aggiornamenti non riuscito: {e}")),
    };
    let rel: ApiRelease = resp.into_json().map_err(|e| e.to_string())?;
    if rel.draft || rel.prerelease {
        return Ok(None);
    }
    let latest = parse_version(&rel.tag_name)?;
    let current = semver::Version::parse(CURRENT).map_err(|e| e.to_string())?;
    if latest <= current {
        return Ok(None);
    }
    let asset = rel
        .assets
        .iter()
        .find(|a| a.name.eq_ignore_ascii_case(ASSET_NAME))
        .ok_or_else(|| format!("la release {} non contiene {ASSET_NAME}", rel.tag_name))?;
    Ok(Some(Release {
        version: latest,
        notes: rel.body.unwrap_or_default(),
        asset_url: asset.browser_download_url.clone(),
        page_url: rel.html_url,
    }))
}

pub fn parse_version(tag: &str) -> Result<semver::Version, String> {
    semver::Version::parse(tag.trim_start_matches(['v', 'V'])).map_err(|e| format!("tag {tag}: {e}"))
}

/// Scarica l'installatore della nuova versione nella cartella temporanea, lo avvia (aggiorna
/// l'app installata e la riapre) e chiude l'app.
pub fn install(rel: &Release) -> Result<(), String> {
    let resp = agent().get(&rel.asset_url).call().map_err(|e| format!("download non riuscito: {e}"))?;
    let mut bytes = Vec::new();
    resp.into_reader()
        .take(200 * 1024 * 1024)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("download interrotto: {e}"))?;
    // Un eseguibile Windows valido comincia con "MZ".
    if bytes.len() < 1024 || &bytes[..2] != b"MZ" {
        return Err("il file scaricato non è un eseguibile valido".into());
    }
    let setup = std::env::temp_dir().join(format!("BrisMaster-Setup-{}.exe", rel.version));
    std::fs::write(&setup, &bytes).map_err(|e| format!("impossibile scrivere {}: {e}", setup.display()))?;
    // Installazione senza domande (si vede solo l'avanzamento); a fine installazione l'app si riapre.
    std::process::Command::new(&setup)
        .args(["/SILENT", "/SP-", "/NOCANCEL"])
        .spawn()
        .map_err(|e| format!("avvio dell'installatore non riuscito: {e}"))?;
    std::process::exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare() {
        assert!(parse_version("v0.2.0").unwrap() > parse_version("0.1.9").unwrap());
        assert!(parse_version("v1.0.0").unwrap() > semver::Version::parse(CURRENT).unwrap());
    }
}
