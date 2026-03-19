#[cfg(feature = "embedding")]
use crate::embedding::store::EmbeddingStore;
#[cfg(feature = "embedding")]
use crate::models::BlockId;
use crate::models::{CodeBlock, SearchResult};
use crate::search::bm25::Bm25Engine;
#[cfg(feature = "embedding")]
use crate::search::embedding::EmbeddingEngine;
use std::collections::HashMap;
#[cfg(feature = "embedding")]
use tracing::warn;

/// 混合搜索引擎 — 组合 BM25 关键词搜索 + Embedding 语义搜索
pub struct HybridEngine {
    bm25: Bm25Engine,
    #[cfg(feature = "embedding")]
    embedding: Option<EmbeddingEngine>,
    #[cfg(feature = "embedding")]
    alpha: f64,
}

impl HybridEngine {
    #[cfg(feature = "embedding")]
    pub fn new(bm25: Bm25Engine, embedding: Option<EmbeddingEngine>) -> Self {
        Self {
            bm25,
            embedding,
            alpha: 0.5,
        }
    }

    #[cfg(not(feature = "embedding"))]
    pub fn new(bm25: Bm25Engine) -> Self {
        Self { bm25 }
    }

    /// 混合搜索：BM25 + Embedding 加权合并
    #[cfg(feature = "embedding")]
    pub fn search(
        &self,
        query: &str,
        blocks: &[&CodeBlock],
        embedding_store: Option<&EmbeddingStore>,
        lang: Option<&str>,
        limit: usize,
    ) -> Vec<SearchResult> {
        // 1. BM25 搜索（取 3 倍 limit，留足空间给混合排序）
        let bm25_results = self.bm25.search(query, blocks, lang, limit * 3);

        // 如果没有 embedding 引擎，直接返回 BM25 结果（截断到 limit）
        let (embedding_engine, emb_store) = match (self.embedding.as_ref(), embedding_store) {
            (Some(engine), Some(store)) => (engine, store),
            _ => {
                let mut results = bm25_results;
                results.truncate(limit);
                return results;
            }
        };

        // 2. Embedding 搜索
        let emb_scores = match embedding_engine.search(query, blocks, emb_store, limit * 3) {
            Ok(scores) => scores,
            Err(e) => {
                warn!(error = %e, "Embedding 搜索失败，退化为纯 BM25");
                let mut results = bm25_results;
                results.truncate(limit);
                return results;
            }
        };

        // 3. 归一化 BM25 分数到 [0, 1]
        let bm25_map: HashMap<BlockId, (f64, &SearchResult)> = {
            let max_score = bm25_results.iter().map(|r| r.score).fold(0.0f64, f64::max);
            let min_score = bm25_results
                .iter()
                .map(|r| r.score)
                .fold(f64::MAX, f64::min);
            let range = max_score - min_score;

            bm25_results
                .iter()
                .map(|r| {
                    let normalized = if range > 0.0 {
                        (r.score - min_score) / range
                    } else {
                        1.0
                    };
                    (r.block.block_id(), (normalized, r))
                })
                .collect()
        };

        // 4. 归一化 cosine similarity 到 [0, 1]（原始范围 [-1, 1]）
        let emb_map: HashMap<BlockId, f64> = emb_scores
            .into_iter()
            .map(|(id, sim)| (id, (sim + 1.0) / 2.0))
            .collect();

        // 5. 合并所有候选的混合分数
        let block_map: HashMap<BlockId, &CodeBlock> =
            blocks.iter().map(|b| (b.block_id(), *b)).collect();

        let mut all_ids: Vec<BlockId> = bm25_map.keys().cloned().collect();
        for id in emb_map.keys() {
            if !bm25_map.contains_key(id) {
                all_ids.push(id.clone());
            }
        }

        let mut hybrid_results: Vec<SearchResult> = all_ids
            .iter()
            .filter_map(|id| {
                let bm25_score = bm25_map.get(id).map(|(s, _)| *s).unwrap_or(0.0);
                let emb_score = emb_map.get(id).copied().unwrap_or(0.0);

                let final_score = self.alpha * bm25_score + (1.0 - self.alpha) * emb_score;

                if let Some((_, bm25_result)) = bm25_map.get(id) {
                    Some(SearchResult {
                        block: bm25_result.block.clone(),
                        score: final_score,
                        context_code: bm25_result.context_code.clone(),
                    })
                } else {
                    block_map.get(id).map(|block| SearchResult {
                        block: (*block).clone(),
                        score: final_score,
                        context_code: None,
                    })
                }
            })
            .collect();

        // 6. 排序 + 去重 + 多样性限制
        hybrid_results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Self::dedup_and_diversify(hybrid_results, limit)
    }

    /// 纯 BM25 模式（无 embedding feature 时）
    #[cfg(not(feature = "embedding"))]
    pub fn search(
        &self,
        query: &str,
        blocks: &[&CodeBlock],
        lang: Option<&str>,
        limit: usize,
    ) -> Vec<SearchResult> {
        self.bm25.search(query, blocks, lang, limit)
    }

    /// 去重 + 同文件限制 + 截取
    fn dedup_and_diversify(results: Vec<SearchResult>, limit: usize) -> Vec<SearchResult> {
        let mut deduped: Vec<SearchResult> = Vec::new();
        for result in results {
            let dominated = deduped.iter().any(|existing| {
                existing.block.file_path == result.block.file_path
                    && existing.block.start_line <= result.block.start_line
                    && existing.block.end_line >= result.block.end_line
            });
            if dominated {
                continue;
            }
            deduped.retain(|existing| {
                !(existing.block.file_path == result.block.file_path
                    && result.block.start_line <= existing.block.start_line
                    && result.block.end_line >= existing.block.end_line)
            });
            deduped.push(result);
        }

        let max_per_file: usize = 3;
        let mut file_counts: HashMap<&str, usize> = HashMap::new();
        let mut diverse: Vec<SearchResult> = Vec::new();
        for result in &deduped {
            let count = file_counts.entry(&result.block.file_path).or_insert(0);
            if *count < max_per_file {
                *count += 1;
                diverse.push(result.clone());
            }
        }

        diverse.truncate(limit);
        diverse
    }
}
