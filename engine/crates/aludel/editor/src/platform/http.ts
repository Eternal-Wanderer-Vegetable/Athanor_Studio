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

//! HTTP 平台实现（aludel 本地 server 原型）：保留既有 open/save/verify，
//! 桌面专有能力（对话框、草稿、最近文件、任务）显式禁用。

import type {
  DocumentGateway,
  JobRequest,
  JobSnapshot,
  OpenResult,
  PreviewResult,
  RecoveryDraft,
  SaveResult,
  StagedAssetRef,
} from "./gateway";

async function post(path: string, body: unknown): Promise<{ status: number; data: Record<string, unknown> }> {
  const r = await fetch(path, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body ?? {}),
  });
  return { status: r.status, data: (await r.json()) as Record<string, unknown> };
}

function unsupported(name: string): never {
  throw new Error(`当前为浏览器模式，${name} 不可用；请使用桌面版获得完整功能`);
}

export class HttpGateway implements DocumentGateway {
  readonly desktop = false;
  private sessionId = "__http__";

  async newDocument(): Promise<OpenResult> {
    unsupported("新建文档");
  }

  async openDocument(_path: string): Promise<OpenResult> {
    unsupported("按路径打开");
  }

  async openInitial(): Promise<OpenResult> {
    const resp = await fetch("/api/doc");
    const data = (await resp.json()) as Record<string, unknown>;
    if (!resp.ok) {
      throw new Error(String(data["error"] ?? `HTTP ${resp.status}`));
    }
    return { sessionId: this.sessionId, doc: data as unknown as OpenResult["doc"] };
  }

  async saveDocument(_sessionId: string, body: Record<string, unknown>): Promise<SaveResult> {
    const { status, data } = await post("/api/save", body);
    if (status !== 200) return { ok: false, error: String(data["error"] ?? status) };
    return data as unknown as SaveResult;
  }

  async stageAsset(
    _sessionId: string,
    file: { filename: string; mime: string; dataBase64: string },
  ): Promise<StagedAssetRef> {
    const { status, data } = await post("/api/asset/stage", {
      filename: file.filename,
      mime: file.mime,
      data: file.dataBase64,
    });
    if (status !== 200) throw new Error(String(data["error"] ?? `HTTP ${status}`));
    return { id: String(data["id"]), url: String(data["url"]) };
  }

  assetUrl(_sessionId: string, ref: string): string {
    // asset://<id>[/<name>] → 服务端资产端点；external 条目由服务端 302。
    if (ref.startsWith("asset://")) {
      const id = ref.slice("asset://".length).split("/")[0];
      return `/api/asset/${encodeURIComponent(id)}`;
    }
    return ref;
  }

  async saveDocumentAs(): Promise<SaveResult> {
    unsupported("另存为");
  }

  async closeDocument(): Promise<void> {
    unsupported("关闭文档");
  }

  async verifyDocument(): Promise<Record<string, unknown>> {
    return (await post("/api/verify", {})).data;
  }

  async renderPreview(
    _sessionId: string,
    body: Record<string, unknown>,
  ): Promise<PreviewResult> {
    const { status, data } = await post("/api/preview", body);
    if (status !== 200) throw new Error(String(data["error"] ?? `HTTP ${status}`));
    return data as unknown as PreviewResult;
  }

  async pickOpen(): Promise<string | null> {
    unsupported("文件选择器");
  }

  async pickSaveAs(): Promise<string | null> {
    unsupported("文件选择器");
  }

  async pickImportSource(): Promise<string | null> {
    unsupported("文件选择器");
  }

  async confirm(message: string): Promise<boolean> {
    return window.confirm(message);
  }

  async runJob(_r: JobRequest, _s: string | null, _u: (s: JobSnapshot) => void): Promise<number> {
    unsupported("后台任务");
  }

  async cancelJob(): Promise<boolean> {
    unsupported("后台任务");
  }

  async writeRecovery(): Promise<void> {
    // 浏览器原型无草稿区
  }

  async listRecoveries(): Promise<RecoveryDraft[]> {
    return [];
  }

  async readRecovery(): Promise<Record<string, unknown>> {
    unsupported("恢复草稿");
  }

  async deleteRecovery(): Promise<void> {}

  async listRecent(): Promise<string[]> {
    return [];
  }

  async removeRecent(): Promise<void> {}
}
