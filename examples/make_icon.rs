//! Erzeugt das App-Icon.
//! - macOS-Iconset: `cargo run --example make_icon -- <ordner.iconset>`
//! - Windows-Icon:  `cargo run --example make_icon -- --ico <datei.ico>`

#[path = "../src/icon.rs"]
mod icon;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--ico") {
        let path = args.get(1).cloned().unwrap_or_else(|| "assets/windows/kadra.ico".into());
        if let Some(dir) = std::path::Path::new(&path).parent() {
            std::fs::create_dir_all(dir).unwrap();
        }
        std::fs::write(&path, ico(&[16, 24, 32, 48, 64, 128, 256])).unwrap();
        println!("Icon: {path}");
        return;
    }
    let dir = args.first().cloned().unwrap_or_else(|| "target/Kadra.iconset".into());
    std::fs::create_dir_all(&dir).unwrap();
    for base in [16, 32, 128, 256, 512] {
        icon::render(base).save_png(format!("{dir}/icon_{base}x{base}.png")).unwrap();
        icon::render(base * 2).save_png(format!("{dir}/icon_{base}x{base}@2x.png")).unwrap();
    }
    println!("Iconset: {dir}");
}

/// ICO-Datei mit PNG-kodierten Einträgen (seit Windows Vista unterstützt).
fn ico(sizes: &[u32]) -> Vec<u8> {
    let pngs: Vec<Vec<u8>> = sizes.iter().map(|&s| icon::render_full(s).encode_png().unwrap()).collect();
    let mut out = vec![0, 0, 1, 0];
    out.extend((sizes.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * sizes.len() as u32;
    for (&s, png) in sizes.iter().zip(&pngs) {
        let dim = if s >= 256 { 0 } else { s as u8 }; // 0 = 256
        out.extend([dim, dim, 0, 0]);
        out.extend(1u16.to_le_bytes()); // Farbebenen
        out.extend(32u16.to_le_bytes()); // Bit pro Pixel
        out.extend((png.len() as u32).to_le_bytes());
        out.extend(offset.to_le_bytes());
        offset += png.len() as u32;
    }
    for png in pngs {
        out.extend(png);
    }
    out
}
