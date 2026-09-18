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

//! Aludel — Azodoc 文档编辑器原型（M6.2）。
//!
//! 用法：
//!
//! ```text
//! aludel <doc.azodoc> [--port N] [--no-open]
//! ```
//!
//! 仅监听 `127.0.0.1`，只服务启动时绑定的这一个文档；端口缺省由系统分配，
//! 启动后打印实际地址并尝试打开浏览器。

use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

use aludel::{api, http};

fn main() {
    let mut doc: Option<PathBuf> = None;
    let mut port: u16 = 0;
    let mut no_open = false;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--port" => match args.next().and_then(|v| v.parse::<u16>().ok()) {
                Some(p) => port = p,
                None => die_usage("--port 需要端口号"),
            },
            "--no-open" => no_open = true,
            other if doc.is_none() => doc = Some(PathBuf::from(other)),
            _ => die_usage("多余参数"),
        }
    }
    let Some(doc) = doc else {
        die_usage("缺少 <doc.azodoc>")
    };
    if !doc.is_file() {
        eprintln!("错误：文件不存在: {}", doc.display());
        std::process::exit(1);
    }

    // 预检：打不开的容器直接四段式报错退出（不进 server）
    let data = std::fs::read(&doc).unwrap_or_else(|e| {
        eprintln!("错误：无法读取 {}: {e}", doc.display());
        std::process::exit(1);
    });
    if let Err(e) = azodoc_container::open(data) {
        eprintln!("{}", e.friendly());
        std::process::exit(1);
    }

    let app = Arc::new(api::App::new(doc.clone()));
    let listener = TcpListener::bind(("127.0.0.1", port))
        .unwrap_or_else(|e| panic!("绑定 127.0.0.1:{port} 失败: {e}"));
    let addr = listener.local_addr().expect("listener 必有本地地址");
    println!("Aludel 编辑器（M6 原型）: http://{addr}");
    println!(
        "文档: {}",
        app.doc_path()
            .map(|p| p.display().to_string())
            .unwrap_or_default()
    );
    println!("Ctrl+C 退出。修订/标注信息也可用 athanor history/annotations 查看。");

    if !no_open {
        open_browser(&format!("http://{addr}"));
    }

    let handler_app = Arc::clone(&app);
    http::serve(listener, Arc::new(move |req| api::route(&handler_app, req)));
}

fn open_browser(url: &str) {
    let spawned = browser_command(url)
        .ok()
        .and_then(|mut cmd| cmd.spawn().ok());
    if spawned.is_none() {
        eprintln!("（自动打开浏览器失败，请手动访问 {url}）");
    }
}

#[cfg(target_os = "windows")]
fn browser_command(url: &str) -> Result<Command, ()> {
    let mut cmd = Command::new("cmd");
    cmd.args(["/C", "start", "", url]);
    Ok(cmd)
}

#[cfg(target_os = "macos")]
fn browser_command(url: &str) -> Result<Command, ()> {
    let mut cmd = Command::new("open");
    cmd.arg(url);
    Ok(cmd)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn browser_command(url: &str) -> Result<Command, ()> {
    let mut cmd = Command::new("xdg-open");
    cmd.arg(url);
    Ok(cmd)
}

fn die_usage(reason: &str) -> ! {
    eprintln!("错误：{reason}\n用法: aludel <doc.azodoc> [--port N] [--no-open]");
    std::process::exit(2);
}
