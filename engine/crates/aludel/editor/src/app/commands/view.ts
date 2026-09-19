// This file is part of Athanor, the Azodoc document engine.
// SPDX-License-Identifier: AGPL-3.0-only

//! 布局/视图域命令：页面设置、预览、面板、主题、命令面板、缩放。
//! 布局页签承载页面设置（Word 习惯）；视图页签承载显示/面板/缩放/预览。

import { askPageSettings } from "../../ui/page-settings";
import type { CommandDeps } from "./deps";
import type { RegDef } from "./types";

export function layoutViewCommands(deps: CommandDeps): RegDef[] {
  const { ctl, shell } = deps;
  return [
    // ---- 页面设置（布局页签 + 视图菜单可达） ----
    { id: "layout.pageSettings", label: "页面设置…", run: () => {
      void askPageSettings(ctl.currentPageTheme).then((theme) => {
        if (theme) ctl.setPageTheme(theme);
      });
    }, opts: {
      tab: "layout", group: "页面设置", groupOrder: 10, elId: "m-page-settings",
      menu: "view", menuOrder: 20, focusPolicy: "keep", keywords: ["page", "yemian"],
    } },
    // ---- 预览 ----
    { id: "view.preview", label: "打印预览", run: () => void deps.openPreview(), opts: {
      shortcut: "Ctrl+P", caps: ["previewPaged"],
      tab: "view", group: "预览", groupOrder: 10, elId: "m-preview",
      menu: "view", menuOrder: 10, menuId: "m-preview-menu", focusPolicy: "keep",
      keywords: ["preview", "print", "yulan", "dayin"],
    } },
    // ---- 面板 ----
    { id: "view.toggleOutline", label: "大纲面板", run: () => shell.toggleOutline(), opts: {
      tab: "view", group: "面板", groupOrder: 30, needsDoc: false,
      menu: "view", menuOrder: 30, active: () => shell.leftOpen,
    } },
    { id: "view.toggleInspector", label: "检查面板", run: () => shell.toggleInspector(), opts: {
      tab: "view", group: "面板", groupOrder: 30, needsDoc: false,
      menu: "view", menuOrder: 31, active: () => shell.rightOpen,
    } },
    { id: "view.ribbonCollapse", label: "折叠功能区", run: () => deps.ribbon.toggleCollapse(), opts: {
      tab: "view", group: "面板", groupOrder: 30, needsDoc: false,
      menu: "view", menuOrder: 32,
    } },
    // ---- 外观 ----
    { id: "view.theme", label: "深色主题", run: () => shell.toggleTheme(), opts: {
      tab: "view", group: "外观", groupOrder: 40, needsDoc: false,
      menu: "view", menuOrder: 40, active: () => shell.theme === "dark",
      keywords: ["dark", "theme", "shense"],
    } },
    // ---- 工具 ----
    { id: "view.palette", label: "命令面板", run: () => deps.palette.open(), opts: {
      shortcut: "Ctrl+K", tab: "view", group: "工具", groupOrder: 50, needsDoc: false,
      menu: "tools", menuOrder: 30, focusPolicy: "keep",
      keywords: ["command", "palette", "mingling"],
    } },
    // ---- 缩放 ----
    { id: "view.zoomIn", label: "放大", run: () => deps.setZoom(deps.getZoom() + 0.1), opts: {
      tab: "view", group: "缩放", groupOrder: 60, needsDoc: false,
      menu: "view", menuOrder: 60, elId: "view-zoom-in",
    } },
    { id: "view.zoomOut", label: "缩小", run: () => deps.setZoom(deps.getZoom() - 0.1), opts: {
      tab: "view", group: "缩放", groupOrder: 60, needsDoc: false,
      menu: "view", menuOrder: 61, elId: "view-zoom-out",
    } },
    { id: "view.zoomReset", label: "100%", run: () => deps.setZoom(1), opts: {
      tab: "view", group: "缩放", groupOrder: 60, needsDoc: false,
      menu: "view", menuOrder: 62, elId: "view-zoom-reset",
    } },
  ];
}
