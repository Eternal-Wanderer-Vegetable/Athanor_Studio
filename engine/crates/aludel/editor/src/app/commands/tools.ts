// This file is part of Athanor, the Azodoc document engine.
// SPDX-License-Identifier: AGPL-3.0-only

//! 审阅/工具/帮助域命令：文档检查（原“文档检查”菜单并入工具/审阅）、
//! 快捷键一览。批注线程/修订追踪尚无模型能力——不放假入口。

import { el } from "../../ui/dom";
import type { CommandDeps } from "./deps";
import type { RegDef } from "./types";

export function toolsCommands(deps: CommandDeps): RegDef[] {
  const { ctl } = deps;
  return [
    { id: "file.verify", label: "检查文档", run: () => ctl.verify(), opts: {
      tab: "review", group: "校验", groupOrder: 10,
      menu: "tools", menuOrder: 0, menuId: "m-verify", focusPolicy: "keep",
      keywords: ["verify", "jiancha"],
    } },
    // 修订历史/标注/损失盘点在检查面板中派生渲染；还原入口在历史列表项上。
    // 批注线程/trackedChanges/fields/toc/referenceCitations：模型未就绪（planned）。
    { id: "help.shortcuts", label: "快捷键一览", run: () => {
      // 数据来自同一 registry 投影——面板只读元数据，不另存副本。
      const items = deps.registry.all()
        .filter((c) => c.shortcut)
        .sort((a, b) => a.id.localeCompare(b.id));
      const menu = el("div", "az-menu shortcuts-menu");
      menu.setAttribute("role", "menu");
      menu.appendChild(el("div", "group-title", "快捷键"));
      for (const c of items) {
        const row = el("div", "shortcut-row");
        row.appendChild(el("span", undefined, c.label));
        row.appendChild(el("span", "menu-shortcut", c.shortcut!));
        menu.appendChild(row);
      }
      deps.overlays.show(menu, { restoreFocus: null });
    }, opts: {
      menu: "help", menuOrder: 0, needsDoc: false, focusPolicy: "keep",
      keywords: ["help", "shortcut", "kuaisujian", "bangzhu"],
    } },
  ];
}
