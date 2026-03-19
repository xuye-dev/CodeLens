use crate::embedding::model::EmbeddingModel;
use crate::embedding::store::EmbeddingStore;
use crate::error::Result;
use crate::models::{BlockId, CodeBlock};
use std::sync::Arc;

/// 向量语义搜索引擎 — 基于 cosine similarity 排序
pub struct EmbeddingEngine {
    model: Arc<EmbeddingModel>,
}

impl EmbeddingEngine {
    pub fn new(model: Arc<EmbeddingModel>) -> Self {
        Self { model }
    }

    /// 对代码块集合做向量相似度搜索
    ///
    /// 返回 (BlockId, cosine_similarity) 列表，按相似度降序排列。
    pub fn search(
        &self,
        query: &str,
        blocks: &[&CodeBlock],
        embedding_store: &EmbeddingStore,
        limit: usize,
    ) -> Result<Vec<(BlockId, f64)>> {
        let query_embedding = self.model.embed(query)?;

        let mut scores: Vec<(BlockId, f64)> = blocks
            .iter()
            .filter_map(|block| {
                let id = block.block_id();
                let block_embedding = embedding_store.get(&id)?;
                let sim = cosine_similarity(&query_embedding, block_embedding);
                Some((id, sim))
            })
            .collect();

        // 按相似度降序排序
        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        scores.truncate(limit * 3); // 多取一些，留给 HybridEngine 合并
        Ok(scores)
    }
}

/// 计算两个向量的 cosine similarity
///
/// 由于向量已 L2 归一化，cosine similarity 等于点积。
fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (*x as f64) * (*y as f64))
        .sum()
}
