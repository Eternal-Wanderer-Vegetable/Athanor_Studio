// This file is part of Athanor, the Azodoc document engine.
// Copyright (C) 2026 Athanor Studio
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

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: dump_ast <file.azodoc>");
    let data = std::fs::read(&path).unwrap();
    let mut c = azodoc_container::open(data).unwrap().0;
    let content: serde_json::Value =
        serde_json::from_slice(&c.read_entry("document/content.json").unwrap()).unwrap();
    let doc = azodoc_convert::ExportDoc {
        content,
        assets: vec![],
        preserved: vec![],
        title: c.manifest_typed().document.title.clone(),
        language: None,
        document_id: None,
    };
    let mut log = azodoc_convert::LossLog::new();
    let ast = azodoc_docx::ast_out::prima_to_ast(
        &doc,
        &std::env::temp_dir().join("ast-assets"),
        &mut log,
    )
    .unwrap();
    println!("{}", serde_json::to_string_pretty(&ast).unwrap());
}
