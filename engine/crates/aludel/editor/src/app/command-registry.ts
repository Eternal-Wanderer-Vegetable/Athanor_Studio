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

import type { CapabilityKey, DocumentGateway, GatewayCapabilities } from "../platform/gateway";
import type { SelectionContext } from "./format";

export interface CommandContext {
  /** 当前是否有已挂载的文档。 */
  hasDocument: boolean;
  /** 平台能力（桌面/HTTP）兼容别名；等价于 capabilities.desktopFileDialogs。 */
  desktop: boolean;
  /** 网关能力矩阵（命令投影的唯一能力事实源）。 */
  capabilities: GatewayCapabilities;
  /** 编辑模式：无文档 / 编辑中 / 预览浮层中。 */
  mode: EditorMode;
  /** 视图模式（连续/页面/页宽）。 */
  viewMode: ViewMode;
  /** 文档脏标记。 */
  dirty: boolean;
  /** 有任务在途。 */
  busy: boolean;
  /** 存在非空文本选区。 */
  hasSelection: boolean;
  /** 光标/选区处于表格内（上下文 Table tab 与表格命令的启用依据）。 */
  inTable: boolean;
  /** 最近一次成功预览的真实页数（快照有效；null = 无分页数据）。 */
  pageCount: number | null;
}

/** 编辑器模式（命令投影的 mode 轴）。 */
export type EditorMode = "noDocument" | "editing" | "preview";

/** 视图模式（连续编辑 / 页面 / 页宽）。 */
export type ViewMode = "continuous" | "page" | "pageWidth";

/** 命令可投影到的 surface。 */
export type CommandSurface =
  | "menu"
  | "ribbon"
  | "palette"
  | "floating"
  | "statusbar"
  | "topbar";

/** ribbon 的固定页签（Word 习惯顺序）；table 为上下文页签（仅在表格内出现）。 */
export type RibbonTab =
  | "home"
  | "insert"
  | "layout"
  | "references"
  | "view"
  | "review"
  | "table";

/** 应用菜单顶层分组（Word 习惯：文件/编辑/视图/插入/格式/工具/帮助）。 */
export type MenuGroup =
  | "file"
  | "edit"
  | "view"
  | "insert"
  | "format"
  | "tools"
  | "help"
  /** 兼容别名：原“文档检查”顶层菜单的命令归入 tools。 */
  | "review";

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

  // ---- 能力/模式投影（与 visible 叠加，由 registry 统一判断） ----
  /** 需要的能力键：ctx.capabilities 中任一为 false 则入口隐藏（不渲染死按钮）。 */
  requiredCapabilities?: CapabilityKey[];
  /** 限定的编辑模式；缺省在所有模式投影。 */
  modes?: EditorMode[];
  /** surface 内优先级（溢出/排序提示，小者优先）；缺省 50。 */
  surfacePriority?: number;
  /** 完整菜单路径（如 "文件 > 保存"）；由 menu+label 自动派生，可显式覆盖。 */
  menuPath?: string;

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
  { id: "view", label: "视图" },
  { id: "insert", label: "插入" },
  { id: "format", label: "格式" },
  { id: "tools", label: "工具" },
  { id: "help", label: "帮助" },
];

/** 顶层菜单 id → 展示名（menuPath 派生用；含兼容别名 review）。 */
export const MENU_LABELS: Record<MenuGroup, string> = Object.fromEntries(
  MENU_GROUPS.map((g) => [g.id, g.label]).concat([["review", "工具"]]),
) as Record<MenuGroup, string>;

export const RIBBON_TABS: { id: RibbonTab; label: string; contextual: boolean }[] = [
  { id: "home", label: "开始", contextual: false },
  { id: "insert", label: "插入", contextual: false },
  { id: "layout", label: "布局", contextual: false },
  { id: "references", label: "引用", contextual: false },
  { id: "view", label: "视图", contextual: false },
  { id: "review", label: "审阅", contextual: false },
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
      .filter((c) => c.tab === tab && (c.surfaces?.includes("ribbon") ?? true) && this.isVisible(c, ctx))
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

  /** 命令的能力要求在当前上下文是否满足。 */
  private capsOk(cmd: Command, ctx: CommandContext): boolean {
    if (!cmd.requiredCapabilities?.length) return true;
    return cmd.requiredCapabilities.every((k) => ctx.capabilities[k]);
  }

  /** 命令的模式限定在当前上下文是否满足。 */
  private modeOk(cmd: Command, ctx: CommandContext): boolean {
    return !cmd.modes?.length || cmd.modes.includes(ctx.mode);
  }

  /** 统一可见性：自定义 visible + 能力 + 模式三者与的关系。 */
  isVisible(cmd: Command, ctx: CommandContext): boolean {
    return (cmd.visible?.(ctx) ?? true) && this.capsOk(cmd, ctx) && this.modeOk(cmd, ctx);
  }

  /** 投影：某菜单分组的命令（"review" 兼容别名归入 "tools"）。 */
  byMenu(menu: MenuGroup, ctx: CommandContext): Command[] {
    const target = menu === "review" ? "tools" : menu;
    return this.all()
      .filter((c) => {
        const group = c.menu === "review" ? "tools" : c.menu;
        return group === target && (c.surfaces?.includes("menu") ?? true) && this.isVisible(c, ctx);
      })
      .sort((a, b) => (a.menuOrder ?? 0) - (b.menuOrder ?? 0));
  }

  /** 投影：命令面板候选（全部可见命令，含不进 ribbon/menu 的）。 */
  paletteCandidates(ctx: CommandContext): Command[] {
    return this.all()
      .filter((c) => (c.surfaces?.includes("palette") ?? true) && this.isVisible(c, ctx))
      .sort((a, b) => a.id.localeCompare(b.id));
  }

  /** 投影：浮动工具栏命令。 */
  floating(ctx: CommandContext): Command[] {
    return this.all()
      .filter((c) => (c.surfaces?.includes("floating") ?? false) && this.isVisible(c, ctx));
  }

  /** 执行命令（disabled 或不可见时不执行——隐藏入口不能由快捷键绕过）。 */
  async run(id: string, ctx: CommandContext): Promise<void> {
    const cmd = this.commands.get(id);
    if (!cmd || !this.isVisible(cmd, ctx) || !cmd.enabled(ctx)) return;
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

// ---------------------------------------------------------------- 上下文构造

/** 构造 CommandContext 的纯输入（main.ts 的装配层把这些槽位接到真实状态）。 */
export interface CommandContextInput {
  gateway: Pick<DocumentGateway, "desktop" | "capabilities">;
  hasDocument: boolean;
  dirty: boolean;
  busy: boolean;
  /** 当前选区上下文（无视图时为 null）。 */
  sel: SelectionContext | null;
  viewMode: ViewMode;
  /** 预览浮层是否打开（mode = "preview"）。 */
  previewOpen: boolean;
  pageCount: number | null;
}

/** 集中计算命令上下文：mode/capability/selection/page 状态的唯一构造点。
 *  纯函数——surface 单测可直接喂输入验证投影结果。 */
export function buildCommandContext(input: CommandContextInput): CommandContext {
  const sel = input.sel;
  return {
    hasDocument: input.hasDocument,
    desktop: input.gateway.desktop,
    capabilities: input.gateway.capabilities,
    mode: input.previewOpen ? "preview" : input.hasDocument ? "editing" : "noDocument",
    viewMode: input.viewMode,
    dirty: input.dirty,
    busy: input.busy,
    hasSelection: sel !== null && sel.kind !== "empty",
    inTable: sel?.inTable ?? false,
    pageCount: input.pageCount,
  };
}
