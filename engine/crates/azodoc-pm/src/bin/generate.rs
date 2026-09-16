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

//! 重新生成 ProseMirror Schema 工件到 `gen/`（黄金文件，入库管理）。
//!
//! ```text
//! cargo run -p azodoc-pm --bin generate
//! ```

use std::path::PathBuf;

fn main() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("gen");
    std::fs::create_dir_all(&dir).expect("创建 gen/ 目录失败");

    let files = [
        ("aludel-schema.json", azodoc_pm::gen::aludel_schema_json()),
        ("schema.mjs", azodoc_pm::gen::schema_mjs()),
    ];
    for (name, body) in &files {
        let path = dir.join(name);
        std::fs::write(&path, body).unwrap_or_else(|e| panic!("写入 {} 失败: {e}", path.display()));
        println!("已生成 {}", path.display());
    }
}
