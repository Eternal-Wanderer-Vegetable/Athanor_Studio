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

//! 文档检查面板（修订历史/语义标注/损失/警告）：DocResponse 的派生
//! 视图，不持有也不写回文档状态；还原动作回调给 controller。

import type { DocResponse } from "../platform/gateway";
import { $ } from "./dom";

export function renderDocMeta(d: DocResponse): void {
  $("doc-path").textContent = d.path ?? "未命名文档";
  $("rev").textContent = d.revision ?? "（未保存过）";
}

export function renderReviewSections(
  d: DocResponse,
  onCheckout?: (revId: string) => void,
): void {
  renderDocMeta(d);

  const history = $("history");
  history.replaceChildren(
    ...(d.history ?? [])
      .slice()
      .reverse()
      .map((e) => {
        const li = document.createElement("li");
        const tag = document.createElement("span");
        tag.className = `tag tag--${e.author_type}`;
        tag.textContent = e.author_type;
        // §5.3：主题快照标记（无 = 仅正文历史）
        const themeMark = e.theme_sha256 ? " ◈" : "";
        li.append(
          tag,
          document.createTextNode(
            ` ${e.id} · ${e.message || "（无说明）"}${themeMark}${e.is_current ? " ⟵当前" : ""}`,
          ),
        );
        if (onCheckout && !e.is_current) {
          const btn = document.createElement("button");
          btn.type = "button";
          btn.className = "hist-restore";
          btn.dataset.rev = e.id;
          btn.title = e.theme_sha256
            ? "还原到此修订（含主题快照）"
            : "还原到此修订（该修订无主题快照，仅正文历史）";
          btn.textContent = "还原";
          btn.addEventListener("click", () => onCheckout(e.id));
          li.append(btn);
        }
        return li;
      }),
  );

  const anns = $("annotations");
  anns.replaceChildren(
    ...((d.annotations?.annotations ?? []) as Record<string, unknown>[]).map((a) => {
      const li = document.createElement("li");
      const detached = a["detached"] === true;
      if (detached) li.className = "detached";
      const target = (a["target"] ?? {}) as Record<string, unknown>;
      li.textContent = `${a["id"]} [${a["type"]}] @${target["block"] ?? "?"}${detached ? " · 失配 detached" : ""}`;
      return li;
    }),
  );

  const loss = $("loss");
  const lossItems: HTMLLIElement[] = (d.loss_summary?.unknown_blocks ?? []).map((u) => {
    const li = document.createElement("li");
    li.textContent = `unknown · ${u["summary"] ?? ""}（${u["loss_class"] ?? "?"}）`;
    return li;
  });
  if ((d.loss_summary?.detached_annotations ?? 0) > 0) {
    const li = document.createElement("li");
    li.className = "detached";
    li.textContent = `失配标注 ${d.loss_summary!.detached_annotations} 条`;
    lossItems.push(li);
  }
  if (lossItems.length === 0) {
    const li = document.createElement("li");
    li.textContent = "无";
    lossItems.push(li);
  }
  loss.replaceChildren(...lossItems);

  const warns = $("warnings");
  warns.replaceChildren(
    ...((d.warnings?.length ?? 0) > 0
      ? d.warnings.map((w) => {
          const li = document.createElement("li");
          li.textContent = w;
          return li;
        })
      : [Object.assign(document.createElement("li"), { textContent: "无" })]),
  );
}
