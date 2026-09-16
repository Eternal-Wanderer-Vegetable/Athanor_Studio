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

/** 跨 crate 引用的 JSON（azodoc-pm 生成的 schema spec）。结构以运行时为准。 */
declare module "*.json" {
  const value: {
    nodes: Record<string, Record<string, unknown>>;
    marks: Record<string, Record<string, unknown>>;
  };
  export default value;
}
