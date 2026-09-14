//! athanor — Azodoc 引擎命令行（M1：new / info / verify / recover）。

mod verify_cmd;

use azodoc_container::ContainerError;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "athanor", version, about = "Athanor — Azodoc 文档引擎命令行")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// 创建一个新的最小 Azodoc 文档（不覆盖已有文件）
    New {
        /// 目标 .azodoc 路径
        path: PathBuf,
        /// 文档标题
        #[arg(long)]
        title: Option<String>,
        /// 文档语言（BCP 47）
        #[arg(long, default_value = "zh-CN")]
        lang: String,
    },
    /// 显示文档概要
    Info {
        path: PathBuf,
        /// 以 JSON 输出 manifest
        #[arg(long)]
        json: bool,
    },
    /// 校验容器完整性与规范符合性（退出码 0=有效，1=无效）
    Verify { path: PathBuf },
    /// 从受损文件中抢救内容到目录（源文件只读，不修改）
    Recover {
        path: PathBuf,
        /// 输出目录
        #[arg(short, long)]
        out: PathBuf,
    },
}

fn main() {
    let cli = Cli::parse();
    let code = match cli.command {
        Command::New { path, title, lang } => cmd_new(&path, title.as_deref(), &lang),
        Command::Info { path, json } => cmd_info(&path, json),
        Command::Verify { path } => verify_cmd::run(&path),
        Command::Recover { path, out } => cmd_recover(&path, &out),
    };
    std::process::exit(code);
}

fn die(e: ContainerError) -> i32 {
    eprintln!("{}", e.friendly());
    1
}

fn read_container(
    path: &std::path::Path,
) -> Result<(azodoc_container::Container, Vec<String>), i32> {
    let data = std::fs::read(path).map_err(|e| {
        eprintln!("错误：无法读取 {}: {e}", path.display());
        1
    })?;
    azodoc_container::open(data).map_err(|e| {
        eprintln!("{}", e.friendly());
        1
    })
}

fn cmd_new(path: &std::path::Path, title: Option<&str>, lang: &str) -> i32 {
    if path.exists() {
        eprintln!(
            "错误：目标文件已存在，athanor new 不覆盖已有文件: {}",
            path.display()
        );
        return 1;
    }
    let title = title.unwrap_or("未命名文档");
    match azodoc_container::builder::build_minimal_document(title, lang) {
        Ok(bytes) => match std::fs::write(path, &bytes) {
            Ok(_) => {
                println!("已创建 {}（{title}）", path.display());
                println!("  查看: athanor info {}", path.display());
                println!("  校验: athanor verify {}", path.display());
                0
            }
            Err(e) => {
                eprintln!("错误：写入失败: {e}");
                1
            }
        },
        Err(e) => die(e),
    }
}

fn cmd_info(path: &std::path::Path, as_json: bool) -> i32 {
    let (c, warnings) = match read_container(path) {
        Ok(x) => x,
        Err(code) => return code,
    };
    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(c.manifest_value()).unwrap_or_else(|_| "{}".into())
        );
        print_warnings(&warnings);
        return 0;
    }

    let m = c.manifest_typed();
    let profile_str = match c.profile {
        azodoc_container::Profile::Prefixed => "prefixed",
        azodoc_container::Profile::PlainZip => "plain_zip",
    };
    println!(
        "文档:   {} ({})",
        m.document.title.as_deref().unwrap_or("<未命名>"),
        m.document.id
    );
    println!("格式:   {} · {}", m.azodoc.format_version, profile_str);
    println!(
        "修订:   {}",
        m.current_revision.as_deref().unwrap_or("（无）")
    );
    println!("条目:   {}", c.entry_names().len());
    println!("层:");
    for (key, v) in m.layers.iter() {
        match serde_json::from_value::<azodoc_model::manifest::LayerRef>(v.clone()) {
            Ok(lr) => println!(
                "  {key:<14} {}  {}…",
                lr.path,
                &lr.sha256[..lr.sha256.len().min(8)]
            ),
            Err(_) => println!("  {key:<14} <结构未识别，按 R2 保留>"),
        }
    }
    println!("兼容缓存:");
    for e in &m.compatibility {
        let fresh = m.compat_is_fresh(e);
        println!(
            "  {:<10} {:<38} {}",
            e.format,
            e.path,
            if fresh {
                "fresh"
            } else {
                "stale（M2 起 athanor upgrade 重建）"
            }
        );
    }
    let unknown = azodoc_model::manifest::unknown_manifest_key_paths(c.manifest_value());
    if !unknown.is_empty() {
        println!("未知 manifest 字段（R2 保留）: {}", unknown.join(", "));
    }
    print_warnings(&warnings);
    0
}

fn print_warnings(warnings: &[String]) {
    for w in warnings {
        println!("警告: {w}");
    }
}

fn cmd_recover(path: &std::path::Path, out: &std::path::Path) -> i32 {
    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("错误：无法读取 {}: {e}", path.display());
            return 1;
        }
    };
    match azodoc_container::recover::recover(&data, out) {
        Ok(outcome) => {
            println!("模式: {}", outcome.mode);
            for line in &outcome.lines {
                match line {
                    Ok(msg) => println!("  OK    {msg}"),
                    Err(msg) => println!("  失败  {msg}"),
                }
            }
            println!(
                "汇总: 成功 {} · 失败 {} · 输出目录 {}",
                outcome.salvaged,
                outcome.failed,
                outcome.out_dir.display()
            );
            println!("报告: {}", out.join("recovery-report.txt").display());
            if outcome.salvaged == 0 {
                1
            } else {
                0
            }
        }
        Err(e) => die(e),
    }
}

/// 供 verify_cmd 复用的小工具。
pub(crate) fn sha256_hex(data: &[u8]) -> String {
    use sha2::Digest;
    let d = sha2::Sha256::digest(data);
    let mut s = String::with_capacity(d.len() * 2);
    for b in d {
        s.push_str(&format!("{b:02x}"));
    }
    s
}
