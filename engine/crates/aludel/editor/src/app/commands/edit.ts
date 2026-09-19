// This file is part of Athanor, the Azodoc document engine.
// SPDX-License-Identifier: AGPL-3.0-only

//! 编辑域命令：撤销/重做、查找/替换、剪贴板。
//! 原生 Ctrl+V/C/X 仍走 handleDOMEvents + pendingPasteMode；这里的命令经
//! Clipboard API 走同一 sanitize/资产路径（菜单/按钮/托盘入口）。

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
    // ---- 剪贴板（同一 sanitize 通道；native 键位不劫持） ----
    { id: "edit.pasteKeep", label: "粘贴（保留格式）", run: () => { void ctl.pasteClipboard("keep"); }, opts: {
      menu: "edit", menuOrder: 2, needsDoc: true, keywords: ["paste", "zhantie"],
    } },
    { id: "edit.pastePlain", label: "粘贴为纯文本", run: () => { void ctl.pasteClipboard("plain"); }, opts: {
      menu: "edit", menuOrder: 3, needsDoc: true, keywords: ["paste plain", "chunwenben"],
    } },
    { id: "edit.pasteMatch", label: "粘贴（匹配格式）", run: () => { void ctl.pasteClipboard("match"); }, opts: {
      menu: "edit", menuOrder: 4, needsDoc: true, keywords: ["paste match", "pipeigeishi"],
    } },
    { id: "edit.copy", label: "复制", run: async () => {
      const v = ctl.view;
      if (!v || v.state.selection.empty) return;
      if (document.execCommand?.("copy")) return; // 原生路径：触发 copy 事件，浏览器接管
      await navigator.clipboard.writeText(ctl.selectionText());
    }, opts: {
      menu: "edit", menuOrder: 5, needsSelection: true, keywords: ["copy", "fuzhi"],
    } },
    { id: "edit.cut", label: "剪切", run: async () => {
      const v = ctl.view;
      if (!v || v.state.selection.empty) return;
      if (document.execCommand?.("cut")) return; // 原生路径：cut 事件由浏览器接管（写剪贴板 + 删选区）
      await navigator.clipboard.writeText(ctl.selectionText());
      ctl.deleteSelection();
    }, opts: {
      menu: "edit", menuOrder: 6, needsSelection: true, keywords: ["cut", "jianqie"],
    } },
  ];
}
