//! Su Windows mette nell'exe l'icona, i dati della versione (nome, autore, descrizione)
//! e il manifest: un exe "anonimo" senza questi dati viene bloccato più spesso dagli antivirus.

fn main() {
    println!("cargo:rerun-if-changed=assets/brismaster.ico");
    println!("cargo:rerun-if-changed=assets/brismaster.manifest");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon("assets/brismaster.ico")
        .set_manifest_file("assets/brismaster.manifest")
        .set("ProductName", "BrisMaster")
        .set("FileDescription", "BrisMaster - assistente per giochi di carte")
        .set("CompanyName", "Motthz")
        .set("LegalCopyright", "Copyright (c) 2026 Motthz - licenza MIT")
        .set("OriginalFilename", "BrisMaster.exe")
        .set("InternalName", "BrisMaster")
        .set("Comments", "https://github.com/motthz/brismaster");
    res.compile()
        .expect("risorse Windows (icona e versione) non compilate");
}
