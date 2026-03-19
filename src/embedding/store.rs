use crate::models::BlockId;
use std::collections::HashMap;

/// 内存向量存储 — 按 BlockId 存储 embedding 向量
pub struct EmbeddingStore {
    embeddings: HashMap<BlockId, Vec<f32>>,
}

impl EmbeddingStore {
    pub fn new() -> Self {
        Self {
            embeddings: HashMap::new(),
        }
    }

    /// 插入或更新一个代码块的 embedding
    pub fn insert(&mut self, id: BlockId, vector: Vec<f32>) {
        self.embeddings.insert(id, vector);
    }

    /// 移除指定文件的所有 embedding
    pub fn remove_by_file(&mut self, file_path: &str) {
        self.embeddings.retain(|id, _| id.file_path != file_path);
    }

    /// 获取指定代码块的 embedding
    pub fn get(&self, id: &BlockId) -> Option<&Vec<f32>> {
        self.embeddings.get(id)
    }

    /// 获取所有 embedding 的引用
    pub fn all_embeddings(&self) -> &HashMap<BlockId, Vec<f32>> {
        &self.embeddings
    }

    /// 已存储的向量数量
    pub fn count(&self) -> usize {
        self.embeddings.len()
    }
}

impl Default for EmbeddingStore {
    fn default() -> Self {
        Self::new()
    }
}
