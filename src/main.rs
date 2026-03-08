mod error;
mod indexer;
mod mcp;
mod models;
mod parser;
mod scanner;
mod search;

use crate::indexer::builder::IndexBuilder;
use crate::indexer::store::IndexStore;
use crate::indexer::watcher::FileWatcher;
use crate::mcp::server::CodeLensServer;
use clap::Parser;
use rmcp::ServiceExt;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tracing::info;

/// CodeLens — 本地代码上下文检索 MCP Server
#[derive(Parser, Debug)]
#[command(name = "codelens", version, about = "本地代码上下文检索 MCP Server")]
struct Cli {
    /// 项目路径（要扫描和索引的目录，默认为当前工作目录）
    #[arg(short, long, default_value = ".")]
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
    let project_path = Path::new(&cli.path);

    info!(path = %cli.path, "CodeLens 启动中...");

    // 1. Scanner 扫描文件
    let scanner_instance = scanner::Scanner::new(project_path)?;
    let root = scanner_instance.root().to_path_buf();

    // 2. 创建索引存储和构建器
    let mut store = IndexStore::new();
    let builder = IndexBuilder::new();

    // 3. 全量构建索引
    builder.build(&scanner_instance, &mut store)?;

    // 4. 将 store 和 builder 包装为共享引用
    let store = Arc::new(Mutex::new(store));
    let builder = Arc::new(builder);

    // 5. 启动文件监听（增量更新）
    let _watcher = FileWatcher::start(&root, Arc::clone(&store), Arc::clone(&builder))?;

    info!("索引构建完成，启动 MCP Server...");

    // 6. 启动 MCP Server（通过 stdio 通信）
    let server = CodeLensServer::new(Arc::clone(&store));
    let transport = rmcp::transport::io::stdio();

    let server_handle = server.serve(transport).await?;
    server_handle.waiting().await?;

    Ok(())
}
