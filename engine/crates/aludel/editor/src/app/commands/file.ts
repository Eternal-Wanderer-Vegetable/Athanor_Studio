// This file is part of Athanor, the Azodoc document engine.
// SPDX-License-Identifier: AGPL-3.0-only

//! 文件/任务域命令：新建/打开/保存/另存/关闭 + 导入导出/出版任务。
//! 桌面能力由 caps 声明：HTTP 模式整条入口隐藏，不显示死按钮。

import type { CommandDeps } from "./deps";
import type { RegDef } from "./types";

export function fileCommands(deps: CommandDeps): RegDef[] {
  const { ctl } = deps;
  return [
    { id: "file.new", label: "新建", run: () => ctl.newDocument(), opts: {
      shortcut: "Ctrl+N", caps: ["desktopFileDialogs"], needsDoc: false,
      menu: "file", menuOrder: 0, menuId: "m-new", focusPolicy: "none",
      keywords: ["new", "xinjian"],
    } },
    { id: "file.open", label: "打开…", run: () => ctl.pickAndOpen(), opts: {
      shortcut: "Ctrl+O", caps: ["desktopFileDialogs"], needsDoc: false,
      menu: "file", menuOrder: 1, menuId: "m-open", focusPolicy: "none",
      keywords: ["open", "dakai"],
    } },
    { id: "file.save", label: "保存", run: () => ctl.requestSave(), opts: {
      shortcut: "Ctrl+S", menu: "file", menuOrder: 2, menuId: "m-save",
      keywords: ["save", "baocun"],
    } },
    { id: "file.saveAs", label: "另存为…", run: () => ctl.saveAs(), opts: {
      shortcut: "Ctrl+Shift+S", caps: ["desktopFileDialogs"],
      menu: "file", menuOrder: 3, menuId: "m-saveas", focusPolicy: "none",
      keywords: ["save as", "saveas", "lingcunwei"],
    } },
    { id: "file.close", label: "关闭", run: () => ctl.closeDocument(), opts: {
      caps: ["desktopFileDialogs"], menu: "file", menuOrder: 4, menuId: "m-close",
      focusPolicy: "none", keywords: ["close", "guanbi"],
    } },
    { id: "job.import", label: "导入…", run: () => ctl.runJob("import"), opts: {
      caps: ["desktopFileDialogs", "backgroundJobs"], needsDoc: false,
      menu: "file", menuOrder: 10, menuId: "m-import", focusPolicy: "keep",
      keywords: ["import", "daoru"],
    } },
    { id: "job.export", label: "导出 Markdown…", run: () => ctl.runJob("export"), opts: {
      caps: ["desktopFileDialogs", "backgroundJobs"],
      menu: "file", menuOrder: 11, menuId: "m-export", focusPolicy: "keep",
      keywords: ["export", "markdown", "daochu"],
    } },
    { id: "job.exportHtml", label: "导出 HTML…", run: () => ctl.runJob("export", "html"), opts: {
      caps: ["desktopFileDialogs", "backgroundJobs"],
      menu: "file", menuOrder: 12, menuId: "m-export-html", focusPolicy: "keep",
    } },
    { id: "job.exportText", label: "导出纯文本…", run: () => ctl.runJob("export", "text"), opts: {
      caps: ["desktopFileDialogs", "backgroundJobs"],
      menu: "file", menuOrder: 13, menuId: "m-export-text", focusPolicy: "keep",
    } },
    { id: "job.exportDocx", label: "导出 Word 文档…", run: () => ctl.runJob("export", "docx"), opts: {
      caps: ["desktopFileDialogs", "backgroundJobs"],
      menu: "file", menuOrder: 14, menuId: "m-export-docx", focusPolicy: "keep",
      keywords: ["docx", "word"],
    } },
    { id: "job.publish", label: "导出 PDF…", run: () => ctl.runJob("publish"), opts: {
      caps: ["desktopFileDialogs", "backgroundJobs"],
      menu: "file", menuOrder: 15, menuId: "m-publish", focusPolicy: "keep",
      keywords: ["pdf", "publish"],
    } },
    { id: "job.cancel", label: "取消任务", run: () => ctl.cancelJob(), opts: {
      caps: ["backgroundJobs"], needsJob: true, needsDoc: false,
      menu: "file", menuOrder: 16, menuId: "cancel-job", focusPolicy: "keep",
    } },
  ];
}
