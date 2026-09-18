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

//! 平台抽象：UI 只面对 DocumentGateway；Tauri 与 HTTP 的能力差异显式声明，
//! 不支持的操作返回 capability 标记或明确报错。

export interface DocResponse {
  path: string | null;
  fingerprint: string | null;
  revision: string | null;
  draft?: boolean;
  pm_doc: Record<string, unknown>;
  theme?: Record<string, unknown>;
  annotations: { annotations?: Record<string, unknown>[] };
  history: {
    id: string;
    author_type: string;
    author_id: string | null;
    message: string;
    timestamp: string | null;
    is_current: boolean;
    is_head: boolean;
  }[];
  warnings: string[];
  loss_summary: {
    unknown_blocks: Record<string, unknown>[];
    detached_annotations: number;
  };
}

export interface OpenResult {
  sessionId: string;
  doc: DocResponse;
}

export interface SaveResult {
  ok: boolean;
  revision?: string;
  ids_assigned?: number;
  ids_deduplicated?: number;
  relocate?: { unchanged: number; reanchored: number; moved: number; detached: number } | null;
  warnings?: string[];
  fingerprint?: string;
  path?: string;
  draft?: boolean;
  saved_as?: boolean;
  session_id?: string;
  doc?: SaveResult;
  error?: string;
}

export interface JobSnapshot {
  id: number;
  phase: string;
  progress: number;
  session_id?: string | null;
  result?: { output?: string; document?: string; report?: { summary?: { loss?: Record<string, number> } } };
  error?: { code: string; message: string };
}

export interface JobRequest {
  kind: "import" | "export" | "publish";
  input?: string;
  output?: string;
  reader?: string;
  format?: string;
  noPaged?: boolean;
}

export interface RecoveryDraft {
  file: string;
  session_path: string | null;
  generation: number;
  saved_at: string | null;
}

export interface PreviewResult {
  /** Paged.js 增强后的印刷 HTML（polyfill 内嵌，可直接喂 iframe srcdoc）。 */
  html: string;
  /** 未增强版本：分页超时/失败时的未分页回退呈现。 */
  print_html: string;
  page_size: string;
  snapshot: {
    content_hash: string;
    theme_hash: string;
    layout_hash: string;
    mode: string;
    css_version: string;
  };
}

export interface StagedAssetRef {
  /** 服务端生成的 as_ id。 */
  id: string;
  /** 写入正文 asset 字段的引用：`asset://<id>/<filename>`。 */
  url: string;
}

export interface DocumentGateway {
  readonly desktop: boolean;

  newDocument(title?: string): Promise<OpenResult>;
  openDocument(path: string): Promise<OpenResult>;
  /** HTTP 模式直接返回当前文档（服务端单文档会话）。 */
  openInitial(): Promise<OpenResult>;
  saveDocument(sessionId: string, body: Record<string, unknown>): Promise<SaveResult>;
  /** 暂存资产（dataBase64 为文件字节）；返回写入正文的 asset:// 引用。 */
  stageAsset(
    sessionId: string,
    file: { filename: string; mime: string; dataBase64: string },
  ): Promise<StagedAssetRef>;
  /** `asset://<id>` → 本端可展示 URL（HTTP 端点 / Tauri 自定义协议）；
   * 非 asset:// 引用原样返回。 */
  assetUrl(sessionId: string, ref: string): string;
  saveDocumentAs(
    sessionId: string,
    target: string,
    overwrite: boolean,
    body: Record<string, unknown>,
  ): Promise<SaveResult>;
  closeDocument(sessionId: string): Promise<void>;
  verifyDocument(sessionId: string): Promise<Record<string, unknown>>;
  /** 印刷预览：对当前 PM 快照产出 Paged.js 增强 HTML + 快照指纹（不出版）。 */
  renderPreview(sessionId: string, body: Record<string, unknown>): Promise<PreviewResult>;

  // ---- 文件对话框（HTTP 模式抛 unsupported）----
  pickOpen(): Promise<string | null>;
  pickSaveAs(defaultName: string): Promise<string | null>;
  pickImportSource(): Promise<string | null>;
  confirm(message: string, title?: string): Promise<boolean>;

  // ---- 任务 ----
  runJob(request: JobRequest, sessionId: string | null, onUpdate: (s: JobSnapshot) => void): Promise<number>;
  cancelJob(jobId: number): Promise<boolean>;

  // ---- 恢复草稿（HTTP 模式为空实现）----
  writeRecovery(sessionKey: string, generation: number, pmDoc: unknown, sessionPath: string | null, theme?: unknown): Promise<void>;
  listRecoveries(): Promise<RecoveryDraft[]>;
  readRecovery(file: string): Promise<Record<string, unknown>>;
  deleteRecovery(key: string): Promise<void>;

  // ---- 最近文件 ----
  listRecent(): Promise<string[]>;
  removeRecent(path: string): Promise<void>;
}
