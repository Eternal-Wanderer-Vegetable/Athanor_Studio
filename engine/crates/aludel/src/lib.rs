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

//! aludel — Azodoc 文档编辑器原型（M6.2）：本地 server + 保存落链管线。
//!
//! 库形态供集成测试与（未来的 A3 Tauri 壳）直接调用；二进制入口在
//! `src/main.rs`。API 面即 A3 的 command 层候选：`App::open` / `App::save` /
//! `App::verify`。

pub mod api;
pub mod http;

/// 静态冒烟页（M6.3 将替换为 ProseMirror 正式前端；构建产物届时嵌入）。
pub const INDEX_HTML: &str = include_str!("../static/index.html");
