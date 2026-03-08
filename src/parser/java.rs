use crate::error::{CodeLensError, Result};
use crate::models::{BlockKind, CodeBlock};
use crate::parser::Parser;
use std::fs;
use std::path::Path;
use tree_sitter::{Node, Tree};

/// Java 语言解析器 — 使用 tree-sitter 解析 .java 文件
pub struct JavaParser {
    language: tree_sitter::Language,
}

impl JavaParser {
    pub fn new() -> Self {
        Self {
            language: tree_sitter_java::LANGUAGE.into(),
        }
    }

    /// 解析语法树，提取代码块
    fn extract_blocks(&self, tree: &Tree, source: &str, file_path: &str) -> Vec<CodeBlock> {
        let mut blocks = Vec::new();
        let lines: Vec<&str> = source.lines().collect();

        self.visit_node(
            tree.root_node(),
            source,
            file_path,
            &lines,
            &mut blocks,
            None,
        );

        blocks
    }

    /// 递归遍历语法树节点
    fn visit_node(
        &self,
        node: Node,
        source: &str,
        file_path: &str,
        lines: &[&str],
        blocks: &mut Vec<CodeBlock>,
        parent_name: Option<&str>,
    ) {
        match node.kind() {
            "import_declaration" => {
                if let Some(block) = self.extract_import(node, source, file_path) {
                    blocks.push(block);
                }
            }
            "class_declaration" => {
                if let Some(block) =
                    self.extract_class(node, source, file_path, lines, BlockKind::Class)
                {
                    let class_name = block.name.clone();
                    blocks.push(block);
                    // 递归处理类内部成员
                    if let Some(body) = node.child_by_field_name("body") {
                        self.visit_children(
                            body,
                            source,
                            file_path,
                            lines,
                            blocks,
                            Some(&class_name),
                        );
                    }
                    return;
                }
            }
            "interface_declaration" => {
                if let Some(block) =
                    self.extract_class(node, source, file_path, lines, BlockKind::Interface)
                {
                    let name = block.name.clone();
                    blocks.push(block);
                    if let Some(body) = node.child_by_field_name("body") {
                        self.visit_children(body, source, file_path, lines, blocks, Some(&name));
                    }
                    return;
                }
            }
            "enum_declaration" => {
                if let Some(block) =
                    self.extract_class(node, source, file_path, lines, BlockKind::Enum)
                {
                    let name = block.name.clone();
                    blocks.push(block);
                    if let Some(body) = node.child_by_field_name("body") {
                        self.visit_children(body, source, file_path, lines, blocks, Some(&name));
                    }
                    return;
                }
            }
            "method_declaration" => {
                if let Some(block) =
                    self.extract_method(node, source, file_path, lines, parent_name)
                {
                    blocks.push(block);
                }
                return;
            }
            "constructor_declaration" => {
                if let Some(block) =
                    self.extract_constructor(node, source, file_path, lines, parent_name)
                {
                    blocks.push(block);
                }
                return;
            }
            "field_declaration" => {
                if let Some(block) = self.extract_field(node, source, file_path, lines, parent_name)
                {
                    blocks.push(block);
                }
                return;
            }
            _ => {}
        }

        self.visit_children(node, source, file_path, lines, blocks, parent_name);
    }

    /// 遍历子节点
    fn visit_children(
        &self,
        node: Node,
        source: &str,
        file_path: &str,
        lines: &[&str],
        blocks: &mut Vec<CodeBlock>,
        parent_name: Option<&str>,
    ) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit_node(child, source, file_path, lines, blocks, parent_name);
        }
    }

    /// 提取 import 语句
    fn extract_import(&self, node: Node, source: &str, file_path: &str) -> Option<CodeBlock> {
        let content = node_text(node, source);
        let name = content
            .strip_prefix("import ")
            .and_then(|s| s.strip_suffix(';'))
            .unwrap_or(&content)
            .trim()
            .to_string();

        Some(CodeBlock {
            file_path: file_path.to_string(),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            content,
            language: "java".to_string(),
            kind: BlockKind::Import,
            name,
            parent: None,
            signature: None,
            annotations: Vec::new(),
            dependencies: Vec::new(),
        })
    }

    /// 提取类/接口/枚举定义
    fn extract_class(
        &self,
        node: Node,
        source: &str,
        file_path: &str,
        _lines: &[&str],
        kind: BlockKind,
    ) -> Option<CodeBlock> {
        let name = node
            .child_by_field_name("name")
            .map(|n| node_text(n, source))
            .unwrap_or_default();

        let content = node_text(node, source);
        let annotations = self.collect_annotations(node, source);

        // 提取签名（不含方法体）
        let signature = self.extract_class_signature(node, source);

        // 提取父类/接口依赖
        let mut dependencies = Vec::new();
        if let Some(superclass) = node.child_by_field_name("superclass") {
            dependencies.push(node_text(superclass, source));
        }
        if let Some(interfaces) = node.child_by_field_name("interfaces") {
            dependencies.push(node_text(interfaces, source));
        }

        Some(CodeBlock {
            file_path: file_path.to_string(),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            content,
            language: "java".to_string(),
            kind,
            name,
            parent: None,
            signature: Some(signature),
            annotations,
            dependencies,
        })
    }

    /// 提取类签名（不含方法体内容）
    fn extract_class_signature(&self, node: Node, source: &str) -> String {
        let mut sig = String::new();
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "class_body"
                || child.kind() == "interface_body"
                || child.kind() == "enum_body"
            {
                break;
            }
            if !sig.is_empty() {
                sig.push(' ');
            }
            sig.push_str(&node_text(child, source));
        }
        sig
    }

    /// 提取方法定义
    fn extract_method(
        &self,
        node: Node,
        source: &str,
        file_path: &str,
        _lines: &[&str],
        parent_name: Option<&str>,
    ) -> Option<CodeBlock> {
        let name = node
            .child_by_field_name("name")
            .map(|n| node_text(n, source))
            .unwrap_or_default();

        let content = node_text(node, source);
        let annotations = self.collect_annotations(node, source);
        let signature = self.extract_method_signature(node, source);

        Some(CodeBlock {
            file_path: file_path.to_string(),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            content,
            language: "java".to_string(),
            kind: BlockKind::Method,
            name,
            parent: parent_name.map(|s| s.to_string()),
            signature: Some(signature),
            annotations,
            dependencies: Vec::new(),
        })
    }

    /// 提取方法签名（不含方法体）
    fn extract_method_signature(&self, node: Node, source: &str) -> String {
        let mut sig = String::new();
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "block" {
                break;
            }
            if !sig.is_empty() {
                sig.push(' ');
            }
            sig.push_str(&node_text(child, source));
        }
        sig
    }

    /// 提取构造函数
    fn extract_constructor(
        &self,
        node: Node,
        source: &str,
        file_path: &str,
        _lines: &[&str],
        parent_name: Option<&str>,
    ) -> Option<CodeBlock> {
        let name = node
            .child_by_field_name("name")
            .map(|n| node_text(n, source))
            .unwrap_or_default();

        let content = node_text(node, source);
        let annotations = self.collect_annotations(node, source);
        let signature = self.extract_method_signature(node, source);

        Some(CodeBlock {
            file_path: file_path.to_string(),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            content,
            language: "java".to_string(),
            kind: BlockKind::Constructor,
            name,
            parent: parent_name.map(|s| s.to_string()),
            signature: Some(signature),
            annotations,
            dependencies: Vec::new(),
        })
    }

    /// 提取字段定义
    fn extract_field(
        &self,
        node: Node,
        source: &str,
        file_path: &str,
        _lines: &[&str],
        parent_name: Option<&str>,
    ) -> Option<CodeBlock> {
        let content = node_text(node, source);

        // 从字段声明中提取变量名
        let name = node
            .child_by_field_name("declarator")
            .and_then(|d| d.child_by_field_name("name"))
            .map(|n| node_text(n, source))
            .unwrap_or_else(|| {
                // 回退：遍历子节点找 variable_declarator
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "variable_declarator" {
                        if let Some(name_node) = child.child_by_field_name("name") {
                            return node_text(name_node, source);
                        }
                    }
                }
                String::new()
            });

        let annotations = self.collect_annotations(node, source);

        let signature = content.trim_end_matches(';').trim().to_string();

        Some(CodeBlock {
            file_path: file_path.to_string(),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            content,
            language: "java".to_string(),
            kind: BlockKind::Field,
            name,
            parent: parent_name.map(|s| s.to_string()),
            signature: Some(signature),
            annotations,
            dependencies: Vec::new(),
        })
    }

    /// 收集节点前面的注解
    fn collect_annotations(&self, node: Node, source: &str) -> Vec<String> {
        let mut annotations = Vec::new();

        // 检查前一个兄弟节点是否为注解
        let mut prev = node.prev_sibling();
        while let Some(sibling) = prev {
            if sibling.kind() == "marker_annotation" || sibling.kind() == "annotation" {
                annotations.push(node_text(sibling, source));
            } else {
                break;
            }
            prev = sibling.prev_sibling();
        }

        // 也检查当前节点的子节点中是否有 modifiers 包含注解
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "modifiers" {
                let mut mod_cursor = child.walk();
                for mod_child in child.children(&mut mod_cursor) {
                    if mod_child.kind() == "marker_annotation" || mod_child.kind() == "annotation" {
                        annotations.push(node_text(mod_child, source));
                    }
                }
            }
        }

        annotations
    }
}

impl Parser for JavaParser {
    fn parse(&self, file_path: &Path) -> Result<Vec<CodeBlock>> {
        let source = fs::read_to_string(file_path).map_err(CodeLensError::Io)?;

        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&self.language)
            .map_err(|e| CodeLensError::Parse {
                path: file_path.display().to_string(),
                message: format!("设置 Java 语言失败: {e}"),
            })?;

        let tree = parser
            .parse(&source, None)
            .ok_or_else(|| CodeLensError::Parse {
                path: file_path.display().to_string(),
                message: "解析 Java 文件失败".to_string(),
            })?;

        let file_path_str = file_path.to_string_lossy().to_string();
        Ok(self.extract_blocks(&tree, &source, &file_path_str))
    }

    fn supported_extensions(&self) -> &[&str] {
        &["java"]
    }
}

/// 获取节点对应的源代码文本
fn node_text(node: Node, source: &str) -> String {
    source[node.byte_range()].to_string()
}
