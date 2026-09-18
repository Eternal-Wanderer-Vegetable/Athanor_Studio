// This file is part of Athanor, the Azodoc document engine.
// Copyright (C) 2026 The Athanor Studio Developers
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published
// by the Free Software Foundation, version 3 of the License only.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! HTTP-independent document session facade for future desktop frontends.
//!
//! The session owns the document identity and delegates the established
//! open/save/verify semantics to the Aludel application core. Keeping this
//! boundary separate from transport code lets Tauri commands reuse the same
//! seven-step save pipeline without embedding an HTTP server.
//!
//! A session bound to a file path uses that path as its identity key; an
//! unsaved draft uses a generated `doc_*` key and rebinding happens through
//! [`DocumentSession::save_as`], which returns the new key after success.

use std::path::PathBuf;

use azodoc_model::id::{AzodocId, IdKind};
use serde_json::Value;

use crate::api::{ApiError, App};

/// A single document session suitable for embedding in a desktop command
/// state. The underlying `App` retains its serialization and validation
/// guarantees.
pub struct DocumentSession {
    app: App,
}

impl DocumentSession {
    /// Bind a session to one document path.
    pub fn new(doc_path: PathBuf) -> Self {
        Self {
            app: App::new(doc_path),
        }
    }

    /// Create an unsaved draft session from container bytes.
    pub fn new_draft(container_bytes: Vec<u8>) -> Self {
        Self {
            app: App::new_draft(container_bytes),
        }
    }

    /// Stable identity for this session: the bound path, or a draft marker.
    /// For drafts this is generated per-session by the caller via
    /// [`DocumentSession::draft_key`].
    pub fn doc_path(&self) -> Option<PathBuf> {
        self.app.doc_path()
    }

    /// Generate a fresh draft session key (`doc_<ULID>`).
    pub fn draft_key() -> String {
        AzodocId::generate(IdKind::Doc).as_str().to_string()
    }

    /// Open the document and return its PM state and auxiliary layers.
    pub fn open(&self) -> Result<Value, ApiError> {
        self.app.open()
    }

    /// Save a PM document through the established seven-step pipeline.
    pub fn save(&self, body: &Value) -> Result<Value, ApiError> {
        self.app.save(body)
    }

    /// Stage an asset into session memory; the returned `asset://` reference
    /// is written into the PM document and persisted on the next save.
    pub fn stage_asset(
        &self,
        filename: &str,
        mime: &str,
        bytes: Vec<u8>,
    ) -> Result<Value, ApiError> {
        self.app.stage_asset(filename, mime, bytes)
    }

    /// Read an asset for rendering (staged first, then registry).
    pub fn read_asset(&self, id: &str) -> Result<crate::assets::AssetRead, ApiError> {
        self.app.read_asset(id)
    }

    /// Save-as: write the pipeline output to `target` and rebind the session.
    pub fn save_as(
        &self,
        target: PathBuf,
        body: &Value,
        overwrite: bool,
    ) -> Result<Value, ApiError> {
        self.app.save_at(target, body, overwrite)
    }

    /// Current content fingerprint (SHA-256 of the container bytes).
    pub fn fingerprint(&self) -> Result<String, ApiError> {
        self.app.fingerprint()
    }

    /// Render a print-preview snapshot for the supplied PM state (E4).
    /// Shares the publish pipeline's export/print-CSS/Paged augmentation but
    /// does not commit — preview consumes the same versioned rendering.
    pub fn preview(&self, body: &Value) -> Result<Value, ApiError> {
        self.app.preview(body)
    }

    /// Restore the session document to a revision snapshot (E5).
    /// Returns the re-opened document payload plus `theme_restored`.
    pub fn checkout(&self, body: &Value) -> Result<Value, ApiError> {
        self.app.checkout(body)
    }

    /// Verify the bound document using the engine verifier.
    pub fn verify(&self) -> Result<Value, ApiError> {
        self.app.verify()
    }

    /// Whether the session is bound to an on-disk path.
    pub fn is_file_backed(&self) -> bool {
        self.doc_path().is_some()
    }
}
