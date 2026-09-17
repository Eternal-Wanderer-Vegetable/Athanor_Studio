// This file is part of Athanor, the Azodoc document engine.
// Copyright (C) 2026 The Athanor Studio Developers
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published
// by the Free Software Foundation, version 3 of the License only.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod jobs;

fn main() {
    tauri::Builder::default()
        .manage(commands::StudioState::default())
        .manage(jobs::JobManager::default())
        .invoke_handler(tauri::generate_handler![
            commands::open_document,
            commands::save_document,
            commands::verify_document,
            commands::get_history,
            jobs::run_job,
            jobs::get_job,
            jobs::cancel_job,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Athanor Studio");
}
