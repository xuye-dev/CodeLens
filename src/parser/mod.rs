pub mod java;
pub mod js;
pub mod xml;

use crate::error::Result;
use crate::models::CodeBlock;
use std::path::Path;

/// 语言解析器统一 trait
///
/// 每种语言实现各自的解析器，通过此 trait 提供统一接口。
pub trait Parser: Send + Sync {
    /// 解析指定文件，提取结构化代码块
    fn parse(&self, file_path: &Path) -> Result<Vec<CodeBlock>>;

    /// 返回该解析器支持的文件扩展名列表（不含点号）
    fn supported_extensions(&self) -> &[&str];
}

/// 根据文件扩展名选择对应的解析器
pub fn get_parser_for_file(file_path: &Path, parsers: &[Box<dyn Parser>]) -> Option<usize> {
    let ext = file_path.extension()?.to_str()?;
    parsers
        .iter()
        .position(|p| p.supported_extensions().contains(&ext))
}

/// 创建所有内置解析器
pub fn create_parsers() -> Vec<Box<dyn Parser>> {
    vec![
        Box::new(java::JavaParser::new()),
        Box::new(js::JsParser::new()),
        Box::new(xml::XmlParser::new()),
    ]
}
