use crate::error::Result;
use crate::indexer::store::IndexStore;
use crate::parser::{self, Parser};
use crate::scanner::Scanner;
use tracing::{info, warn};

/// 索引构建器 — 串联 Scanner → Parser → IndexStore 的完整流程
pub struct IndexBuilder {
    parsers: Vec<Box<dyn Parser>>,
}

impl IndexBuilder {
    pub fn new() -> Self {
        Self {
            parsers: parser::create_parsers(),
        }
    }

    /// 全量构建索引：扫描目录 → 解析文件 → 写入存储
    pub fn build(&self, scanner: &Scanner, store: &mut IndexStore) -> Result<()> {
        let files = scanner.scan()?;
        info!(file_count = files.len(), "扫描完成，开始解析文件");

        let mut parsed_count = 0;
        let mut error_count = 0;

        for file_path in &files {
            if let Some(parser_idx) = parser::get_parser_for_file(file_path, &self.parsers) {
                match self.parsers[parser_idx].parse(file_path) {
                    Ok(blocks) => {
                        if !blocks.is_empty() {
                            store.add(file_path.clone(), blocks);
                            parsed_count += 1;
                        }
                    }
                    Err(e) => {
                        warn!(path = %file_path.display(), error = %e, "解析文件失败，跳过");
                        error_count += 1;
                    }
                }
            }
        }

        info!(
            parsed_count,
            error_count,
            block_count = store.block_count(),
            "索引构建完成"
        );

        Ok(())
    }

    /// 重新解析单个文件并更新索引
    pub fn reindex_file(&self, file_path: &std::path::Path, store: &mut IndexStore) -> Result<()> {
        if let Some(parser_idx) = parser::get_parser_for_file(file_path, &self.parsers) {
            match self.parsers[parser_idx].parse(file_path) {
                Ok(blocks) => {
                    store.update(file_path.to_path_buf(), blocks);
                }
                Err(e) => {
                    warn!(path = %file_path.display(), error = %e, "重新解析文件失败");
                }
            }
        }
        Ok(())
    }

    /// 获取解析器列表的引用（供外部使用）
    pub fn parsers(&self) -> &[Box<dyn Parser>] {
        &self.parsers
    }
}

impl Default for IndexBuilder {
    fn default() -> Self {
        Self::new()
    }
}
