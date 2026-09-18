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

//! 统一命令注册表：菜单、工具栏、快捷键共享同一 Command 定义——
//! run/enabled/active/label/shortcut 只有一份，避免 UI 与按键行为漂移。

export interface CommandContext {
  /** 当前是否有已挂载的文档。 */
  hasDocument: boolean;
  /** 平台能力（桌面/HTTP）。 */
  desktop: boolean;
  /** 文档脏标记。 */
  dirty: boolean;
  /** 有任务在途。 */
  busy: boolean;
}

export interface Command {
  id: string;
  label: string;
  /** 快捷键提示文本（如 "Ctrl+S"）；绑定由 keymap/keydown 统一做。 */
  shortcut?: string;
  /** 选区相关命令的激活态（按钮高亮）。 */
  active?: () => boolean;
  enabled: (ctx: CommandContext) => boolean;
  run: () => void | Promise<void>;
}

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
