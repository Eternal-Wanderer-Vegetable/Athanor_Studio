fn main() {
    println!("cargo:rerun-if-changed=tauri.conf.json");
    // 应用图标以真实文件形式提交在 icons/（源图 app-icon.svg，用
    // `tauri icon` 生成全套尺寸）。tauri_build 会把 bundle.icon 里的
    // icon.ico 嵌入 Windows 资源，并把 PNG 用于各平台窗口图标。
    // Embed Tauri's Windows manifest (Common Controls v6) and platform
    // resources. Without this, TaskDialogIndirect fails at process load time.
    tauri_build::build();
}
