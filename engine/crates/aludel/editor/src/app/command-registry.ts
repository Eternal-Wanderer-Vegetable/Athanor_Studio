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

//! 统一命令注册表：菜单、功能区、命令面板、快捷键、浮动工具栏、
//! 状态栏共享同一份 Command 元数据——run/enabled/active/label/shortcut
//! 只有一份，surface 只是投影，避免 UI 与按键行为漂移（见
//! docs/editor-gui-command-matrix.md）。

export interface CommandContext {
  /** 当前是否有已挂载的文档。 */
  hasDocument: boolean;
  /** 平台能力（桌面/HTTP）。 */
  desktop: boolean;
  /** 文档脏标记。 */
  dirty: boolean;
  /** 有任务在途。 */
  busy: boolean;
  /** 存在非空文本选区。 */
  hasSelection: boolean;
  /** 光标/选区处于表格内（上下文 Table tab 与表格命令的启用依据）。 */
  inTable: boolean;
}

/** 命令可投影到的 surface。 */
export type CommandSurface =
  | "menu"
  | "ribbon"
  | "palette"
  | "floating"
  | "statusbar"
  | "topbar";

/** ribbon 的固定页签；table 为上下文页签（仅在表格内出现）。 */
export type RibbonTab = "home" | "insert" | "view" | "review" | "table";

/** 应用菜单顶层分组。 */
export type MenuGroup = "file" | "edit" | "insert" | "view" | "review";

/** 命令执行后的焦点策略（统一由 surface 执行层兑现）。 */
export type FocusPolicy =
  /** 默认：即时命令执行完把输入焦点还给正文（保留选区）。 */
  | "editor"
  /** 保持当前焦点（picker/查找/对话框/预览等自身管理焦点的命令）。 */
  | "keep"
  /** 不做任何焦点移动（文件类命令自身管理）。 */
  | "none";

export interface Command {
  id: string;
  label: string;
  /** 快捷键提示文本（如 "Ctrl+S"）；绑定由 document keydown 统一做。 */
  shortcut?: string;
  /** 选区相关命令的激活态（按钮高亮/aria-pressed）。 */
  active?: () => boolean;
  enabled: (ctx: CommandContext) => boolean;
  /** 禁用原因（有值时禁用的控件在 title/面板中解释，而非静默灰掉）。 */
  disabledReason?: (ctx: CommandContext) => string | null;
  /** 动态可见性：能力不可用的命令不渲染入口（缺省恒可见）。 */
  visible?: (ctx: CommandContext) => boolean;
  run: () => void | Promise<void>;

  // ---- 投影元数据（surface 布局只读这些字段） ----
  /** 出现的 surfaces；缺省视为 ["menu", "ribbon", "palette"]。 */
  surfaces?: CommandSurface[];
  /** ribbon 页签；无 tab 的命令不进功能区。 */
  tab?: RibbonTab;
  /** tab 内分组名（决定分栏与溢出优先级，配合 groupOrder）。 */
  group?: string;
  /** 组内排序（小者靠前）。 */
  order?: number;
  /** 组排序（小者靠前；溢出时大者优先收进“更多”）。 */
  groupOrder?: number;
  /** 应用菜单顶层分组；无 menu 的命令不进菜单。 */
  menu?: MenuGroup;
  /** 菜单/面板内排序。 */
  menuOrder?: number;
  /** 命令面板搜索关键词（中文/英文别名）。 */
  keywords?: string[];
  /** 稳定 DOM id（回归测试与既有选择器依赖；缺省由 surface 生成）。 */
  elId?: string;
  /** 菜单项稳定 DOM id（如 m-preview；缺省不赋 id）。 */
  menuId?: string;
  /** 执行后焦点策略；缺省 "editor"。 */
  focusPolicy?: FocusPolicy;
}

export const MENU_GROUPS: { id: MenuGroup; label: string }[] = [
  { id: "file", label: "文件" },
  { id: "edit", label: "编辑" },
  { id: "insert", label: "插入" },
  { id: "view", label: "视图" },
  { id: "review", label: "文档检查" },
];

export const RIBBON_TABS: { id: RibbonTab; label: string; contextual: boolean }[] = [
  { id: "home", label: "开始", contextual: false },
  { id: "insert", label: "插入", contextual: false },
  { id: "view", label: "视图", contextual: false },
  { id: "review", label: "文档检查", contextual: false },
  { id: "table", label: "表格", contextual: true },
];

export class CommandRegistry {
  private commands = new Map<string, Command>();
  private listeners = new Set<() => void>();

  register(cmd: Command): void {
    this.commands.set(cmd.id, cmd);
  }

  get(id: string): Command | undefined {
    return this.commands.get(id);
  }

  all(): Command[] {
    return [...this.commands.values()];
  }

  /** 投影：某 ribbon tab 的命令，按 groupOrder/group/order 排序。 */
  byTab(tab: RibbonTab, ctx: CommandContext): Command[] {
    return this.all()
      .filter((c) => c.tab === tab && (c.surfaces?.includes("ribbon") ?? true) && (c.visible?.(ctx) ?? true))
      .sort((a, b) =>
        (a.groupOrder ?? 50) - (b.groupOrder ?? 50) ||
        (a.group ?? "").localeCompare(b.group ?? "") ||
        (a.order ?? 0) - (b.order ?? 0));
  }

  /** 投影：某 ribbon tab 的分组（保持组间声明顺序）。 */
  groupsFor(tab: RibbonTab, ctx: CommandContext): { name: string; order: number; commands: Command[] }[] {
    const map = new Map<string, { name: string; order: number; commands: Command[] }>();
    for (const c of this.byTab(tab, ctx)) {
      const name = c.group ?? "";
      let g = map.get(name);
      if (!g) {
        g = { name, order: c.groupOrder ?? 50, commands: [] };
        map.set(name, g);
      }
      g.commands.push(c);
    }
    return [...map.values()].sort((a, b) => a.order - b.order);
  }

  /** 投影：某菜单分组的命令。 */
  byMenu(menu: MenuGroup, ctx: CommandContext): Command[] {
    return this.all()
      .filter((c) => c.menu === menu && (c.surfaces?.includes("menu") ?? true) && (c.visible?.(ctx) ?? true))
      .sort((a, b) => (a.menuOrder ?? 0) - (b.menuOrder ?? 0));
  }

  /** 投影：命令面板候选（全部可见命令，含不进 ribbon/menu 的）。 */
  paletteCandidates(ctx: CommandContext): Command[] {
    return this.all()
      .filter((c) => (c.surfaces?.includes("palette") ?? true) && (c.visible?.(ctx) ?? true))
      .sort((a, b) => a.id.localeCompare(b.id));
  }

  /** 投影：浮动工具栏命令。 */
  floating(ctx: CommandContext): Command[] {
    return this.all()
      .filter((c) => (c.surfaces?.includes("floating") ?? false) && (c.visible?.(ctx) ?? true));
  }

  /** 执行命令（disabled 时不执行）。 */
  async run(id: string, ctx: CommandContext): Promise<void> {
    const cmd = this.commands.get(id);
    if (!cmd || !cmd.enabled(ctx)) return;
    await cmd.run();
    this.emit();
  }

  /** 状态变化（事务/会话事件）后通知 UI 刷新 enabled/active。 */
  onChange(listener: () => void): void {
    this.listeners.add(listener);
  }

  emit(): void {
    for (const l of this.listeners) l();
  }
}
