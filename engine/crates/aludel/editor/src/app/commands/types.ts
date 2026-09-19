// This file is part of Athanor, the Azodoc document engine.
// SPDX-License-Identifier: AGPL-3.0-only

//! 领域命令定义的共享类型：模块只返回定义（id/label/run/opts），
//! 由 main.ts 统一绑定 controller/shell/gateway 依赖并注册——
//! 不建立第二个 dispatcher。

import type {
  CommandSurface,
  EditorMode,
  FocusPolicy,
  MenuGroup,
  RibbonTab,
} from "../command-registry";
import type { CapabilityKey } from "../../platform/gateway";
import type { CommandContext } from "../command-registry";

export interface RegOpts {
  shortcut?: string;
  /** 兼容别名：等价于 caps: ["desktopFileDialogs"]。 */
  desktopOnly?: boolean;
  /** 需要的能力键：ctx.capabilities 任一为 false 则入口隐藏。 */
  caps?: CapabilityKey[];
  modes?: EditorMode[];
  needsDoc?: boolean;
  needsSelection?: boolean;
  needsTable?: boolean;
  needsJob?: boolean;
  active?: () => boolean;
  disabledReason?: string | ((c: CommandContext) => string);
  tab?: RibbonTab;
  group?: string;
  order?: number;
  groupOrder?: number;
  menu?: MenuGroup;
  menuOrder?: number;
  menuId?: string;
  elId?: string;
  surfaces?: CommandSurface[];
  keywords?: string[];
  focusPolicy?: FocusPolicy;
}

export interface RegDef {
  id: string;
  label: string;
  run: () => void | Promise<void>;
  opts?: RegOpts;
}
