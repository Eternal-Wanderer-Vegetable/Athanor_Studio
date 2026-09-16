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

//! M6.2 端到端：打开 → 编辑 → 保存 → `athanor verify` 通过；
//! 编辑会话产生 `author: human` 修订；标注随编辑自动重定位。

use aludel::api::{route, App};
use aludel::http::{self, Request};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn tmpdir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("athanor-m62-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn make_doc(tag: &str) -> (PathBuf, PathBuf) {
    let dir = tmpdir(tag);
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../corpus/markdown/basic.md");
    let doc = dir.join("d.azodoc");
    assert_eq!(
        athanor_cli::cmd_import(&src, &doc, None, "zh-CN", None, false),
        0,
        "语料导入失败"
    );
    (dir, doc)
}

fn read_container(p: &Path) -> azodoc_container::Container {
    let data = std::fs::read(p).unwrap();
    azodoc_container::open(data).unwrap().0
}

/// 找到第一个 paragraph（Prima），返回 (id, 纯文本)。
fn first_paragraph(content_file: &Value) -> (String, String) {
    fn walk(v: &Value) -> Option<(String, String)> {
        if let Some(arr) = v.as_array() {
            for item in arr {
                if let Some(hit) = walk(item) {
                    return Some(hit);
                }
            }
        } else if let Some(obj) = v.as_object() {
            if obj.get("type").and_then(Value::as_str) == Some("paragraph") {
                let id = obj.get("id").and_then(Value::as_str)?.to_string();
                let text = azodoc_convert::plain_text_of_block(v);
                return Some((id, text));
            }
            for key in ["content", "children", "items", "cells", "rows"] {
                if let Some(c) = obj.get(key) {
                    if let Some(hit) = walk(c) {
                        return Some(hit);
                    }
                }
            }
        }
        None
    }
    walk(content_file).expect("语料中必有 paragraph")
}

/// PM 树中的定位步：容器键名 → 数组下标。
enum Step {
    Key(&'static str),
    Idx(usize),
}

/// 找到第 `nth` 个 paragraph（0 起）的路径（先查后改，绕开递归可变借用）。
fn path_of_nth_paragraph(v: &Value, nth: usize) -> Option<Vec<Step>> {
    fn walk(v: &Value, nth: usize, counter: &mut usize, path: &mut Vec<Step>) -> bool {
        if v.get("type").and_then(Value::as_str) == Some("paragraph") {
            if *counter == nth {
                return true;
            }
            *counter += 1;
            return false; // paragraph 的 content 是行内节点，无需下钻
        }
        for key in ["content", "children", "items", "cells", "rows"] {
            let Some(arr) = v.get(key).and_then(Value::as_array) else {
                continue;
            };
            path.push(Step::Key(key));
            for (i, item) in arr.iter().enumerate() {
                path.push(Step::Idx(i));
                if walk(item, nth, counter, path) {
                    return true;
                }
                path.pop();
            }
            path.pop();
        }
        false
    }
    let mut path = Vec::new();
    let mut counter = 0usize;
    walk(v, nth, &mut counter, &mut path).then_some(path)
}

/// 沿路径取可变引用。
fn at_path_mut<'a>(v: &'a mut Value, path: &[Step]) -> Option<&'a mut Value> {
    let mut cur = v;
    for step in path {
        cur = match step {
            Step::Key(k) => cur.get_mut(*k)?,
            Step::Idx(i) => cur.get_mut(*i)?,
        };
    }
    Some(cur)
}

/// 找到第 `nth` 个 paragraph（0 起），返回其可变引用。
fn nth_paragraph_mut(pm: &mut Value, nth: usize) -> Option<&mut Value> {
    let path = path_of_nth_paragraph(pm, nth)?;
    at_path_mut(pm, &path)
}

/// 模拟一次编辑会话：改第 2 段文本 + 顶层新增一段（空 ID，触发补发）。
fn edit_pm(pm: &mut Value) {
    let para2 = nth_paragraph_mut(pm, 1).expect("第 2 个 paragraph 存在");
    let run0 = para2
        .as_object_mut()
        .unwrap()
        .get_mut("content")
        .and_then(Value::as_array_mut)
        .and_then(|a| a.first_mut())
        .expect("第 2 段有行内内容");
    assert_eq!(run0["type"], "text");
    run0["text"] = json!("编辑器改写：软件版本为 3.0。");

    let top = pm.as_object_mut().unwrap().get_mut("content").unwrap();
    top.as_array_mut().unwrap().push(json!({
        "type": "paragraph",
        "attrs": {"id": "", "extra": null},
        "content": [{"type": "text", "text": "M6.2 新增段落（ID 由引擎补发）"}]
    }));
}

#[test]
fn open_shows_pm_doc_and_layers() {
    let (_dir, doc) = make_doc("open");
    let app = App::new(doc.clone());

    // 先加一条标注，验证语义层出现在 open 响应中
    let data = std::fs::read(&doc).unwrap();
    let (mut c, _) = azodoc_container::open(data).unwrap();
    let content: Value =
        serde_json::from_slice(&c.read_entry("document/content.json").unwrap()).unwrap();
    drop(c);
    let (pid, text) = first_paragraph(&content);
    assert_eq!(
        athanor_cli::cmd_annotate(
            &doc,
            &athanor_cli::AnnotateArgs {
                block: &pid,
                ann_type: "Concept",
                value_json: r#"{"label": "冒烟"}"#,
                exact: Some(text.get(..6).unwrap_or(&text)),
                prefix: None,
                suffix: None,
                confidence: Some(0.9),
                author: Some("human:test"),
                source: None,
            }
        ),
        0
    );

    let opened = app.open().unwrap();
    assert_eq!(opened["pm_doc"]["type"], "doc");
    assert_eq!(
        opened["annotations"]["annotations"]
            .as_array()
            .unwrap()
            .len(),
        1,
        "语义标注应出现在 open 响应中"
    );
    let history = opened["history"].as_array().unwrap();
    assert_eq!(history.last().unwrap()["author_type"], "importer");
    assert!(opened["revision"].is_string());
}

#[test]
fn save_pipeline_lands_human_revision_and_passes_verify() {
    let (_dir, doc) = make_doc("save");
    // 标注第 1 段（text_quote 锚定）——编辑别处，重定位应报告 unchanged
    let data = std::fs::read(&doc).unwrap();
    let (mut c, _) = azodoc_container::open(data).unwrap();
    let content: Value =
        serde_json::from_slice(&c.read_entry("document/content.json").unwrap()).unwrap();
    drop(c);
    let (pid, text) = first_paragraph(&content);
    assert_eq!(
        athanor_cli::cmd_annotate(
            &doc,
            &athanor_cli::AnnotateArgs {
                block: &pid,
                ann_type: "Concept",
                value_json: r#"{"label": "锚点"}"#,
                exact: Some(text.get(..6).unwrap_or(&text)),
                prefix: None,
                suffix: None,
                confidence: None,
                author: None,
                source: None,
            }
        ),
        0
    );

    let app = App::new(doc.clone());
    let mut pm = app.open().unwrap()["pm_doc"].clone();
    edit_pm(&mut pm);

    let resp = app
        .save(&json!({ "pm_doc": pm, "message": "M6.2 e2e", "author_id": "tester" }))
        .expect("保存应成功");
    assert!(
        resp["revision"].as_str().unwrap().starts_with("rev_"),
        "必须产生修订号"
    );
    assert!(
        resp["ids_assigned"].as_u64().unwrap() >= 1,
        "新增段落必须补发 ID"
    );
    assert!(
        resp["relocate"]["unchanged"].as_u64().unwrap() >= 1,
        "未受影响的标注必须 unchanged"
    );
    assert_eq!(resp["relocate"]["detached"].as_u64().unwrap(), 0);

    // 容器核验：human 修订落链、manifest 推进
    let mut c = read_container(&doc);
    let h = c.history().unwrap();
    let last = h.last().unwrap();
    assert_eq!(
        last.author_type, "human",
        "A1 验收③：编辑会话产生 human 修订"
    );
    assert_eq!(last.author_id.as_deref(), Some("tester"));
    assert_eq!(last.message, "M6.2 e2e");
    assert_eq!(
        c.manifest_typed().current_revision.as_deref(),
        resp["revision"].as_str()
    );
    drop(c);

    // A1 验收②：保存后 athanor verify 通过
    assert_eq!(
        athanor_cli::verify_cmd::run(&doc),
        0,
        "保存后的容器必须通过 verify"
    );
}

#[test]
fn save_rejects_unknown_pm_node_and_keeps_file_intact() {
    let (_dir, doc) = make_doc("reject");
    let app = App::new(doc.clone());
    let err = app
        .save(&json!({
            "pm_doc": {"type": "doc", "attrs": {"schema_version": "1.0", "extra": null},
                        "content": [{"type": "warp_drive", "attrs": {}}]},
        }))
        .expect_err("未知 PM 节点必须被拒绝");
    assert!(
        err.to_string().contains("未知"),
        "错误信息应指明未知节点: {err}"
    );

    // 文件未被破坏，仍可打开
    assert!(app.open().is_ok(), "被拒绝的保存不得损伤原文件");
}

#[test]
fn save_rejects_dangling_footnote_ref() {
    let (_dir, doc) = make_doc("footnote");
    let app = App::new(doc.clone());
    let err = app
        .save(&json!({
            "pm_doc": {"type": "doc", "attrs": {"schema_version": "1.0", "extra": null}, "content": [
                {"type": "paragraph", "attrs": {"id": "blk_00000000000000000000000009", "extra": null},
                 "content": [{"type": "footnote_ref", "attrs": {"id": "blk_does_not_exist", "extra": null}}]}
            ]},
        }))
        .expect_err("悬空 footnote_ref 必须被规范校验拦截");
    assert!(
        err.to_string().contains("校验未通过"),
        "应被 validate 步骤拦截: {err}"
    );
}

// ---------------------------------------------------------------- HTTP 层

fn start_server(doc: PathBuf) -> u16 {
    let app = Arc::new(App::new(doc));
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        http::serve(listener, {
            let app = Arc::clone(&app);
            Arc::new(move |req| route(&app, req))
        });
    });
    port
}

fn http_json(port: u16, method: &str, path: &str, body: Option<&Value>) -> (u16, Value, String) {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    let body_str = body.map(|v| v.to_string()).unwrap_or_default();
    let req = format!(
        "{method} {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body_str}",
        body_str.len()
    );
    s.write_all(req.as_bytes()).unwrap();
    let mut buf = Vec::new();
    s.read_to_end(&mut buf).unwrap();
    let raw = String::from_utf8_lossy(&buf).to_string();
    let (head, body) = raw.split_once("\r\n\r\n").expect("响应必有头体分隔");
    let status: u16 = head
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .expect("状态行");
    (
        status,
        serde_json::from_str(body).unwrap_or(Value::Null),
        head.to_string(),
    )
}

#[test]
fn http_layer_routes_end_to_end() {
    let (_dir, doc) = make_doc("http");
    let port = start_server(doc.clone());

    // 静态冒烟页
    let (status, _, head) = http_json(port, "GET", "/", None);
    assert_eq!(status, 200);
    assert!(head.contains("text/html"));

    // open
    let (status, doc_resp, _) = http_json(port, "GET", "/api/doc", None);
    assert_eq!(status, 200);
    assert_eq!(doc_resp["pm_doc"]["type"], "doc");

    // 编辑 + 保存（走完整 HTTP 栈）
    let mut pm = doc_resp["pm_doc"].clone();
    edit_pm(&mut pm);
    let (status, save_resp, _) = http_json(
        port,
        "POST",
        "/api/save",
        Some(&json!({ "pm_doc": pm, "author_id": "http-tester" })),
    );
    assert_eq!(status, 200, "保存失败: {save_resp}");
    assert!(save_resp["revision"].as_str().unwrap().starts_with("rev_"));

    // 404 / 方法不允许
    let (status, _, _) = http_json(port, "GET", "/nope", None);
    assert_eq!(status, 404);

    // 保存结果真实落盘
    let mut c = read_container(&doc);
    let last = c.history().unwrap().pop().unwrap();
    assert_eq!(last.author_type, "human");
    assert_eq!(last.author_id.as_deref(), Some("http-tester"));
}

#[test]
fn request_body_over_limit_is_rejected() {
    let (_dir, doc) = make_doc("big");
    let port = start_server(doc);
    // 头部声明超过上限的 Content-Length，正文只发少量字节——
    // 真实客户端在收到 413 前不会灌完 64 MiB，这里等价地触发同一上限逻辑
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    let req = format!(
        "POST /api/save HTTP/1.1\r\nHost: x\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{{}}",
        http::MAX_BODY + 1
    );
    s.write_all(req.as_bytes()).unwrap();
    let mut buf = Vec::new();
    s.read_to_end(&mut buf).unwrap();
    let head = String::from_utf8_lossy(&buf);
    assert!(
        head.starts_with("HTTP/1.1 413"),
        "超限请求体应回 413: {head}"
    );
}

#[test]
fn route_handles_raw_request_struct() {
    // 直接测 route()：GET / 返回冒烟页
    let (_dir, doc) = make_doc("route");
    let app = App::new(doc);
    let req = Request {
        method: "GET".into(),
        path: "/".into(),
        body: Vec::new(),
    };
    let resp = route(&app, &req);
    assert_eq!(resp.status, 200);
    let html = String::from_utf8(resp.body).unwrap();
    assert!(html.contains("ALUDEL"));
}
