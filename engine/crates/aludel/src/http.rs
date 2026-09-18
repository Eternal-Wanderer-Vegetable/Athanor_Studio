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

//! 最小 HTTP/1.1 server（仅本机回环，M6.2 原型专用）。
//!
//! 取舍沿用 `azodoc-pdf/src/cdp.rs` 的先例：两条命令能自实现的就不引服务端
//! 框架。只支持一请求一连接（`Connection: close`）、`Content-Length` 请求体、
//! 无 TLS/压缩/分块传输——编辑器原型不需要更多。

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::time::Duration;

/// 请求体上限（PM 文档 JSON 全量上传；快照模式本就是全量保存）。
pub const MAX_BODY: usize = 64 * 1024 * 1024;
/// 请求头上限。
const MAX_HEADER: usize = 64 * 1024;

pub struct Request {
    pub method: String,
    pub path: String,
    pub body: Vec<u8>,
}

pub struct Response {
    pub status: u16,
    pub content_type: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    pub fn json(v: &serde_json::Value) -> Self {
        Response {
            status: 200,
            content_type: "application/json; charset=utf-8".to_string(),
            headers: Vec::new(),
            body: serde_json::to_vec(v).unwrap_or_else(|_| b"{}".to_vec()),
        }
    }

    pub fn json_with_status(status: u16, v: &serde_json::Value) -> Self {
        let mut r = Self::json(v);
        r.status = status;
        r
    }

    pub fn html(doc: &str) -> Self {
        Response {
            status: 200,
            content_type: "text/html; charset=utf-8".to_string(),
            headers: Vec::new(),
            body: doc.as_bytes().to_vec(),
        }
    }

    /// 任意 MIME 的二进制响应（资产字节）。
    pub fn bytes(mime: String, body: Vec<u8>) -> Self {
        Response {
            status: 200,
            content_type: mime,
            headers: Vec::new(),
            body,
        }
    }

    /// 302 重定向（external 资产 → 原始 URL）。
    pub fn redirect(location: &str) -> Self {
        Response {
            status: 302,
            content_type: "text/plain; charset=utf-8".to_string(),
            headers: vec![("Location".to_string(), location.to_string())],
            body: Vec::new(),
        }
    }
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        302 => "Found",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Payload Too Large",
        500 => "Internal Server Error",
        _ => "Unknown",
    }
}

pub type Handler = Arc<dyn Fn(&Request) -> Response + Send + Sync>;

/// 接受循环：每连接一线程，handler panic 转换为 500（不拖垮 server）。
pub fn serve(listener: TcpListener, handler: Handler) {
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let handler = Arc::clone(&handler);
        std::thread::spawn(move || {
            let _ = handle(stream, handler);
        });
    }
}

fn handle(stream: TcpStream, handler: Handler) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    let mut stream = stream;
    match read_request(&mut stream)? {
        Outcome::Closed => Ok(()),
        Outcome::Reject(status) => write_response(
            &mut stream,
            &Response::json_with_status(status, &serde_json::json!({ "error": reason(status) })),
        ),
        Outcome::Ready(req) => {
            let resp = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| handler(&req)))
                .unwrap_or_else(|_| {
                    Response::json_with_status(
                        500,
                        &serde_json::json!({ "error": "处理器内部错误（panic 已拦截）" }),
                    )
                });
            write_response(&mut stream, &resp)
        }
    }
}

enum Outcome {
    Closed,
    Reject(u16),
    Ready(Request),
}

fn read_request(stream: &mut TcpStream) -> std::io::Result<Outcome> {
    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 4096];
    let header_end = loop {
        let n = stream.read(&mut chunk)?;
        if n == 0 {
            return Ok(Outcome::Closed);
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(pos) = find(&buf, b"\r\n\r\n") {
            break pos;
        }
        if buf.len() > MAX_HEADER {
            return Ok(Outcome::Reject(413));
        }
    };

    let header = String::from_utf8_lossy(&buf[..header_end]).to_string();
    let mut lines = header.split("\r\n");
    let request_line = lines.next().unwrap_or("");
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_ascii_uppercase();
    let path = parts.next().unwrap_or("/").to_string();
    if method.is_empty() {
        return Ok(Outcome::Reject(400));
    }

    let mut content_length = 0usize;
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            if k.trim().eq_ignore_ascii_case("content-length") {
                content_length = v.trim().parse().unwrap_or(0);
            }
        }
    }
    if content_length > MAX_BODY {
        return Ok(Outcome::Reject(413));
    }

    let mut body = buf[header_end + 4..].to_vec();
    while body.len() < content_length {
        let n = stream.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..n]);
    }
    body.truncate(content_length);

    Ok(Outcome::Ready(Request { method, path, body }))
}

fn write_response(stream: &mut TcpStream, resp: &Response) -> std::io::Result<()> {
    let mut head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n",
        resp.status,
        reason(resp.status),
        resp.content_type,
        resp.body.len()
    );
    for (k, v) in &resp.headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes())?;
    stream.write_all(&resp.body)?;
    stream.flush()
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}
