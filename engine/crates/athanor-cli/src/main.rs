//! athanor — Azodoc 引擎命令行（薄壳；实现在 lib.rs）。

use athanor_cli::{
    cmd_import, cmd_info, cmd_new, cmd_publish, cmd_recover, cmd_transmute, cmd_upgrade,
};
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
        path: PathBuf,
        #[arg(long)]
        title: Option<String>,
        #[arg(long, default_value = "zh-CN")]
        lang: String,
    },
    /// 显示文档概要
    Info {
        path: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// 校验容器完整性与规范符合性（退出码 0=有效，1=无效）
    Verify { path: PathBuf },
    /// 从受损文件中抢救内容到目录（源文件只读，不修改）
    Recover {
        path: PathBuf,
        #[arg(short, long)]
        out: PathBuf,
    },
    /// 导入 Markdown / HTML 生成 .azodoc（不覆盖已有文件）
    Import {
        /// 输入文件（.md / .html）
        input: PathBuf,
        /// 目标 .azodoc 路径
        #[arg(short, long)]
        out: PathBuf,
        /// 强制指定格式（默认按扩展名）
        #[arg(long)]
        format: Option<String>,
        /// 文档语言（输入未提供时使用）
        #[arg(long, default_value = "zh-CN")]
        lang: String,
        /// 额外把转换报告写到容器外
        #[arg(long)]
        report: Option<PathBuf>,
        /// 存在任何损失时以退出码 3 结束
        #[arg(long)]
        strict_loss: bool,
    },
    /// 导出为 markdown / txt / html（默认刷新容器内兼容缓存）
    Transmute {
        path: PathBuf,
        /// 目标格式：markdown|md, text|txt, html
        #[arg(long)]
        to: String,
        /// 输出文件（默认与容器同目录、同名不同扩展名）
        #[arg(long)]
        out: Option<PathBuf>,
        /// 不刷新容器内兼容缓存
        #[arg(long)]
        no_cache: bool,
        /// 存在任何损失时以退出码 3 结束
        #[arg(long)]
        strict_loss: bool,
    },
    /// 重建所有过期（stale）的兼容缓存
    Upgrade { path: PathBuf },
    /// 出版为 PDF（无头 Chromium 打印）并写入 publication 记录
    Publish {
        path: PathBuf,
        /// 额外把 PDF 复制到该路径
        #[arg(short, long)]
        out: Option<PathBuf>,
        /// 浏览器路径（覆盖自动定位）
        #[arg(long)]
        browser: Option<String>,
    },
    /// 显示修订链
    History {
        path: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// 把当前内容落为一条新修订
    Commit {
        path: PathBuf,
        /// 作者，格式 `类型:标识`（类型: human|ai|importer|converter|system）
        #[arg(long)]
        author: String,
        #[arg(long, default_value = "")]
        message: String,
    },
    /// 把内容还原为某修订的快照（ID 原样保留；标注自动重定位）
    Checkout {
        path: PathBuf,
        #[arg(long)]
        revision: String,
        /// 仅导出该修订快照到目录，不修改容器
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
    /// 添加一条语义标注
    Annotate {
        path: PathBuf,
        /// 目标块 ID
        #[arg(long)]
        block: String,
        /// 标注类型（如 Concept、ProgrammingLanguage）
        #[arg(long = "type")]
        ann_type: String,
        /// 标注值（JSON 对象）
        #[arg(long)]
        value: String,
        /// 引用文本（提供后使用 text_quote 锚定）
        #[arg(long)]
        exact: Option<String>,
        #[arg(long)]
        prefix: Option<String>,
        #[arg(long)]
        suffix: Option<String>,
        #[arg(long)]
        confidence: Option<f64>,
        /// 作者，格式 `类型:标识`
        #[arg(long)]
        author: Option<String>,
        #[arg(long)]
        source: Option<String>,
    },
    /// 列出语义标注
    Annotations {
        path: PathBuf,
        #[arg(long)]
        json: bool,
    },
}

fn main() {
    let cli = Cli::parse();
    let code = match cli.command {
        Command::New { path, title, lang } => cmd_new(&path, title.as_deref(), &lang),
        Command::Info { path, json } => cmd_info(&path, json),
        Command::Verify { path } => athanor_cli::verify_cmd::run(&path),
        Command::Recover { path, out } => cmd_recover(&path, &out),
        Command::Import {
            input,
            out,
            format,
            lang,
            report,
            strict_loss,
        } => cmd_import(
            &input,
            &out,
            format.as_deref(),
            &lang,
            report.as_deref(),
            strict_loss,
        ),
        Command::Transmute {
            path,
            to,
            out,
            no_cache,
            strict_loss,
        } => cmd_transmute(&path, &to, out.as_deref(), no_cache, strict_loss),
        Command::Upgrade { path } => cmd_upgrade(&path),
        Command::Publish { path, out, browser } => athanor_cli::cmd_publish(
            &path,
            &athanor_cli::PublishArgs {
                out: out.as_deref(),
                browser: browser.as_deref(),
            },
        ),
        Command::History { path, json } => athanor_cli::cmd_history(&path, json),
        Command::Commit {
            path,
            author,
            message,
        } => athanor_cli::cmd_commit(&path, &author, &message),
        Command::Checkout {
            path,
            revision,
            out,
        } => athanor_cli::cmd_checkout(&path, &revision, out.as_deref()),
        Command::Annotate {
            path,
            block,
            ann_type,
            value,
            exact,
            prefix,
            suffix,
            confidence,
            author,
            source,
        } => athanor_cli::cmd_annotate(
            &path,
            &athanor_cli::AnnotateArgs {
                block: &block,
                ann_type: &ann_type,
                value_json: &value,
                exact: exact.as_deref(),
                prefix: prefix.as_deref(),
                suffix: suffix.as_deref(),
                confidence,
                author: author.as_deref(),
                source: source.as_deref(),
            },
        ),
        Command::Annotations { path, json } => athanor_cli::cmd_annotations(&path, json),
    };
    std::process::exit(code);
}
