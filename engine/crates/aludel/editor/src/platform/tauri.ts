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

//! Tauri 平台实现：invoke 命令 + 原生对话框。

import { Channel, invoke } from "@tauri-apps/api/core";
import { open as dialogOpen, save as dialogSave, confirm as dialogConfirm } from "@tauri-apps/plugin-dialog";
import type {
  DocResponse,
  DocumentGateway,
  JobRequest,
  JobSnapshot,
  OpenResult,
  PreviewResult,
  RecoveryDraft,
  SaveResult,
  StagedAssetRef,
} from "./gateway";

const DOC_FILTER = { name: "Azodoc 文档", extensions: ["azodoc"] };

export class TauriGateway implements DocumentGateway {
  readonly desktop = true;

  async newDocument(title?: string): Promise<OpenResult> {
    const r = await invoke<{ session_id: string; doc: OpenResult["doc"] }>("new_document", {
      title: title ?? null,
    });
    return { sessionId: r.session_id, doc: r.doc };
  }

  async openDocument(path: string): Promise<OpenResult> {
    const r = await invoke<{ session_id: string; doc: OpenResult["doc"] }>("open_document", { path });
    return { sessionId: r.session_id, doc: r.doc };
  }

  async openInitial(): Promise<OpenResult> {
    // 桌面启动没有预绑定文档：新建空白草稿进入可直接输入的初始态。
    return this.newDocument();
  }

  async saveDocument(sessionId: string, body: Record<string, unknown>): Promise<SaveResult> {
    return invoke<SaveResult>("save_document", { sessionId, body });
  }

  async saveDocumentAs(
    sessionId: string,
    target: string,
    overwrite: boolean,
    body: Record<string, unknown>,
  ): Promise<SaveResult> {
    return invoke<SaveResult>("save_document_as", { sessionId, target, overwrite, body });
  }

  async stageAsset(
    sessionId: string,
    file: { filename: string; mime: string; dataBase64: string },
  ): Promise<StagedAssetRef> {
    return invoke<StagedAssetRef>("stage_asset", {
      sessionId,
      filename: file.filename,
      mime: file.mime,
      data: file.dataBase64,
    });
  }

  assetUrl(sessionId: string, ref: string): string {
    // azodoc-asset://localhost/<session>/<id> → lib.rs 自定义协议处理器；
    // session id 可能含路径字符，必须 percent-encode。
    if (ref.startsWith("asset://")) {
      const id = ref.slice("asset://".length).split("/")[0];
      return `azodoc-asset://localhost/${encodeURIComponent(sessionId)}/${encodeURIComponent(id)}`;
    }
    return ref;
  }

  async closeDocument(sessionId: string): Promise<void> {
    await invoke("close_document", { sessionId });
  }

  async verifyDocument(sessionId: string): Promise<Record<string, unknown>> {
    return invoke<Record<string, unknown>>("verify_document", { sessionId });
  }

  async renderPreview(
    sessionId: string,
    body: Record<string, unknown>,
  ): Promise<PreviewResult> {
    return invoke<PreviewResult>("preview_document", { sessionId, body });
  }

  async checkoutRevision(
    sessionId: string,
    body: Record<string, unknown>,
  ): Promise<DocResponse> {
    return invoke<DocResponse>("checkout_revision", { sessionId, body });
  }

  async pickOpen(): Promise<string | null> {
    return dialogOpen({ filters: [DOC_FILTER], multiple: false });
  }

  async pickSaveAs(defaultName: string): Promise<string | null> {
    return dialogSave({ filters: [DOC_FILTER], defaultPath: defaultName });
  }

  async pickImportSource(): Promise<string | null> {
    return dialogOpen({ multiple: false });
  }

  async confirm(message: string, title?: string): Promise<boolean> {
    return dialogConfirm(message, { title: title ?? "Athanor Studio", kind: "warning" });
  }

  async runJob(
    request: JobRequest,
    sessionId: string | null,
    onUpdate: (s: JobSnapshot) => void,
  ): Promise<number> {
    const channel = new Channel<JobSnapshot>();
    channel.onmessage = onUpdate;
    return invoke<number>("run_job", { request, onUpdate: channel, sessionId });
  }

  async cancelJob(jobId: number): Promise<boolean> {
    return invoke<boolean>("cancel_job", { jobId });
  }

  async writeRecovery(
    sessionKey: string,
    generation: number,
    pmDoc: unknown,
    sessionPath: string | null,
    theme?: unknown,
  ): Promise<void> {
    await invoke("write_recovery", {
      sessionKey,
      generation,
      pmDoc,
      sessionPath,
      theme,
    });
  }

  async listRecoveries(): Promise<RecoveryDraft[]> {
    return invoke<RecoveryDraft[]>("list_recoveries");
  }

  async readRecovery(file: string): Promise<Record<string, unknown>> {
    return invoke<Record<string, unknown>>("read_recovery", { file });
  }

  async deleteRecovery(key: string): Promise<void> {
    await invoke("delete_recovery", { key });
  }

  async listRecent(): Promise<string[]> {
    return invoke<string[]>("list_recent");
  }

  async removeRecent(path: string): Promise<void> {
    await invoke("remove_recent", { path });
  }
}
