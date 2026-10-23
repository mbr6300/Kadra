//! Windows: Icon und Versionsinfo in die .exe einbetten (Explorer, Taskleiste, Eigenschaften).

fn main() {
    println!("cargo:rerun-if-changed=assets/windows/kadra.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon("assets/windows/kadra.ico")
        .set("ProductName", "Kadra")
        .set("FileDescription", "Kadra – Screenshot-Werkzeug")
        .set("CompanyName", "mbr6300")
        .set("LegalCopyright", "© 2026 mbr6300 · GNU GPL v3.0 oder neuer");
    if let Err(e) = res.compile() {
        println!("cargo:warning=Windows-Ressourcen nicht eingebettet: {e}");
    }
}
