use crate::models::CodeBlock;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// 内存索引存储 — 按文件路径存储代码块集合
pub struct IndexStore {
    /// 文件路径 -> 该文件中提取的所有代码块
    blocks: HashMap<PathBuf, Vec<CodeBlock>>,
}

impl IndexStore {
    /// 创建空的索引存储
    pub fn new() -> Self {
        Self {
            blocks: HashMap::new(),
        }
    }

    /// 添加一个文件的代码块（覆盖已有数据）
    pub fn add(&mut self, file_path: PathBuf, code_blocks: Vec<CodeBlock>) {
        self.blocks.insert(file_path, code_blocks);
    }

    /// 移除指定文件的所有代码块
    pub fn remove(&mut self, file_path: &Path) -> Option<Vec<CodeBlock>> {
        self.blocks.remove(file_path)
    }

    /// 更新指定文件的代码块（等同于重新添加）
    pub fn update(&mut self, file_path: PathBuf, code_blocks: Vec<CodeBlock>) {
        self.add(file_path, code_blocks);
    }

    /// 获取所有已索引的代码块（扁平化）
    pub fn all_blocks(&self) -> Vec<&CodeBlock> {
        self.blocks.values().flat_map(|v| v.iter()).collect()
    }

    /// 按语言筛选代码块
    pub fn blocks_by_language(&self, lang: &str) -> Vec<&CodeBlock> {
        self.blocks
            .values()
            .flat_map(|v| v.iter())
            .filter(|b| b.language == lang)
            .collect()
    }

    /// 获取指定文件的代码块
    pub fn blocks_for_file(&self, file_path: &Path) -> Option<&Vec<CodeBlock>> {
        self.blocks.get(file_path)
    }

    /// 已索引文件数量
    pub fn file_count(&self) -> usize {
        self.blocks.len()
    }

    /// 已索引代码块总数
    pub fn block_count(&self) -> usize {
        self.blocks.values().map(|v| v.len()).sum()
    }

    /// 清空所有索引数据
    pub fn clear(&mut self) {
        self.blocks.clear();
    }
}

impl Default for IndexStore {
    fn default() -> Self {
        Self::new()
    }
}
