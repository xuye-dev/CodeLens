use thiserror::Error;

/// CodeLens 全局统一错误类型
#[derive(Debug, Error)]
pub enum CodeLensError {
    /// IO 操作错误（文件读取、目录扫描等）
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    /// 代码解析错误
    #[error("解析错误: {path}: {message}")]
    Parse { path: String, message: String },

    /// 索引操作错误
    #[error("索引错误: {0}")]
    Index(String),

    /// XML 解析错误
    #[error("XML 解析错误: {0}")]
    Xml(#[from] quick_xml::Error),

    /// 文件监听错误
    #[error("文件监听错误: {0}")]
    Watcher(#[from] notify::Error),
}

/// 便捷类型别名
pub type Result<T> = std::result::Result<T, CodeLensError>;
