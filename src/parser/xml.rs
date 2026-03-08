use crate::error::{CodeLensError, Result};
use crate::models::{BlockKind, CodeBlock};
use crate::parser::Parser;
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use std::fs;
use std::path::Path;

/// MyBatis SQL 语句类型
const MYBATIS_SQL_TAGS: &[&str] = &["select", "insert", "update", "delete"];

/// XML 解析器 — 支持 MyBatis Mapper XML 和通用 XML 配置文件
pub struct XmlParser;

impl XmlParser {
    pub fn new() -> Self {
        Self
    }

    /// 解析 XML 文件内容
    fn parse_xml(&self, source: &str, file_path: &str) -> Result<Vec<CodeBlock>> {
        let mut blocks = Vec::new();
        let lines: Vec<&str> = source.lines().collect();

        // 判断是否为 MyBatis Mapper 文件
        let is_mybatis = source.contains("<mapper") && source.contains("namespace");

        if is_mybatis {
            self.parse_mybatis_mapper(source, file_path, &lines, &mut blocks)?;
        } else {
            self.parse_generic_xml(source, file_path, &lines, &mut blocks)?;
        }

        Ok(blocks)
    }

    /// 解析 MyBatis Mapper XML
    fn parse_mybatis_mapper(
        &self,
        source: &str,
        file_path: &str,
        lines: &[&str],
        blocks: &mut Vec<CodeBlock>,
    ) -> Result<()> {
        let mut reader = Reader::from_str(source);

        let mut namespace = String::new();
        let mut current_tag: Option<String> = None;
        let mut current_id = String::new();
        let mut current_attrs = String::new();
        let mut current_content = String::new();
        let mut tag_start_line: usize = 0;

        loop {
            match reader.read_event() {
                Ok(Event::Start(ref e)) => {
                    let tag_name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    let byte_pos = reader.buffer_position();
                    let line = byte_offset_to_line(source, byte_pos);

                    if tag_name == "mapper" {
                        // 提取 namespace
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"namespace" {
                                namespace = String::from_utf8_lossy(&attr.value).to_string();
                                blocks.push(CodeBlock {
                                    file_path: file_path.to_string(),
                                    start_line: line,
                                    end_line: line,
                                    content: format!("<mapper namespace=\"{namespace}\">"),
                                    language: "xml".to_string(),
                                    kind: BlockKind::XmlNamespace,
                                    name: namespace.clone(),
                                    parent: None,
                                    signature: Some(format!("namespace: {namespace}")),
                                    annotations: Vec::new(),
                                    dependencies: Vec::new(),
                                });
                            }
                        }
                    } else if MYBATIS_SQL_TAGS.contains(&tag_name.as_str()) {
                        current_tag = Some(tag_name.clone());
                        tag_start_line = line;
                        current_id.clear();
                        current_attrs.clear();

                        // 收集属性
                        let mut attr_parts = Vec::new();
                        for attr in e.attributes().flatten() {
                            let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                            let val = String::from_utf8_lossy(&attr.value).to_string();
                            if key == "id" {
                                current_id = val.clone();
                            }
                            attr_parts.push(format!("{key}=\"{val}\""));
                        }
                        current_attrs = attr_parts.join(" ");
                        current_content.clear();
                    }
                }
                Ok(Event::Text(ref e)) => {
                    if current_tag.is_some() {
                        let text = e.unescape().unwrap_or_default().to_string();
                        current_content.push_str(&text);
                    }
                }
                Ok(Event::End(ref e)) => {
                    let tag_name = String::from_utf8_lossy(e.name().as_ref()).to_string();

                    if current_tag.as_deref() == Some(&tag_name) {
                        let byte_pos = reader.buffer_position();
                        let end_line = byte_offset_to_line(source, byte_pos);

                        // 从源文件中提取原始内容
                        let raw_content = extract_lines(lines, tag_start_line, end_line);

                        blocks.push(CodeBlock {
                            file_path: file_path.to_string(),
                            start_line: tag_start_line,
                            end_line,
                            content: raw_content,
                            language: "xml".to_string(),
                            kind: BlockKind::XmlNode,
                            name: current_id.clone(),
                            parent: Some(namespace.clone()),
                            signature: Some(format!("<{tag_name} {}>", current_attrs)),
                            annotations: Vec::new(),
                            dependencies: extract_sql_dependencies(&current_content),
                        });

                        current_tag = None;
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => {
                    return Err(CodeLensError::Parse {
                        path: file_path.to_string(),
                        message: format!("XML 解析错误: {e}"),
                    });
                }
                _ => {}
            }
        }

        Ok(())
    }

    /// 解析通用 XML 配置文件
    fn parse_generic_xml(
        &self,
        source: &str,
        file_path: &str,
        lines: &[&str],
        blocks: &mut Vec<CodeBlock>,
    ) -> Result<()> {
        let mut reader = Reader::from_str(source);

        let mut depth: usize = 0;
        // 只提取顶层和二级元素
        let max_depth: usize = 2;

        loop {
            match reader.read_event() {
                Ok(Event::Start(ref e)) => {
                    depth += 1;
                    if depth <= max_depth {
                        let tag_name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                        let byte_pos = reader.buffer_position();
                        let line = byte_offset_to_line(source, byte_pos);

                        // 收集属性
                        let mut id = String::new();
                        let mut attr_parts = Vec::new();
                        for attr in e.attributes().flatten() {
                            let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                            let val = String::from_utf8_lossy(&attr.value).to_string();
                            if key == "id" || key == "name" {
                                id = val.clone();
                            }
                            attr_parts.push(format!("{key}=\"{val}\""));
                        }

                        let name = if id.is_empty() { tag_name.clone() } else { id };

                        blocks.push(CodeBlock {
                            file_path: file_path.to_string(),
                            start_line: line,
                            end_line: line, // 对于通用 XML，只记录开始行
                            content: extract_lines(lines, line, line),
                            language: "xml".to_string(),
                            kind: BlockKind::XmlNode,
                            name,
                            parent: None,
                            signature: if attr_parts.is_empty() {
                                Some(format!("<{tag_name}>"))
                            } else {
                                Some(format!("<{tag_name} {}>", attr_parts.join(" ")))
                            },
                            annotations: Vec::new(),
                            dependencies: Vec::new(),
                        });
                    }
                }
                Ok(Event::End(_)) => {
                    depth = depth.saturating_sub(1);
                }
                Ok(Event::Empty(ref e)) => {
                    if depth < max_depth {
                        let tag_name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                        let byte_pos = reader.buffer_position();
                        let line = byte_offset_to_line(source, byte_pos);

                        let mut id = String::new();
                        let mut attr_parts = Vec::new();
                        for attr in e.attributes().flatten() {
                            let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                            let val = String::from_utf8_lossy(&attr.value).to_string();
                            if key == "id" || key == "name" {
                                id = val.clone();
                            }
                            attr_parts.push(format!("{key}=\"{val}\""));
                        }

                        let name = if id.is_empty() { tag_name.clone() } else { id };

                        blocks.push(CodeBlock {
                            file_path: file_path.to_string(),
                            start_line: line,
                            end_line: line,
                            content: extract_lines(lines, line, line),
                            language: "xml".to_string(),
                            kind: BlockKind::XmlNode,
                            name,
                            parent: None,
                            signature: if attr_parts.is_empty() {
                                Some(format!("<{tag_name}/>"))
                            } else {
                                Some(format!("<{tag_name} {}/>", attr_parts.join(" ")))
                            },
                            annotations: Vec::new(),
                            dependencies: Vec::new(),
                        });
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => {
                    return Err(CodeLensError::Parse {
                        path: file_path.to_string(),
                        message: format!("XML 解析错误: {e}"),
                    });
                }
                _ => {}
            }
        }

        Ok(())
    }
}

impl Parser for XmlParser {
    fn parse(&self, file_path: &Path) -> Result<Vec<CodeBlock>> {
        let source = fs::read_to_string(file_path).map_err(CodeLensError::Io)?;
        let file_path_str = file_path.to_string_lossy().to_string();
        self.parse_xml(&source, &file_path_str)
    }

    fn supported_extensions(&self) -> &[&str] {
        &["xml"]
    }
}

/// 将字节偏移转换为行号（从 1 开始）
fn byte_offset_to_line(source: &str, byte_offset: u64) -> usize {
    let byte_offset = byte_offset as usize;
    let offset = byte_offset.min(source.len());
    source[..offset].matches('\n').count() + 1
}

/// 从源码行中提取指定行范围的内容
fn extract_lines(lines: &[&str], start: usize, end: usize) -> String {
    let start_idx = start.saturating_sub(1);
    let end_idx = end.min(lines.len());
    if start_idx >= lines.len() {
        return String::new();
    }
    lines[start_idx..end_idx].join("\n")
}

/// 从 SQL 内容中提取表名等依赖信息
fn extract_sql_dependencies(sql: &str) -> Vec<String> {
    let mut deps = Vec::new();
    let sql_upper = sql.to_uppercase();
    let words: Vec<&str> = sql.split_whitespace().collect();
    let upper_words: Vec<String> = words.iter().map(|w| w.to_uppercase()).collect();

    for (i, word) in upper_words.iter().enumerate() {
        if (word == "FROM" || word == "JOIN" || word == "INTO" || word == "UPDATE")
            && i + 1 < words.len()
        {
            let table =
                words[i + 1].trim_matches(|c: char| !c.is_alphanumeric() && c != '_' && c != '.');
            if !table.is_empty() && !is_sql_keyword(&table.to_uppercase()) {
                deps.push(table.to_string());
            }
        }
    }

    // 提取 #{} 参数引用
    let mut rest = sql.as_bytes();
    while let Some(pos) = find_subsequence(rest, b"#{") {
        rest = &rest[pos + 2..];
        if let Some(end) = find_subsequence(rest, b"}") {
            let param = String::from_utf8_lossy(&rest[..end]).to_string();
            if !param.is_empty() {
                deps.push(format!("param:{param}"));
            }
            rest = &rest[end + 1..];
        }
    }

    let _ = sql_upper; // suppress unused warning
    deps
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn is_sql_keyword(word: &str) -> bool {
    matches!(
        word,
        "SELECT"
            | "FROM"
            | "WHERE"
            | "AND"
            | "OR"
            | "INSERT"
            | "INTO"
            | "UPDATE"
            | "SET"
            | "DELETE"
            | "VALUES"
            | "JOIN"
            | "LEFT"
            | "RIGHT"
            | "INNER"
            | "OUTER"
            | "ON"
            | "AS"
            | "IN"
            | "NOT"
            | "NULL"
            | "IS"
            | "LIKE"
            | "ORDER"
            | "BY"
            | "GROUP"
            | "HAVING"
            | "LIMIT"
            | "OFFSET"
            | "UNION"
            | "ALL"
            | "DISTINCT"
            | "EXISTS"
            | "BETWEEN"
            | "CASE"
            | "WHEN"
            | "THEN"
            | "ELSE"
            | "END"
    )
}
