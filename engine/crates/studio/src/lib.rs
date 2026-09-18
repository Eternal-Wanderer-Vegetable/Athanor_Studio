// Copyright (C) 2026 The Athanor Studio Developers
// SPDX-License-Identifier: AGPL-3.0-only

//! Athanor Studio 桌面壳（Tauri）。库形态供集成测试直接调用
//! `SessionRegistry` 与 recovery/recent 逻辑；二进制入口在 `main.rs`。

pub mod commands;
pub mod jobs;
pub mod recent;
pub mod recovery;
pub mod sessions;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(sessions::SessionRegistry::default())
        .manage(jobs::JobManager::default())
        .invoke_handler(tauri::generate_handler![
            commands::new_document,
            commands::open_document,
            commands::save_document,
            commands::save_document_as,
            commands::close_document,
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
