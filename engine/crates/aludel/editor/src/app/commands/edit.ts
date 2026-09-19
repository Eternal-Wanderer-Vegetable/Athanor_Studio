// This file is part of Athanor, the Azodoc document engine.
// SPDX-License-Identifier: AGPL-3.0-only

//! 编辑域命令：撤销/重做、查找/替换。
//! （剪贴板命令在 Word 习惯中属于编辑菜单；原生 paste 经
//!   DocumentController 的 sanitize 路径，剪贴板 API 入口后续接入。）

import { undo, redo } from "prosemirror-history";
import type { CommandDeps } from "./deps";
import type { RegDef } from "./types";

export function editCommands(deps: CommandDeps): RegDef[] {
  const { ctl } = deps;
  const pmRun = (fn: (state: never, dispatch: never) => boolean): void => {
    const v = ctl.view;
    if (v) fn(v.state as never, v.dispatch as never);
  };
  return [
    { id: "edit.undo", label: "撤销", run: () => pmRun(undo as never), opts: {
      shortcut: "Ctrl+Z", tab: "home", group: "编辑", groupOrder: 50, elId: "tb-undo",
      menu: "edit", menuOrder: 0, keywords: ["undo", "chexiao"],
    } },
    { id: "edit.redo", label: "重做", run: () => pmRun(redo as never), opts: {
      shortcut: "Ctrl+Y", tab: "home", group: "编辑", groupOrder: 50, elId: "tb-redo",
      menu: "edit", menuOrder: 1, keywords: ["redo", "chongzuo"],
    } },
    { id: "edit.find", label: "查找", run: () => deps.toggleFindBar(true), opts: {
      shortcut: "Ctrl+F", tab: "home", group: "编辑", groupOrder: 50,
      menu: "edit", menuOrder: 10, focusPolicy: "keep", keywords: ["find", "chazhao"],
    } },
    { id: "edit.replace", label: "替换", run: () => deps.toggleFindBar(true, true), opts: {
      shortcut: "Ctrl+H", tab: "home", group: "编辑", groupOrder: 50,
      menu: "edit", menuOrder: 11, focusPolicy: "keep", keywords: ["replace", "tihuan"],
    } },
  ];
}
