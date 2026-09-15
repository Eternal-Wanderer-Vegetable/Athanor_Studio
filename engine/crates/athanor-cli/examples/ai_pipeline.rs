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

//! A2 示例：把 `.azodoc` 喂给 LLM，产出语义标注与改写修订（author `ai:*`）。
//!
//! 用法：
//!
//! ```text
//! cargo run -p athanor-cli --example ai_pipeline -- <doc.azodoc> [--provider fake|http]
//! ```
//!
//! - `fake`（默认，未配置 `AZODOC_AI_BASE_URL` 时）：内置确定性提供方，离线可跑。
//! - `http`：OpenAI 兼容 `/chat/completions`，环境变量
//!   `AZODOC_AI_BASE_URL` / `AZODOC_AI_API_KEY` / `AZODOC_AI_MODEL`。
//!
//! 运行后全程可审计：
//!
//! ```text
//! athanor history <doc.azodoc>          # AI 落链的修订（author: ai:<model>）
//! athanor annotations <doc.azodoc>      # AI 标注（带 confidence / source）
//! athanor checkout --revision <rev> ... # 人工否决：回滚 AI 改写
//! ```

use athanor_cli::ai_demo::{self, AiProvider, FakeProvider, HttpProvider};
use std::path::PathBuf;

fn main() {
    let mut doc: Option<PathBuf> = None;
    let mut mode = "auto".to_string();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--provider" => mode = args.next().unwrap_or_else(|| die_usage()),
            other if doc.is_none() => doc = Some(PathBuf::from(other)),
            _ => die_usage(),
        }
    }
    let Some(doc) = doc else { die_usage() };

    let provider: Box<dyn AiProvider> = match mode.as_str() {
        "fake" => Box::new(FakeProvider::default()),
        "http" => match HttpProvider::from_env() {
            Ok(p) => Box::new(p),
            Err(e) => {
                eprintln!("{}", e.friendly());
                std::process::exit(1);
            }
        },
        _ => {
            if HttpProvider::is_configured() {
                match HttpProvider::from_env() {
                    Ok(p) => Box::new(p),
                    Err(e) => {
                        eprintln!("{}", e.friendly());
                        std::process::exit(1);
                    }
                }
            } else {
                Box::new(FakeProvider::default())
            }
        }
    };

    println!("AI 管线开始（provider: {}）", provider.model());
    println!("  文档: {}", doc.display());
    match ai_demo::run(&doc, provider.as_ref()) {
        Ok(r) => {
            println!("完成：");
            println!(
                "  语义标注: {} 条（author ai:{}，带 confidence）",
                r.annotations_added, r.model
            );
            println!("  改写落链: {} 条", r.rewrites_applied);
            if let Some(rev) = &r.revision {
                println!("  新修订:   {rev}");
            }
            println!();
            println!("人工复核（AI 产出默认待审）：");
            println!("  athanor history {:?}", doc);
            println!("  athanor annotations {:?}", doc);
            if r.revision.is_some() {
                println!(
                    "  否决改写: athanor checkout {:?} --revision <人工修订ID>",
                    doc
                );
            }
        }
        Err(e) => {
            eprintln!("{}", e.friendly());
            std::process::exit(1);
        }
    }
}

fn die_usage() -> ! {
    eprintln!("用法: ai_pipeline <doc.azodoc> [--provider fake|http]");
    std::process::exit(2);
}
