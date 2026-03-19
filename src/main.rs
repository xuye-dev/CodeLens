mod embedding;
mod error;
mod indexer;
mod mcp;
mod models;
mod parser;
mod scanner;
mod search;

#[cfg(feature = "embedding")]
use crate::embedding::model::EmbeddingModel;
#[cfg(feature = "embedding")]
use crate::embedding::store::EmbeddingStore;
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

    /// 模型文件目录（默认 ~/.cache/codelens/models/）
    #[cfg(feature = "embedding")]
    #[arg(long)]
    model_dir: Option<String>,

    /// 禁用语义搜索（仅使用 BM25 关键词搜索）
    #[cfg(feature = "embedding")]
    #[arg(long, default_value = "false")]
    no_embedding: bool,
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

    // 4. 初始化 Embedding（可选，仅当 feature 启用时）
    #[cfg(feature = "embedding")]
    let (embedding_model, embedding_store) = init_embedding(&cli, &store).await;

    // 5. 将 store 和 builder 包装为共享引用
    let store = Arc::new(Mutex::new(store));
    let builder = Arc::new(builder);

    // 6. 启动文件监听（增量更新）
    #[cfg(feature = "embedding")]
    let _watcher = FileWatcher::start(
        &root,
        Arc::clone(&store),
        Arc::clone(&builder),
        embedding_model.clone(),
        embedding_store.as_ref().map(Arc::clone),
    )?;
    #[cfg(not(feature = "embedding"))]
    let _watcher = FileWatcher::start(&root, Arc::clone(&store), Arc::clone(&builder))?;

    info!("索引构建完成，启动 MCP Server...");

    // 7. 启动 MCP Server（通过 stdio 通信）
    #[cfg(feature = "embedding")]
    let server = CodeLensServer::new(Arc::clone(&store), embedding_model, embedding_store);
    #[cfg(not(feature = "embedding"))]
    let server = CodeLensServer::new(Arc::clone(&store));

    let transport = rmcp::transport::io::stdio();
    let server_handle = server.serve(transport).await?;
    server_handle.waiting().await?;

    Ok(())
}

#[cfg(feature = "embedding")]
async fn init_embedding(
    cli: &Cli,
    store: &IndexStore,
) -> (
    Option<Arc<EmbeddingModel>>,
    Option<Arc<Mutex<EmbeddingStore>>>,
) {
    if cli.no_embedding {
        info!("语义搜索已禁用（--no-embedding）");
        return (None, None);
    }

    let model_dir_path = cli.model_dir.as_deref().map(Path::new);

    match embedding::downloader::ensure_model_files(model_dir_path).await {
        Ok(model_dir) => match EmbeddingModel::load(&model_dir) {
            Ok(model) => {
                let model = Arc::new(model);
                let mut emb_store = EmbeddingStore::new();

                let all_blocks = store.all_blocks();
                info!(block_count = all_blocks.len(), "开始计算 embedding 向量...");

                let texts: Vec<String> = all_blocks.iter().map(|b| b.embedding_text()).collect();
                let text_refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();

                match model.embed_batch(&text_refs) {
                    Ok(vectors) => {
                        for (block, vector) in all_blocks.iter().zip(vectors.into_iter()) {
                            emb_store.insert(block.block_id(), vector);
                        }
                        info!(embedding_count = emb_store.count(), "Embedding 计算完成");
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "批量 embedding 计算失败，语义搜索将不可用");
                    }
                }

                (Some(model), Some(Arc::new(Mutex::new(emb_store))))
            }
            Err(e) => {
                tracing::warn!(error = %e, "Embedding 模型加载失败，退化为纯 BM25");
                (None, None)
            }
        },
        Err(e) => {
            tracing::warn!(error = %e, "模型文件准备失败，退化为纯 BM25");
            (None, None)
        }
    }
}
