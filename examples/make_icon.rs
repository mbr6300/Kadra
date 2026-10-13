//! Erzeugt das App-Icon (macOS-Iconset) – Aufruf: `cargo run --example make_icon -- <ordner.iconset>`

#[path = "../src/icon.rs"]
mod icon;

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| "target/SnapIt.iconset".into());
    std::fs::create_dir_all(&dir).unwrap();
    for base in [16, 32, 128, 256, 512] {
        icon::render(base).save_png(format!("{dir}/icon_{base}x{base}.png")).unwrap();
        icon::render(base * 2).save_png(format!("{dir}/icon_{base}x{base}@2x.png")).unwrap();
    }
    println!("Iconset: {dir}");
}
