mod error;
mod indexer;
mod models;
mod parser;
mod scanner;

use clap::Parser;
use tracing::info;

/// CodeLens — 本地代码上下文检索 MCP Server
#[derive(Parser, Debug)]
#[command(name = "codelens", version, about = "本地代码上下文检索 MCP Server")]
struct Cli {
    /// 项目路径（要扫描和索引的目录）
    #[arg(short, long)]
    path: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化结构化日志（输出到 stderr，避免干扰 stdio 通信）
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    // 解析命令行参数
    let cli = Cli::parse();

    info!(path = %cli.path, "CodeLens 启动中...");

    // TODO: 后续阶段实现完整启动流程
    // 1. Scanner 扫描文件
    // 2. Parser 解析代码
    // 3. Indexer 建立索引
    // 4. 启动文件监听
    // 5. 启动 MCP Server

    info!("CodeLens 初始化完成，等待后续模块接入");

    Ok(())
}
