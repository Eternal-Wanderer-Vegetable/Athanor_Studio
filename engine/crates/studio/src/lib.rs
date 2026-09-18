// Copyright (C) 2026 The Athanor Studio Developers
// SPDX-License-Identifier: AGPL-3.0-only

//! Athanor Studio 桌面壳（Tauri）。库形态供集成测试直接调用
//! `SessionRegistry` 与 recovery/recent 逻辑；二进制入口在 `main.rs`。

pub mod commands;
pub mod jobs;
pub mod recent;
pub mod recovery;
pub mod sessions;

/// `azodoc-asset://` 协议：编辑器把 `asset://<as_id>` 渲染为
/// `azodoc-asset://localhost/<session>/<id>`，这里解析 session + id 并
/// 返回资产字节（embedded）或 302 到原始 URL（external）。
/// 暂存资产在会话存活期可见；会话关闭后暂存释放，渲染自然落空。
fn asset_uri_response(
    ctx: tauri::UriSchemeContext<'_, tauri::Wry>,
    request: tauri::http::Request<Vec<u8>>,
) -> tauri::http::Response<Vec<u8>> {
    use tauri::Manager;
    fn err(status: u16, msg: &str) -> tauri::http::Response<Vec<u8>> {
        tauri::http::Response::builder()
            .status(status)
            .body(msg.as_bytes().to_vec())
            .unwrap_or_else(|_| tauri::http::Response::new(Vec::new()))
    }
    let mut segments = request.uri().path().trim_start_matches('/').split('/');
    let (Some(session), Some(id)) = (segments.next(), segments.next()) else {
        return err(400, "bad asset uri");
    };
    let session_id = percent_decode(session);
    let registry = ctx.app_handle().state::<sessions::SessionRegistry>();
    let session = match registry.session(&session_id) {
        Ok(s) => s,
        Err(_) => return err(404, "no such session"),
    };
    match session.read_asset(id) {
        Ok(aludel::assets::AssetRead::Embedded { mime, bytes }) => tauri::http::Response::builder()
            .status(200)
            .header("Content-Type", mime)
            .body(bytes)
            .unwrap_or_else(|_| tauri::http::Response::new(Vec::new())),
        Ok(aludel::assets::AssetRead::External { url }) => tauri::http::Response::builder()
            .status(302)
            .header("Location", url)
            .body(Vec::new())
            .unwrap_or_else(|_| tauri::http::Response::new(Vec::new())),
        Ok(aludel::assets::AssetRead::Missing) => err(404, "asset not found"),
        Err(e) => err(500, &e.to_string()),
    }
}

/// URI 段 percent-decode（session id 可能含路径字符）。
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(sessions::SessionRegistry::default())
        .manage(jobs::JobManager::default())
        .register_uri_scheme_protocol("azodoc-asset", asset_uri_response)
        .invoke_handler(tauri::generate_handler![
            commands::new_document,
            commands::open_document,
            commands::save_document,
            commands::save_document_as,
            commands::close_document,
            commands::preview_document,
            commands::checkout_revision,
            commands::stage_asset,
            commands::read_asset,
            commands::verify_document,
            commands::get_history,
            commands::write_recovery,
            commands::list_recoveries,
            commands::read_recovery,
            commands::delete_recovery,
            commands::list_recent,
            commands::remove_recent,
            jobs::run_job,
            jobs::get_job,
            jobs::cancel_job,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Athanor Studio");
}
