use serde::{Deserialize, Serialize};

/// 代码块类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum BlockKind {
    /// 类定义
    Class,
    /// 接口定义
    Interface,
    /// 枚举定义
    Enum,
    /// 方法/函数定义
    Method,
    /// 构造函数
    Constructor,
    /// 字段/属性
    Field,
    /// 注解
    Annotation,
    /// Import 语句
    Import,
    /// XML 节点（如 MyBatis 的 select/insert 等）
    XmlNode,
    /// XML 命名空间
    XmlNamespace,
    /// 其他
    Other,
}

/// 代码块 — 索引和检索的最小单元
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeBlock {
    /// 文件绝对路径
    pub file_path: String,
    /// 起始行号（从 1 开始）
    pub start_line: usize,
    /// 结束行号（含）
    pub end_line: usize,
    /// 代码内容
    pub content: String,
    /// 语言类型（如 "java"、"xml"）
    pub language: String,
    /// 代码块类型
    pub kind: BlockKind,
    /// 名称（类名、方法名、XML 节点 id 等）
    pub name: String,
    /// 所属父级名称（如方法所属的类名）
    pub parent: Option<String>,
    /// 签名信息（如方法签名）
    pub signature: Option<String>,
    /// 注解列表
    pub annotations: Vec<String>,
    /// 依赖/引用信息
    pub dependencies: Vec<String>,
}

/// 代码块唯一标识 — 用于关联 embedding 向量
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct BlockId {
    pub file_path: String,
    pub start_line: usize,
    pub end_line: usize,
}

impl CodeBlock {
    /// 生成该代码块的唯一标识
    pub fn block_id(&self) -> BlockId {
        BlockId {
            file_path: self.file_path.clone(),
            start_line: self.start_line,
            end_line: self.end_line,
        }
    }

    /// 构造用于 embedding 的结构化文本
    pub fn embedding_text(&self) -> String {
        let mut text = format!("{} {:?} {}", self.language, self.kind, self.name);
        if let Some(ref sig) = self.signature {
            text.push(' ');
            text.push_str(sig);
        }
        if let Some(ref parent) = self.parent {
            text.push(' ');
            text.push_str(parent);
        }
        for ann in &self.annotations {
            text.push(' ');
            text.push_str(ann);
        }
        // 附加代码内容（截取前 500 字符，避免超长）
        text.push('\n');
        if self.content.len() > 500 {
            text.push_str(&self.content[..500]);
        } else {
            text.push_str(&self.content);
        }
        text
    }
}

/// 搜索结果 — 包含匹配的代码块和相关度信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// 匹配的代码块
    pub block: CodeBlock,
    /// BM25 相关度分数
    pub score: f64,
    /// 上下文代码（根据 context 参数生成）
    pub context_code: Option<String>,
}
