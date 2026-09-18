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

//! 会话状态机（纯逻辑，无 DOM/PM 依赖，供 Vitest 直测）。
//!
//! 关键不变量：
//! - dirty 只由正文等价比较驱动；selection/滚动/缩放不算修改；
//! - 每个打开的会话有单调递增 epoch：任何异步响应若 epoch 过期一律丢弃；
//! - 保存在途时新请求排队，完成后用最新快照再存一次（Ctrl+S 重入安全）。

export type SaveState = "idle" | "saving" | "saved" | "error";

export interface SessionState {
  /** 后端会话键（路径或 doc_*）。 */
  sessionId: string;
  /** 绑定的文件路径（未命名草稿为 null）。 */
  path: string | null;
  /** 显示名（文件名或“未命名文档”）。 */
  displayName: string;
  /** 打开时的修订号。 */
  revision: string | null;
  /** 保存基线指纹（容器字节 sha256）。 */
  fingerprint: string | null;
  /** 内容编辑代数（正文改变时递增）。 */
  editGeneration: number;
  /** 已完成保存确认的代数（用于“保存中继续输入”判定）。 */
  savedGeneration: number;
  /** 内容脏标记（doc 与保存基线不等价时由控制器置位）。 */
  dirty: boolean;
  /** 异步请求纪元：打开/切换/导入接管时递增，过期响应丢弃。 */
  epoch: number;
  saveState: SaveState;
  /** 在途保存期间又有保存请求。 */
  saveQueued: boolean;
  /** 当前在途保存所捕获的快照代数。 */
  inFlightGeneration: number | null;
  /** 活动任务 id。 */
  activeJob: number | null;
}

export function initialState(): SessionState {
  return {
    sessionId: "",
    path: null,
    displayName: "未命名文档",
    revision: null,
    fingerprint: null,
    editGeneration: 0,
    savedGeneration: 0,
    dirty: false,
    epoch: 0,
    saveState: "idle",
    saveQueued: false,
    inFlightGeneration: null,
    activeJob: null,
  };
}

export class SessionStore {
  state: SessionState = initialState();

  /** 文档打开/接管：重置全部会话字段并推进 epoch。 */
  open(params: {
    sessionId: string;
    path: string | null;
    revision: string | null;
    fingerprint: string | null;
    displayName?: string;
  }): number {
    const epoch = ++this.state.epoch;
    const name =
      params.displayName ??
      (params.path ? (params.path.split(/[\\/]/).pop() || params.path) : "未命名文档");
    this.state = {
      ...initialState(),
      epoch,
      sessionId: params.sessionId,
      path: params.path,
      displayName: name,
      revision: params.revision,
      fingerprint: params.fingerprint,
    };
    return epoch;
  }

  /** 内容级编辑（控制器确认 doc 结构变化后调用）。 */
  markEdited(): void {
    this.state.editGeneration += 1;
    this.state.dirty = true;
    if (this.state.saveState === "saved") this.state.saveState = "idle";
  }

  /** 控制器比较 doc 与基线后更新 dirty（撤销回保存点 → clean）。 */
  setDirty(dirty: boolean): void {
    this.state.dirty = dirty;
  }

  /** 请求保存：在途则排队并返回 false（不应发起新请求）。 */
  requestSave(): boolean {
    if (this.state.saveState === "saving") {
      this.state.saveQueued = true;
      return false;
    }
    this.state.saveState = "saving";
    this.state.saveQueued = false;
    this.state.inFlightGeneration = this.state.editGeneration;
    return true;
  }

  /** 保存成功：只确认到 inFlightGeneration；之后的输入保持 dirty 由控制器复核。 */
  saveSucceeded(fingerprint: string | null, revision: string | null, path?: string | null): void {
    const s = this.state;
    s.savedGeneration = s.inFlightGeneration ?? s.editGeneration;
    s.inFlightGeneration = null;
    s.fingerprint = fingerprint ?? s.fingerprint;
    s.revision = revision ?? s.revision;
    if (path !== undefined) {
      s.path = path;
      if (path) s.displayName = path.split(/[\\/]/).pop() || path;
    }
    s.saveState = s.saveQueued ? "idle" : "saved";
  }

  /** 保存失败：回到 idle（dirty 由控制器按 doc 等价性重新评估）。 */
  saveFailed(): void {
    this.state.saveState = "error";
    this.state.inFlightGeneration = null;
  }

  /** 保存完成后若存在排队请求则返回 true（应再存一轮最新快照）。 */
  consumeQueuedSave(): boolean {
    if (this.state.saveQueued && this.state.saveState !== "saving") {
      this.state.saveQueued = false;
      return true;
    }
    return false;
  }

  /** 当前 epoch（控制器捕获后发异步请求，响应回来比对）。 */
  currentEpoch(): number {
    return this.state.epoch;
  }

  /** 任务归属与状态。 */
  setActiveJob(id: number | null): void {
    this.state.activeJob = id;
  }
}
