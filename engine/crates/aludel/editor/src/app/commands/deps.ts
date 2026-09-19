// This file is part of Athanor, the Azodoc document engine.
// SPDX-License-Identifier: AGPL-3.0-only

//! 领域命令模块共享的运行时依赖：由 main.ts 装配层构造一次。
//! 模块的 run() 只调用这些公共入口，不直接持有 controller/shell 实例。

import type { DocumentController } from "../document-controller";
import type { CharacterFormat, ParagraphFormat } from "../format";
import type { Ribbon } from "../../ui/ribbon";
import type { Shell } from "../../ui/shell";
import type { AppMenus } from "../../ui/menus";
import type { CommandPalette } from "../../ui/palette";
import type { OverlayController } from "../../ui/overlay";
import type { CommandRegistry } from "../command-registry";

export interface CommandDeps {
  ctl: DocumentController;
  shell: Shell;
  ribbon: Ribbon;
  menus: AppMenus;
  palette: CommandPalette;
  overlays: OverlayController;
  registry: CommandRegistry;
  /** 段落格式事务（setParagraphFormat 包装）。 */
  applyPara: (patch: ParagraphFormat) => void;
  /** 字符格式事务（setCharacterFormat 包装）。 */
  applyChar: (patch: CharacterFormat) => void;
  /** 当前缩放值（状态栏/缩放命令读写）。 */
  getZoom: () => number;
  setZoom: (z: number) => void;
  /** 打开打印预览（view.preview 命令实现）。 */
  openPreview: () => Promise<void>;
  /** 查找栏开关。 */
  toggleFindBar: (show: boolean, withReplace?: boolean) => void;
  /** picker 型命令（字体/字号/颜色/高亮）激活 ribbon 页签并聚焦控件。 */
  openPicker: (pickerId: string) => void;
  /** 生成 Azodoc 风格节点 id（Crockford ULID）。 */
  newId: (prefix: "blk" | "row" | "cel" | "col") => string;
}
