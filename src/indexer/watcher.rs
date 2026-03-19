use crate::embedding::model::EmbeddingModel;
use crate::embedding::store::EmbeddingStore;
use crate::error::Result;
use crate::indexer::builder::IndexBuilder;
use crate::indexer::store::IndexStore;
use crate::parser;
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

/// 文件监听器 — 监听项目目录变化，触发增量索引更新
pub struct FileWatcher {
    _watcher: RecommendedWatcher,
}

impl FileWatcher {
    /// 启动文件监听，返回 FileWatcher 实例
    ///
    /// 当文件发生变化时，自动重新解析并更新索引和 embedding。
    pub fn start(
        root: &Path,
        store: Arc<Mutex<IndexStore>>,
        builder: Arc<IndexBuilder>,
        embedding_model: Option<Arc<EmbeddingModel>>,
        embedding_store: Option<Arc<Mutex<EmbeddingStore>>>,
    ) -> Result<Self> {
        let (tx, mut rx) = mpsc::channel::<notify::Result<Event>>(256);

        // 创建文件监听器
        let mut watcher = RecommendedWatcher::new(
            move |res| {
                let _ = tx.blocking_send(res);
            },
            Config::default(),
        )?;

        watcher.watch(root, RecursiveMode::Recursive)?;

        info!(path = %root.display(), "文件监听已启动");

        // 在后台任务中处理文件变化事件
        tokio::spawn(async move {
            while let Some(event_result) = rx.recv().await {
                match event_result {
                    Ok(event) => {
                        handle_event(
                            event,
                            &store,
                            &builder,
                            embedding_model.as_ref(),
                            embedding_store.as_ref(),
                        );
                    }
                    Err(e) => {
                        warn!(error = %e, "文件监听事件错误");
                    }
                }
            }
        });

        Ok(Self { _watcher: watcher })
    }
}

/// 处理文件变化事件
fn handle_event(
    event: Event,
    store: &Arc<Mutex<IndexStore>>,
    builder: &Arc<IndexBuilder>,
    embedding_model: Option<&Arc<EmbeddingModel>>,
    embedding_store: Option<&Arc<Mutex<EmbeddingStore>>>,
) {
    let paths: Vec<&PathBuf> = event
        .paths
        .iter()
        .filter(|p| is_supported_file(p, builder.parsers()))
        .collect();

    if paths.is_empty() {
        return;
    }

    match event.kind {
        EventKind::Create(_) | EventKind::Modify(_) => {
            for path in paths {
                debug!(path = %path.display(), "文件变更，重新索引");
                if let Ok(mut store) = store.lock() {
                    if let Err(e) = builder.reindex_file(path, &mut store) {
                        warn!(path = %path.display(), error = %e, "增量更新失败");
                        continue;
                    }

                    // 更新 embedding
                    update_embeddings_for_file(path, &store, embedding_model, embedding_store);
                }
            }
        }
        EventKind::Remove(_) => {
            for path in paths {
                debug!(path = %path.display(), "文件删除，移除索引");
                if let Ok(mut store) = store.lock() {
                    store.remove(path);
                }

                // 移除对应的 embedding
                if let Some(emb_store) = embedding_store {
                    if let Ok(mut emb_store) = emb_store.lock() {
                        let path_str = path.to_string_lossy();
                        emb_store.remove_by_file(&path_str);
                    }
                }
            }
        }
        _ => {}
    }
}

/// 为指定文件更新 embedding
fn update_embeddings_for_file(
    path: &Path,
    store: &IndexStore,
    embedding_model: Option<&Arc<EmbeddingModel>>,
    embedding_store: Option<&Arc<Mutex<EmbeddingStore>>>,
) {
    let (model, emb_store_arc) = match (embedding_model, embedding_store) {
        (Some(m), Some(s)) => (m, s),
        _ => return,
    };

    let path_str = path.to_string_lossy();

    // 移除旧的 embedding
    if let Ok(mut emb_store) = emb_store_arc.lock() {
        emb_store.remove_by_file(&path_str);
    }

    // 获取该文件的新代码块并计算 embedding
    if let Some(blocks) = store.blocks_for_file(path) {
        if blocks.is_empty() {
            return;
        }

        let texts: Vec<String> = blocks.iter().map(|b| b.embedding_text()).collect();
        let text_refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();

        match model.embed_batch(&text_refs) {
            Ok(vectors) => {
                if let Ok(mut emb_store) = emb_store_arc.lock() {
                    for (block, vector) in blocks.iter().zip(vectors.into_iter()) {
                        emb_store.insert(block.block_id(), vector);
                    }
                }
                debug!(path = %path.display(), count = blocks.len(), "增量 embedding 更新完成");
            }
            Err(e) => {
                warn!(path = %path.display(), error = %e, "增量 embedding 计算失败");
            }
        }
    }
}

/// 检查文件是否为支持的类型
fn is_supported_file(path: &Path, parsers: &[Box<dyn crate::parser::Parser>]) -> bool {
    parser::get_parser_for_file(path, parsers).is_some()
}
