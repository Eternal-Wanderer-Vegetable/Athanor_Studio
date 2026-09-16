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

use std::path::{Path, PathBuf};

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

    /// Return the path bound to this session.
    pub fn doc_path(&self) -> &Path {
        self.app.doc_path()
    }

    /// Open the document and return its PM state and auxiliary layers.
    pub fn open(&self) -> Result<Value, ApiError> {
        self.app.open()
    }

    /// Save a PM document through the established seven-step pipeline.
    pub fn save(&self, body: &Value) -> Result<Value, ApiError> {
        self.app.save(body)
    }

    /// Verify the bound document using the engine verifier.
    pub fn verify(&self) -> Value {
        self.app.verify()
    }
}
