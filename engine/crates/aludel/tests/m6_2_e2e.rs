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
use aludel::session::DocumentSession;
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
        .save(&json!({
            "pm_doc": pm,
            "theme": {
                "schema_version": "1.0",
                "theme": "e2e",
                "defaults": { "page": { "size": "Letter landscape", "margin": "30mm 18mm 22mm 18mm" } }
            },
            "message": "M6.2 e2e",
            "author_id": "tester"
        }))
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
    let reopened = app.open().expect("保存后的主题应可重新打开");
    assert_eq!(reopened["theme"]["theme"], "e2e");

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
    assert_eq!(
        c.manifest_value()["layers"]["presentation"]["path"],
        "presentation/theme.json"
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

// ---------------------------------------------------------------- 资产（E1）

fn http_get_bytes(port: u16, path: &str) -> (u16, String, Vec<u8>) {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    let req = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    s.write_all(req.as_bytes()).unwrap();
    let mut buf = Vec::new();
    s.read_to_end(&mut buf).unwrap();
    let (head, body) = {
        let pos = buf
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .expect("响应必有头体分隔");
        (
            String::from_utf8_lossy(&buf[..pos]).to_string(),
            buf[pos + 4..].to_vec(),
        )
    };
    let status: u16 = head
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .expect("状态行");
    (status, head, body)
}

fn b64(data: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(data)
}

/// 含一个 image 节点的最小 PM 文档。
fn pm_with_image(asset: &str) -> Value {
    json!({
        "type": "doc", "attrs": {"schema_version": "1.0", "extra": null},
        "content": [{
            "type": "image",
            "attrs": {"id": "blk_00000000000000000000000000", "extra": null,
                      "asset": asset, "alt": "测试图"}
        }]
    })
}

#[test]
fn staged_asset_persists_to_registry_and_serves() {
    let (_dir, doc) = make_doc("staged");
    let port = start_server(doc.clone());

    // 暂存：mime 白名单拒绝 SVG
    let (status, _, _) = http_json(
        port,
        "POST",
        "/api/asset/stage",
        Some(&json!({"filename": "x.svg", "mime": "image/svg+xml", "data": b64(b"<svg/>")})),
    );
    assert_eq!(status, 400, "SVG 不在白名单应被拒绝");

    // 正常暂存
    let (status, staged, _) = http_json(
        port,
        "POST",
        "/api/asset/stage",
        Some(&json!({"filename": "..\\pic.png", "mime": "image/png", "data": b64(b"png-bytes")})),
    );
    assert_eq!(status, 200);
    let id = staged["id"].as_str().unwrap().to_string();
    let url = staged["url"].as_str().unwrap().to_string();
    assert!(url.starts_with("asset://as_"), "应返回 asset:// 引用");
    assert!(url.ends_with("/pic.png"), "文件名已消毒: {url}");

    // 暂存区即可渲染（保存前）
    let (status, head, body) = http_get_bytes(port, &format!("/api/asset/{id}"));
    assert_eq!(status, 200);
    assert!(head.contains("image/png"), "应为图片 mime: {head}");
    assert_eq!(body, b"png-bytes");

    // 保存：asset:// 引用 → registry + 二进制落库
    let (status, resp, _) = http_json(
        port,
        "POST",
        "/api/save",
        Some(&json!({ "pm_doc": pm_with_image(&url), "author_id": "asset-tester" })),
    );
    assert_eq!(status, 200, "保存失败: {resp}");
    assert_eq!(resp["assets"]["staged"].as_u64().unwrap(), 1);

    let mut c = read_container(&doc);
    let reg: Value = serde_json::from_slice(&c.read_entry("assets/registry.json").unwrap())
        .expect("registry 应存在");
    let entry = reg["assets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == id)
        .expect("资产已登记")
        .clone();
    assert_eq!(entry["storage"], "embedded");
    assert_eq!(entry["filename"], "pic.png");
    let path = entry["path"].as_str().unwrap().to_string();
    assert_eq!(c.read_entry(&path).unwrap(), b"png-bytes");
    assert_eq!(
        entry["sha256"].as_str().unwrap(),
        aludel::doc_store::sha256_hex(b"png-bytes")
    );
    assert_eq!(
        c.manifest_value()["layers"]["assets"]["path"],
        "assets/registry.json"
    );
    drop(c);

    // 保存后仍可读（现在来自 registry），且 verify 通过
    let (status, _, body) = http_get_bytes(port, &format!("/api/asset/{id}"));
    assert_eq!(status, 200);
    assert_eq!(body, b"png-bytes");
    assert_eq!(athanor_cli::verify_cmd::run(&doc), 0, "verify 必须通过");

    // 重开后正文是 asset:// 引用
    let opened = http_json(port, "GET", "/api/doc", None).1;
    let reopened = serde_json::to_string(&opened["pm_doc"]).unwrap();
    assert!(reopened.contains("asset://as_"), "正文应保留 asset:// 引用");
}

#[test]
fn save_migrates_data_uri_and_external_urls() {
    let (_dir, doc) = make_doc("migrate");
    let app = App::new(doc.clone());
    let mut pm = pm_with_image(&format!("data:image/png;base64,{}", b64(b"migrated")));
    let content = pm["content"].as_array_mut().unwrap();
    content.push(json!({
        "type": "image",
        "attrs": {"id": "blk_00000000000000000000000001", "extra": null,
                  "asset": "https://example.com/remote.png", "alt": "外链"}
    }));

    let resp = app.save(&json!({ "pm_doc": pm })).expect("保存应成功");
    assert_eq!(resp["assets"]["migrated"].as_u64().unwrap(), 1);
    assert_eq!(resp["assets"]["external"].as_u64().unwrap(), 1);

    let mut c = read_container(&doc);
    let reg: Value =
        serde_json::from_slice(&c.read_entry("assets/registry.json").unwrap()).unwrap();
    let assets = reg["assets"].as_array().unwrap();
    let emb = assets.iter().find(|e| e["storage"] == "embedded").unwrap();
    assert_eq!(
        c.read_entry(emb["path"].as_str().unwrap()).unwrap(),
        b"migrated"
    );
    let ext = assets
        .iter()
        .find(|e| e["url"] == "https://example.com/remote.png")
        .expect("外链资产已登记为 external");
    assert_eq!(ext["storage"], "external");
    drop(c);

    let content: Value = serde_json::from_slice(
        &read_container(&doc)
            .read_entry("document/content.json")
            .unwrap(),
    )
    .unwrap();
    let s = serde_json::to_string(&content).unwrap();
    assert!(!s.contains("data:"), "data: URI 应被迁移");
    assert!(!s.contains("https://example.com"), "外链应登记为 external");
    assert!(s.contains("asset://as_"));
    assert_eq!(athanor_cli::verify_cmd::run(&doc), 0, "verify 必须通过");
}

#[test]
fn dangling_and_bad_asset_refs_warn_but_save() {
    let (_dir, doc) = make_doc("dangling");
    let app = App::new(doc.clone());
    let ghost = azodoc_model::id::AzodocId::generate(azodoc_model::id::IdKind::As)
        .as_str()
        .to_string();
    let mut pm = pm_with_image(&format!("asset://{ghost}/x.png"));
    pm["content"].as_array_mut().unwrap().push(json!({
        "type": "image",
        "attrs": {"id": "blk_00000000000000000000000002", "extra": null,
                  "asset": "data:!!!not-base64!!!", "alt": "坏 data"}
    }));

    let resp = app
        .save(&json!({ "pm_doc": pm }))
        .expect("保存应成功（警告而非拒绝）");
    let warns = resp["assets"]["warnings"].as_array().unwrap();
    assert!(warns.len() >= 2, "悬空引用与坏 data: 都应报告: {warns:?}");

    // 原引用保留（不产 生悬空 asset:// 之外的改写）
    let content: Value = serde_json::from_slice(
        &read_container(&doc)
            .read_entry("document/content.json")
            .unwrap(),
    )
    .unwrap();
    let s = serde_json::to_string(&content).unwrap();
    assert!(s.contains(&ghost), "悬空 asset:// 原样保留");
    assert!(s.contains("not-base64"), "无法解码的 data: 原样保留");
}

#[test]
fn save_without_asset_refs_keeps_staged_unpersisted() {
    let (_dir, doc) = make_doc("unpersisted");
    let app = App::new(doc.clone());
    // 暂存了但正文没引用 → 保存不落库、不报错
    let staged = app
        .stage_asset("unused.png", "image/png", b"unused".to_vec())
        .unwrap();
    assert!(staged["url"].as_str().unwrap().starts_with("asset://"));
    let mut pm = app.open().unwrap()["pm_doc"].clone();
    edit_pm(&mut pm);
    let resp = app.save(&json!({ "pm_doc": pm })).expect("保存应成功");
    assert_eq!(resp["assets"]["staged"].as_u64().unwrap(), 0);
    assert_eq!(athanor_cli::verify_cmd::run(&doc), 0);
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
    assert!(html.contains("Athanor"));
}

#[test]
fn document_session_facade_reuses_core_operations() {
    let (_dir, doc) = make_doc("session");
    let session = DocumentSession::new(doc.clone());

    let opened = session.open().expect("session open 应复用核心实现");
    assert_eq!(opened["pm_doc"]["type"], "doc");
    assert_eq!(session.doc_path().as_deref(), Some(doc.as_path()));
    assert_eq!(session.verify().unwrap()["ok"], true);
}

/// E2: 表格编辑元数据（table_header 节点、colwidth、columns[]）经
/// 保存管线落 Prima 并在重开后完整恢复。
#[test]
fn table_roundtrip_through_save_reopen() {
    let (_dir, doc) = make_doc("table");
    let app = App::new(doc.clone());
    let resp = app
        .save(&json!({
            "pm_doc": {
                "type": "doc", "attrs": {"schema_version": "1.0", "extra": null},
                "content": [{
                    "type": "table", "attrs": {
                        "id": "blk_00000000000000000000000050", "header_row": true,
                        "columns": [
                            {"id": "col_00000000000000000000000051"},
                            {"id": "col_00000000000000000000000052"}
                        ],
                        "extra": null
                    },
                    "content": [
                        {"type": "table_row", "attrs": {"id": "row_00000000000000000000000053", "extra": null}, "content": [
                            {"type": "table_header", "attrs": {
                                "id": "cel_00000000000000000000000054", "column": 0,
                                "colspan": 1, "rowspan": 1, "colwidth": [300], "extra": null
                            }, "content": [
                                {"type": "paragraph", "attrs": {"id": "blk_00000000000000000000000055", "extra": null},
                                 "content": [{"type": "text", "text": "名"}]}
                            ]},
                            {"type": "table_header", "attrs": {
                                "id": "cel_00000000000000000000000056", "column": 1,
                                "colspan": 1, "rowspan": 1, "colwidth": [150], "extra": null
                            }, "content": [
                                {"type": "paragraph", "attrs": {"id": "blk_00000000000000000000000057", "extra": null},
                                 "content": [{"type": "text", "text": "值"}]}
                            ]}
                        ]},
                        {"type": "table_row", "attrs": {"id": "row_00000000000000000000000058", "extra": null}, "content": [
                            {"type": "table_cell", "attrs": {
                                "id": "cel_00000000000000000000000059", "column": 0,
                                "colspan": 1, "rowspan": 1, "colwidth": [300], "extra": null
                            }, "content": [
                                {"type": "paragraph", "attrs": {"id": "blk_00000000000000000000000060", "extra": null},
                                 "content": [{"type": "text", "text": "甲"}]}
                            ]},
                            {"type": "table_cell", "attrs": {
                                "id": "cel_00000000000000000000000061", "column": 1,
                                "colspan": 1, "rowspan": 1, "colwidth": [150], "extra": null
                            }, "content": [
                                {"type": "paragraph", "attrs": {"id": "blk_00000000000000000000000062", "extra": null},
                                 "content": [{"type": "text", "text": "乙"}]}
                            ]}
                        ]}
                    ]
                }]
            },
            "author_id": "table-tester"
        }))
        .expect("表格保存应成功");
    assert!(resp["revision"].as_str().unwrap().starts_with("rev_"));

    // 落盘 Prima：header cell role、columns[].width 归并、header_row
    let mut c = read_container(&doc);
    let content: Value =
        serde_json::from_slice(&c.read_entry("document/content.json").unwrap()).unwrap();
    drop(c);
    let table = &content["content"][0];
    assert_eq!(table["type"], "table");
    assert_eq!(table["header_row"], true);
    assert_eq!(table["columns"][0]["width"], 300, "colwidth 归并进 width");
    assert_eq!(table["columns"][1]["width"], 150);
    let row0 = &table["rows"][0]["cells"];
    assert_eq!(row0[0]["role"], "header", "table_header -> role:header");
    assert_eq!(row0[1]["role"], "header");
    assert!(table["rows"][1]["cells"][0].get("role").is_none());

    // 重开：PM 侧节点类型/列宽恢复
    let opened = app.open().expect("重开应成功");
    let pm_table = &opened["pm_doc"]["content"][0];
    let pm_head = &pm_table["content"][0]["content"][0];
    assert_eq!(
        pm_head["type"], "table_header",
        "role:header -> table_header"
    );
    assert_eq!(pm_head["attrs"]["colwidth"], json!([300]));
    let pm_body = &pm_table["content"][1]["content"][0];
    assert_eq!(pm_body["type"], "table_cell");

    assert_eq!(athanor_cli::verify_cmd::run(&doc), 0, "verify 必须通过");
}

/// E4: 手动分页符经保存管线往返；预览端点产出 Paged 增强 HTML + 快照指纹。
#[test]
fn page_break_and_preview_endpoints() {
    let (_dir, doc) = make_doc("pbreak");
    let app = App::new(doc.clone());
    let resp = app
        .save(&json!({
            "pm_doc": {
                "type": "doc", "attrs": {"schema_version": "1.0", "extra": null},
                "content": [
                    {"type": "paragraph", "attrs": {"id": "blk_00000000000000000000000080", "extra": null},
                     "content": [{"type": "text", "text": "前页"}]},
                    {"type": "page_break", "attrs": {"id": "blk_00000000000000000000000081", "extra": null}},
                    {"type": "paragraph", "attrs": {"id": "blk_00000000000000000000000082", "extra": null},
                     "content": [{"type": "text", "text": "后页"}]}
                ]
            },
            "author_id": "pbreak-tester"
        }))
        .expect("page_break 保存应成功");
    assert!(resp["revision"].as_str().unwrap().starts_with("rev_"));

    // 落盘 Prima：page_break 块类型
    let mut c = read_container(&doc);
    let content: Value =
        serde_json::from_slice(&c.read_entry("document/content.json").unwrap()).unwrap();
    drop(c);
    assert_eq!(content["content"][1]["type"], "page_break");

    // 重开：PM 侧恢复 page_break 节点
    let opened = app.open().expect("重开应成功");
    assert_eq!(opened["pm_doc"]["content"][1]["type"], "page_break");

    // 预览：Paged 增强 HTML（polyfill 内嵌）+ 块标记 + 快照指纹
    let pv = app
        .preview(&json!({"pm_doc": opened["pm_doc"]}))
        .expect("预览应成功");
    let html = pv["html"].as_str().unwrap();
    assert!(
        html.contains("__azodocPagedDone"),
        "预览 HTML 应含 Paged 完成回调"
    );
    assert!(html.contains("Paged.js"), "polyfill 应内嵌");
    assert!(html.contains("page-break"), "分页标记应在印刷 HTML 中");
    assert!(
        html.contains("data-block-id"),
        "预览应带块标记（LayoutIndex）"
    );
    let snap = &pv["snapshot"];
    assert_eq!(snap["content_hash"].as_str().unwrap().len(), 64);
    assert_eq!(snap["layout_hash"].as_str().unwrap().len(), 64);
    assert_eq!(snap["mode"], "pagedjs");
    assert!(
        pv["print_html"].as_str().unwrap().contains("前页"),
        "未分页回退 HTML 可用"
    );
    assert_eq!(athanor_cli::verify_cmd::run(&doc), 0, "verify 必须通过");
}

/// E5: 修订快照携带主题；checkout 端点还原主题并返回 theme_restored；
/// 无主题快照的修订如实报告（仅正文历史）。
#[test]
fn checkout_restores_theme_snapshot_and_reports_body_only() {
    let (_dir, doc) = make_doc("checkout");
    let app = App::new(doc.clone());
    let pm = || {
        json!({
            "type": "doc", "attrs": {"schema_version": "1.0", "extra": null},
            "content": [
                {"type": "paragraph", "attrs": {"id": "blk_00000000000000000000000090", "extra": null},
                 "content": [{"type": "text", "text": "v1"}]}
            ]
        })
    };
    // v1：主题 A4
    let r1 = app
        .save(&json!({
            "pm_doc": pm(),
            "theme": {"schema_version": "1.0", "theme": "default", "pageSize": "A4"},
            "author_id": "u"
        }))
        .expect("v1 保存应成功");
    let rev_v1 = r1["revision"].as_str().unwrap().to_string();

    // v2：内容 + 主题都变（Letter）
    let mut pm2 = pm();
    pm2["content"].as_array_mut().unwrap().push(json!({
        "type": "paragraph", "attrs": {"id": "blk_00000000000000000000000091", "extra": null},
        "content": [{"type": "text", "text": "v2"}]
    }));
    let r2 = app
        .save(&json!({
            "pm_doc": pm2,
            "theme": {"schema_version": "1.0", "theme": "default", "pageSize": "Letter"},
            "author_id": "u"
        }))
        .expect("v2 保存应成功");
    let rev_v2 = r2["revision"].as_str().unwrap().to_string();

    // open：两条编辑器修订带 theme_sha256；importer 初始修订无（仅正文历史）
    let opened = app.open().expect("open 应成功");
    let hist = opened["history"].as_array().unwrap();
    let find = |id: &str| hist.iter().find(|e| e["id"] == json!(id)).unwrap();
    assert!(
        find(&rev_v1)["theme_sha256"].is_string(),
        "v1 应携带主题快照"
    );
    assert!(
        find(&rev_v2)["theme_sha256"].is_string(),
        "v2 应携带主题快照"
    );
    let importer = hist
        .iter()
        .find(|e| e["author_type"] == "importer")
        .expect("应有 importer 初始修订");
    assert!(
        importer["theme_sha256"].is_null(),
        "导入时无主题层 → 仅正文历史"
    );
    assert_eq!(opened["theme"]["pageSize"], "Letter");

    // checkout 回 v1：内容还原 + 主题还原 + theme_restored=true
    let doc_after = app
        .checkout(&json!({"revision": rev_v1}))
        .expect("checkout 应成功");
    assert_eq!(doc_after["theme_restored"], true);
    assert_eq!(doc_after["theme"]["pageSize"], "A4");
    assert_eq!(
        doc_after["pm_doc"]["content"].as_array().unwrap().len(),
        1,
        "v2 的段落应消失"
    );
    assert_eq!(doc_after["revision"], rev_v1);
    assert_eq!(athanor_cli::verify_cmd::run(&doc), 0, "verify 必须通过");

    // 回到 v2 后手动构造"仅正文历史"：链中条目去掉 theme 成员（模拟旧文档）
    {
        let mut c = read_container(&doc);
        let mut chain: Value =
            serde_json::from_slice(&c.read_entry("revisions/chain.json").unwrap()).unwrap();
        let revs = chain["revisions"].as_array_mut().unwrap();
        let target = revs.iter_mut().find(|r| r["id"] == json!(rev_v2)).unwrap();
        target.as_object_mut().unwrap().remove("theme_path");
        target.as_object_mut().unwrap().remove("theme_sha256");
        let mut cb = serde_json::to_vec_pretty(&chain).unwrap();
        cb.push(b'\n');
        c.set_entry("revisions/chain.json", cb).unwrap();
        std::fs::write(&doc, c.write().unwrap()).unwrap();
    }
    // 旧形态修订：theme_restored=false、主题保持当前（不伪造）
    let doc_after2 = app
        .checkout(&json!({"revision": rev_v2}))
        .expect("checkout 旧形态应成功");
    assert_eq!(doc_after2["theme_restored"], false);
    assert_eq!(
        doc_after2["theme"]["pageSize"], "A4",
        "仅正文历史不回滚主题"
    );
    // open 侧警告：仅正文历史如实提示
    assert!(doc_after2["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|w| w.as_str().unwrap().contains("仅正文历史")));
}
