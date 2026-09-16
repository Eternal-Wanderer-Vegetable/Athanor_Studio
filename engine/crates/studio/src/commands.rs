// This file is part of Athanor, the Azodoc document engine.
// Copyright (C) 2026 The Athanor Studio Developers
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published
// by the Free Software Foundation, version 3 of the License only.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use aludel::session::DocumentSession;
use serde::Serialize;
use serde_json::Value;

#[derive(Default)]
pub struct StudioState {
    sessions: Mutex<HashMap<String, Arc<DocumentSession>>>,
}

#[derive(Debug, Serialize)]
pub struct CommandError {
    pub code: &'static str,
    pub message: String,
}

impl From<aludel::api::ApiError> for CommandError {
    fn from(error: aludel::api::ApiError) -> Self {
        let code = match error {
            aludel::api::ApiError::BadRequest(_) => "bad_request",
            aludel::api::ApiError::Doc(_) => "document_error",
        };
        Self {
            code,
            message: error.to_string(),
        }
    }
}

fn session_id(path: &std::path::Path) -> String {
    path.to_string_lossy().into_owned()
}

fn get_session(
    state: &tauri::State<'_, StudioState>,
    id: &str,
) -> Result<Arc<DocumentSession>, CommandError> {
    state
        .sessions
        .lock()
        .map_err(|_| CommandError {
            code: "state_poisoned",
            message: "studio session state is unavailable".into(),
        })?
        .get(id)
        .cloned()
        .ok_or_else(|| CommandError {
            code: "session_not_found",
            message: format!("no open document session for {id}"),
        })
}

#[tauri::command]
pub fn open_document(
    state: tauri::State<'_, StudioState>,
    path: String,
) -> Result<Value, CommandError> {
    let path = PathBuf::from(path);
    if !path.is_file() {
        return Err(CommandError {
            code: "document_not_found",
            message: format!("document does not exist: {}", path.display()),
        });
    }
    let id = session_id(&path);
    let session = Arc::new(DocumentSession::new(path));
    let opened = session.open().map_err(CommandError::from)?;
    state
        .sessions
        .lock()
        .map_err(|_| CommandError {
            code: "state_poisoned",
            message: "studio session state is unavailable".into(),
        })?
        .insert(id, session);
    Ok(opened)
}

#[tauri::command]
pub fn save_document(
    state: tauri::State<'_, StudioState>,
    session_id: String,
    body: Value,
) -> Result<Value, CommandError> {
    get_session(&state, &session_id)?
        .save(&body)
        .map_err(CommandError::from)
}

#[tauri::command]
pub fn verify_document(
    state: tauri::State<'_, StudioState>,
    session_id: String,
) -> Result<Value, CommandError> {
    Ok(get_session(&state, &session_id)?.verify())
}

#[tauri::command]
pub fn get_history(
    state: tauri::State<'_, StudioState>,
    session_id: String,
) -> Result<Value, CommandError> {
    let opened = get_session(&state, &session_id)?
        .open()
        .map_err(CommandError::from)?;
    Ok(opened
        .get("history")
        .cloned()
        .unwrap_or(Value::Array(Vec::new())))
}
