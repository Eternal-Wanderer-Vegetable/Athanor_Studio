fn main() {
    // Tauri needs PNG for Unix window icons and ICO for Windows resources.
    // Generate both placeholders before Tauri's build/context code runs;
    // release branding can replace them in a later packaging step.
    let dir = std::path::Path::new("icons");
    std::fs::create_dir_all(dir).expect("create studio icon directory");
    let png: &[u8] = &[
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6,
        0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 156, 99, 168, 8, 56, 241, 31,
        0, 5, 100, 2, 144, 223, 44, 157, 105, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];
    std::fs::write(dir.join("icon.png"), png).expect("write studio PNG icon");
    let mut ico = Vec::with_capacity(22 + png.len());
    ico.extend_from_slice(&[0, 0, 1, 0, 1, 0]);
    ico.extend_from_slice(&[1, 1, 0, 0, 1, 0, 32, 0]);
    ico.extend_from_slice(&(png.len() as u32).to_le_bytes());
    ico.extend_from_slice(&(22u32).to_le_bytes());
    ico.extend_from_slice(png);
    std::fs::write(dir.join("icon.ico"), ico).expect("write studio icon");
    println!("cargo:rerun-if-changed=tauri.conf.json");
    // Embed Tauri's Windows manifest (Common Controls v6) and platform
    // resources. Without this, TaskDialogIndirect fails at process load time.
    tauri_build::build();
}
