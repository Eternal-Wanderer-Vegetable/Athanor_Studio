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

/** E7 功能开关：发布收口的三条旗标。
 *
 *  契约：旗标全部默认开启；关闭只影响交互入口/编辑辅助，不改变容器
 *  读写语义——旧包始终安全读取，任何自动迁移只在用户保存时发生且可
 *  回滚（标准 undo 栈）。测试/回归可用 URL 参数覆盖：
 *      ?flags=assetRegistryV2:0,tableSelectionV2:0,printPreviewV1:0
 */
export const FEATURE_FLAGS = ["assetRegistryV2", "tableSelectionV2", "printPreviewV1"] as const;

export type FeatureFlag = (typeof FEATURE_FLAGS)[number];

function readOverrides(): Partial<Record<FeatureFlag, boolean>> {
  const out: Partial<Record<FeatureFlag, boolean>> = {};
  try {
    const raw = new URLSearchParams(window.location.search).get("flags");
    if (!raw) return out;
    for (const pair of raw.split(",")) {
      const [name, value] = pair.split(":");
      if ((FEATURE_FLAGS as readonly string[]).includes(name)) {
        out[name as FeatureFlag] = value !== "0" && value !== "false";
      }
    }
  } catch {
    // 无 window（vitest 静态导入）：全部默认
  }
  return out;
}

const overrides = readOverrides();

/** 旗标当前值；未覆盖时默认 true。 */
export function flag(name: FeatureFlag): boolean {
  return overrides[name] ?? true;
}
