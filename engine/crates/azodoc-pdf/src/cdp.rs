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

//! 最小 CDP（Chrome DevTools Protocol）客户端。
//!
//! 仅覆盖出版所需命令：启动浏览器（`--remote-debugging-port=0`）从 stderr
//! 捕获 `ws://` 调试地址 → WebSocket 连接（浏览器级会话）→
//! `Target.createTarget` / `Target.attachToTarget`(flatten) → `Page.navigate`
//! → `Runtime.evaluate` 轮询 → `Page.printToPDF`。同步阻塞实现
//! （tungstenite，无 TLS——只连 127.0.0.1）。

use serde_json::{json, Value};
use std::io::BufRead;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use tungstenite::{Message, WebSocket};

use crate::{Browser, PdfError};

/// 轮询间隔（evaluate 轮询与 WS 读超时粒度）。
const POLL_INTERVAL: Duration = Duration::from_millis(100);

// ---------------------------------------------------------------- 纯函数（单测覆盖）

/// 从 stderr 行解析 DevTools WebSocket 地址（`DevTools listening on ws://…`）。
pub(crate) fn parse_ws_url(line: &str) -> Option<String> {
    let i = line.find("ws://")?;
    let rest = &line[i..];
    let end = rest
        .find(|c: char| c.is_whitespace() || c == '"' || c == '\'')
        .unwrap_or(rest.len());
    Some(rest[..end].to_string())
}

/// 本地路径 → `file://` URL（零依赖，仅转义 URL 保留字符）。
pub(crate) fn url_from_path(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let text = text.trim_start_matches('/');
    let mut s = String::with_capacity(text.len() + 8);
    s.push_str("file:///");
    for c in text.chars() {
        match c {
            ' ' => s.push_str("%20"),
            '#' => s.push_str("%23"),
            '%' => s.push_str("%25"),
            '?' => s.push_str("%3F"),
            _ => s.push(c),
        }
    }
    s
}

// ---------------------------------------------------------------- 浏览器进程

/// 浏览器子进程 + 临时用户数据目录（Drop 时统一清理）。
pub(crate) struct BrowserProcess {
    child: Child,
    ws_url_rx: Receiver<String>,
    #[allow(dead_code)]
    user_data: PathBuf,
}

impl Drop for BrowserProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.user_data);
    }
}

impl BrowserProcess {
    /// 等待浏览器退出（已发 Browser.close 时）。
    pub(crate) fn wait(&mut self) -> Result<(), PdfError> {
        self.child.wait().map(|_| ()).map_err(PdfError::Io)
    }
}

/// 以 CDP 调试模式启动浏览器（`--remote-debugging-port=0` 随机端口）。
pub(crate) fn launch_cdp(browser: &Browser) -> Result<BrowserProcess, PdfError> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let user_data =
        std::env::temp_dir().join(format!("athanor-cdp-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&user_data)?;

    let mut child = Command::new(&browser.path)
        .args([
            "--headless",
            "--disable-gpu",
            "--no-first-run",
            "--disable-extensions",
            "--remote-allow-origins=*",
            "--remote-debugging-port=0",
        ])
        .arg(format!("--user-data-dir={}", user_data.display()))
        .arg("about:blank")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(PdfError::Io)?;

    let stderr = child.stderr.take().expect("stderr 已管道化");
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in std::io::BufReader::new(stderr)
            .lines()
            .map_while(Result::ok)
        {
            if let Some(ws) = parse_ws_url(&line) {
                let _ = tx.send(ws);
                break;
            }
        }
    });

    Ok(BrowserProcess {
        child,
        ws_url_rx: rx,
        user_data,
    })
}

/// 等待 stderr 出现 DevTools 调试地址。
pub(crate) fn wait_ws_url(proc: &BrowserProcess, deadline: Instant) -> Result<String, PdfError> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    match proc.ws_url_rx.recv_timeout(remaining) {
        Ok(url) => Ok(url),
        Err(_) => Err(PdfError::Cdp(
            "未捕获 DevTools 调试地址（浏览器可能启动失败或版本过旧）".to_string(),
        )),
    }
}

// ---------------------------------------------------------------- CDP 会话

/// 浏览器级 CDP 会话（flatten 模式下页面命令以 sessionId 路由）。
pub(crate) struct Cdp {
    ws: WebSocket<tungstenite::stream::MaybeTlsStream<TcpStream>>,
    next_id: u64,
    /// 顺带收到的事件/无关响应，按序缓存
    inbox: Vec<Value>,
}

impl Cdp {
    pub(crate) fn connect(ws_url: &str) -> Result<Self, PdfError> {
        let (ws, _resp) = tungstenite::connect(ws_url)
            .map_err(|e| PdfError::Cdp(format!("WebSocket 连接失败: {e}")))?;
        // 读超时兜底：让阻塞读变成 200ms 轮询，超时判定才可能生效
        if let tungstenite::stream::MaybeTlsStream::Plain(s) = ws.get_ref() {
            let _ = s.set_read_timeout(Some(POLL_INTERVAL));
        }
        Ok(Self {
            ws,
            next_id: 0,
            inbox: Vec::new(),
        })
    }

    /// 发命令并等对应响应（期间收到的事件进 inbox）。
    pub(crate) fn call(
        &mut self,
        method: &str,
        params: Value,
        session: Option<&str>,
    ) -> Result<Value, PdfError> {
        self.next_id += 1;
        let id = self.next_id;
        let mut msg = json!({"id": id, "method": method, "params": params});
        if let Some(s) = session {
            msg["sessionId"] = json!(s);
        }
        self.ws
            .send(Message::text(msg.to_string()))
            .map_err(|e| PdfError::Cdp(format!("发送 {method} 失败: {e}")))?;
        loop {
            let v = self.recv_msg(None)?;
            if v.get("id").and_then(Value::as_u64) == Some(id) {
                if let Some(err) = v.get("error") {
                    return Err(PdfError::Cdp(format!("{method} 失败: {err}")));
                }
                return Ok(v.get("result").cloned().unwrap_or(Value::Null));
            }
            self.inbox.push(v);
        }
    }

    /// 等待指定事件（先查 inbox，再阻塞收包）。
    pub(crate) fn wait_event(
        &mut self,
        method: &str,
        session: &str,
        deadline: Instant,
    ) -> Result<Value, PdfError> {
        let matches = |v: &Value| {
            v.get("method").and_then(Value::as_str) == Some(method)
                && v.get("sessionId").and_then(Value::as_str) == Some(session)
        };
        if let Some(pos) = self.inbox.iter().position(matches) {
            return Ok(self.inbox.remove(pos));
        }
        loop {
            let v = self.recv_msg(Some(deadline))?;
            if matches(&v) {
                return Ok(v);
            }
            self.inbox.push(v);
        }
    }

    fn recv_msg(&mut self, deadline: Option<Instant>) -> Result<Value, PdfError> {
        loop {
            match self.ws.read() {
                Ok(Message::Text(t)) => {
                    return serde_json::from_str(t.as_str())
                        .map_err(|e| PdfError::Cdp(format!("CDP 消息解析失败: {e}")));
                }
                Ok(Message::Binary(b)) => {
                    let text = String::from_utf8_lossy(&b);
                    return serde_json::from_str(&text)
                        .map_err(|e| PdfError::Cdp(format!("CDP 消息解析失败: {e}")));
                }
                Ok(Message::Ping(p)) => {
                    let _ = self.ws.send(Message::Pong(p));
                }
                Ok(Message::Close(f)) => {
                    return Err(PdfError::Cdp(format!("连接被浏览器关闭: {f:?}")));
                }
                Ok(_) => continue,
                // 读超时：检查 deadline 后继续轮询
                Err(tungstenite::Error::Io(e))
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut =>
                {
                    if let Some(d) = deadline {
                        if Instant::now() >= d {
                            return Err(PdfError::Timeout {
                                timeout_secs: crate::DEFAULT_TIMEOUT.as_secs(),
                            });
                        }
                    }
                    continue;
                }
                Err(e) => return Err(PdfError::Cdp(format!("接收失败: {e}"))),
            }
        }
    }

    /// `Runtime.evaluate`，返回取值后的结果（异常时返回 Cdp 错误）。
    pub(crate) fn evaluate(&mut self, session: &str, expr: &str) -> Result<Value, PdfError> {
        let r = self.call(
            "Runtime.evaluate",
            json!({"expression": expr, "returnByValue": true}),
            Some(session),
        )?;
        if r.get("exceptionDetails").is_some_and(|v| !v.is_null()) {
            let desc = r
                .pointer("/exceptionDetails/exception/description")
                .and_then(Value::as_str)
                .unwrap_or("未知异常");
            return Err(PdfError::Cdp(format!("evaluate 异常: {desc}")));
        }
        Ok(r.pointer("/result/value").cloned().unwrap_or(Value::Null))
    }

    /// 轮询直到表达式为真（Paged.js 渲染完成标志）。
    pub(crate) fn wait_until_true(
        &mut self,
        session: &str,
        expr: &str,
        deadline: Instant,
    ) -> Result<(), PdfError> {
        loop {
            if self.evaluate(session, expr)? == Value::Bool(true) {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(PdfError::Cdp(format!("等待表达式超时: {expr}")));
            }
            std::thread::sleep(POLL_INTERVAL);
        }
    }
}
