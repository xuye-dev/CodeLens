use crate::models::{CodeBlock, SearchResult};
use std::collections::HashMap;

/// BM25 检索引擎 — 对代码块进行关键词相关度排序
pub struct Bm25Engine {
    /// BM25 参数 k1：词频饱和度（通常 1.2 ~ 2.0）
    k1: f64,
    /// BM25 参数 b：文档长度归一化（通常 0.75）
    b: f64,
}

impl Bm25Engine {
    pub fn new() -> Self {
        Self { k1: 1.5, b: 0.75 }
    }

    /// 搜索匹配的代码块，返回按 BM25 分数排序的结果
    pub fn search(
        &self,
        query: &str,
        blocks: &[&CodeBlock],
        lang: Option<&str>,
        limit: usize,
    ) -> Vec<SearchResult> {
        // 过滤语言（支持逗号分隔的多语言，如 "vue,javascript"）
        let filtered: Vec<&&CodeBlock> = if let Some(lang) = lang {
            let langs: Vec<&str> = lang.split(',').map(|s| s.trim()).collect();
            blocks
                .iter()
                .filter(|b| langs.iter().any(|l| b.language == *l))
                .collect()
        } else {
            blocks.iter().collect()
        };

        if filtered.is_empty() {
            return Vec::new();
        }

        // 分词（对查询和文档都进行简单分词）
        let query_terms = tokenize(query);
        if query_terms.is_empty() {
            return Vec::new();
        }

        // 计算文档平均长度
        let total_len: usize = filtered.iter().map(|b| document_length(b)).sum();
        let avg_dl = total_len as f64 / filtered.len() as f64;

        // 计算每个词项的 IDF（逆文档频率）
        let n = filtered.len() as f64;
        let mut idf_map: HashMap<&str, f64> = HashMap::new();
        for term in &query_terms {
            let df = filtered
                .iter()
                .filter(|b| document_contains(b, term))
                .count() as f64;
            // BM25 IDF 公式
            let idf = ((n - df + 0.5) / (df + 0.5) + 1.0).ln();
            idf_map.insert(term, idf.max(0.0));
        }

        // 计算每个文档的 BM25 分数
        let mut scored: Vec<SearchResult> = filtered
            .iter()
            .filter_map(|block| {
                let dl = document_length(block) as f64;
                let mut score = 0.0;

                for term in &query_terms {
                    let tf = term_frequency(block, term) as f64;
                    let idf = idf_map.get(term.as_str()).copied().unwrap_or(0.0);

                    // BM25 公式
                    let numerator = tf * (self.k1 + 1.0);
                    let denominator = tf + self.k1 * (1.0 - self.b + self.b * dl / avg_dl);
                    score += idf * numerator / denominator;
                }

                // 名称匹配加分 — 区分定义类型和引用类型
                let name_lower = block.name.to_lowercase();
                let query_lower = query.to_lowercase();
                let is_definition = matches!(
                    block.kind,
                    crate::models::BlockKind::Class
                        | crate::models::BlockKind::Interface
                        | crate::models::BlockKind::Enum
                );
                if name_lower == query_lower {
                    score *= if is_definition {
                        5.0
                    } else if matches!(
                        block.kind,
                        crate::models::BlockKind::Method | crate::models::BlockKind::Constructor
                    ) {
                        3.0
                    } else {
                        2.0
                    };
                } else if name_lower.contains(&query_lower) {
                    score *= if is_definition { 2.5 } else { 1.5 };
                }

                // 代码块类型权重：定义类 > 引用类
                let kind_boost = match block.kind {
                    crate::models::BlockKind::Class
                    | crate::models::BlockKind::Interface
                    | crate::models::BlockKind::Enum => 2.0,
                    crate::models::BlockKind::Method | crate::models::BlockKind::Constructor => 1.3,
                    crate::models::BlockKind::XmlNode => 1.2,
                    crate::models::BlockKind::XmlNamespace => 1.1,
                    crate::models::BlockKind::Field => 1.0,
                    crate::models::BlockKind::Import => 0.4,
                    _ => 1.0,
                };
                score *= kind_boost;

                if score > 0.0 {
                    Some(SearchResult {
                        block: (**block).clone(),
                        score,
                        context_code: None,
                    })
                } else {
                    None
                }
            })
            .collect();

        // 按分数降序排序
        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // 去重：父子代码块重叠时只保留更具体的（行范围被包含的保留子块）
        let mut deduped: Vec<SearchResult> = Vec::new();
        for result in scored {
            let dominated = deduped.iter().any(|existing| {
                existing.block.file_path == result.block.file_path
                    && existing.block.start_line <= result.block.start_line
                    && existing.block.end_line >= result.block.end_line
            });
            if dominated {
                continue;
            }
            // 如果新结果包含了已有结果，替换掉被包含的
            deduped.retain(|existing| {
                !(existing.block.file_path == result.block.file_path
                    && result.block.start_line <= existing.block.start_line
                    && result.block.end_line >= existing.block.end_line)
            });
            deduped.push(result);
        }

        // 同文件最多保留 3 个结果，保证结果多样性
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

        // 截取 top-N
        diverse.truncate(limit);

        diverse
    }
}

impl Default for Bm25Engine {
    fn default() -> Self {
        Self::new()
    }
}

/// 简单分词：按空白符和常见分隔符拆分，转小写
fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| {
        c.is_whitespace()
            || c == '.'
            || c == ','
            || c == ';'
            || c == '('
            || c == ')'
            || c == '{'
            || c == '}'
            || c == '<'
            || c == '>'
            || c == ':'
            || c == '/'
            || c == '\\'
    })
    .filter(|s| !s.is_empty())
    .map(|s| s.to_lowercase())
    .collect()
}

/// 将代码块的所有文本内容（名称、内容、签名等）合并后分词
fn document_tokens(block: &CodeBlock) -> Vec<String> {
    let mut text = String::new();
    text.push_str(&block.name);
    text.push(' ');
    text.push_str(&block.content);
    if let Some(ref sig) = block.signature {
        text.push(' ');
        text.push_str(sig);
    }
    for ann in &block.annotations {
        text.push(' ');
        text.push_str(ann);
    }
    if let Some(ref parent) = block.parent {
        text.push(' ');
        text.push_str(parent);
    }
    tokenize(&text)
}

/// 文档长度（词项数）
fn document_length(block: &CodeBlock) -> usize {
    document_tokens(block).len()
}

/// 检查文档是否包含某词项
fn document_contains(block: &CodeBlock, term: &str) -> bool {
    let term_lower = term.to_lowercase();
    // 快速路径：先检查原始文本
    let content_lower = block.content.to_lowercase();
    if content_lower.contains(&term_lower) {
        return true;
    }
    let name_lower = block.name.to_lowercase();
    if name_lower.contains(&term_lower) {
        return true;
    }
    false
}

/// 计算词项在文档中的出现次数（精确匹配 token）
fn term_frequency(block: &CodeBlock, term: &str) -> usize {
    let tokens = document_tokens(block);
    let term_lower = term.to_lowercase();
    tokens.iter().filter(|t| **t == term_lower).count()
}
