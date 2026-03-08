use crate::error::{CodeLensError, Result};
use crate::models::{BlockKind, CodeBlock};
use crate::parser::Parser;
use std::fs;
use std::path::Path;
use tree_sitter::{Node, Tree};

/// Vue 单文件组件（SFC）解析器
///
/// 解析 `.vue` 文件，提取 `<script>` / `<script setup>` 中的 JS/TS 代码块，
/// 以及 `<template>` 区块。`<script>` 内容委托 tree-sitter JS/TS 解析。
/// SFC 区块提取结果
struct SfcBlock {
    content: String,
    start_line: usize,
    end_line: usize,
    is_typescript: bool,
}

/// Vue 单文件组件（SFC）解析器
pub struct VueParser {
    js_language: tree_sitter::Language,
    ts_language: tree_sitter::Language,
}

impl VueParser {
    pub fn new() -> Self {
        Self {
            js_language: tree_sitter_javascript::LANGUAGE.into(),
            ts_language: tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        }
    }

    /// 解析 Vue SFC 文件，返回所有代码块
    fn parse_vue(&self, file_path: &Path) -> Result<Vec<CodeBlock>> {
        let source = fs::read_to_string(file_path).map_err(CodeLensError::Io)?;
        let file_path_str = file_path.to_string_lossy().to_string();
        let mut blocks = Vec::new();

        // 提取 <template> 区块
        if let Some(template) = self.extract_sfc_block(&source, "template") {
            blocks.push(CodeBlock {
                file_path: file_path_str.clone(),
                start_line: template.start_line,
                end_line: template.end_line,
                content: template.content,
                language: "vue".to_string(),
                kind: BlockKind::Other,
                name: "template".to_string(),
                parent: None,
                signature: None,
                annotations: Vec::new(),
                dependencies: Vec::new(),
            });
        }

        // 提取 <script> 或 <script setup> 区块并用 tree-sitter 解析
        for tag in ["script", "script setup"] {
            if let Some(script) = self.extract_sfc_block(&source, tag) {
                let is_ts = script.is_typescript;
                let is_setup = tag == "script setup";
                let lang_label = "vue";

                // 用 tree-sitter 解析脚本内容
                let language = if is_ts {
                    &self.ts_language
                } else {
                    &self.js_language
                };
                let mut parser = tree_sitter::Parser::new();
                parser
                    .set_language(language)
                    .map_err(|e| CodeLensError::Parse {
                        path: file_path_str.clone(),
                        message: format!("设置 Vue script 语言失败: {e}"),
                    })?;

                if let Some(tree) = parser.parse(&script.content, None) {
                    let script_blocks = self.extract_blocks(
                        &tree,
                        &script.content,
                        &file_path_str,
                        lang_label,
                        script.start_line,
                        is_setup,
                    );
                    blocks.extend(script_blocks);
                }
            }
        }

        Ok(blocks)
    }

    /// 从 SFC 源码中提取指定区块（template / script / script setup）
    fn extract_sfc_block(&self, source: &str, tag: &str) -> Option<SfcBlock> {
        // 构造开标签匹配模式
        let open_prefix = format!("<{}", tag);
        let close_tag = if tag == "script setup" {
            "</script>"
        } else {
            &format!("</{tag}>")
        };

        // 查找开标签位置，需要处理 <script> vs <script setup> 的歧义
        let mut search_from = 0;
        let (open_start, open_end) = loop {
            let pos = source[search_from..].find(&open_prefix)? + search_from;
            let end = source[pos..].find('>')? + pos;
            let open_tag_content = &source[pos..=end];

            if tag == "script" {
                // 搜索 <script> 时，需要排除 <script setup>
                if open_tag_content.contains("setup") {
                    search_from = end + 1;
                    continue;
                }
            }
            break (pos, end);
        };
        let content_start = open_end + 1;

        // 检查 lang="ts" 属性
        let open_tag = &source[open_start..=open_end];
        let is_typescript = open_tag.contains("lang=\"ts\"") || open_tag.contains("lang='ts'");

        // 查找闭标签
        let close_start = source[content_start..].find(close_tag)? + content_start;

        let content = source[content_start..close_start].to_string();

        // 计算行号
        let start_line = source[..content_start].matches('\n').count() + 1;
        let end_line = source[..close_start].matches('\n').count() + 1;

        Some(SfcBlock {
            content,
            start_line,
            end_line,
            is_typescript,
        })
    }

    /// 从语法树提取代码块，行号偏移到 Vue 文件中的实际位置
    fn extract_blocks(
        &self,
        tree: &Tree,
        source: &str,
        file_path: &str,
        lang_label: &str,
        line_offset: usize,
        is_setup: bool,
    ) -> Vec<CodeBlock> {
        let mut blocks = Vec::new();
        self.visit_node(
            tree.root_node(),
            source,
            file_path,
            lang_label,
            line_offset,
            &mut blocks,
            None,
            is_setup,
        );
        blocks
    }

    /// 递归遍历语法树节点
    fn visit_node(
        &self,
        node: Node,
        source: &str,
        file_path: &str,
        lang_label: &str,
        line_offset: usize,
        blocks: &mut Vec<CodeBlock>,
        parent_name: Option<&str>,
        is_setup: bool,
    ) {
        match node.kind() {
            "import_statement" => {
                if let Some(block) =
                    self.extract_import(node, source, file_path, lang_label, line_offset)
                {
                    blocks.push(block);
                }
                return;
            }

            "class_declaration" => {
                if let Some(block) = self.extract_class(
                    node,
                    source,
                    file_path,
                    lang_label,
                    line_offset,
                    BlockKind::Class,
                ) {
                    let class_name = block.name.clone();
                    blocks.push(block);
                    if let Some(body) = node.child_by_field_name("body") {
                        self.visit_children(
                            body,
                            source,
                            file_path,
                            lang_label,
                            line_offset,
                            blocks,
                            Some(&class_name),
                            is_setup,
                        );
                    }
                    return;
                }
            }

            "interface_declaration" => {
                if let Some(block) = self.extract_class(
                    node,
                    source,
                    file_path,
                    lang_label,
                    line_offset,
                    BlockKind::Interface,
                ) {
                    let name = block.name.clone();
                    blocks.push(block);
                    if let Some(body) = node.child_by_field_name("body") {
                        self.visit_children(
                            body,
                            source,
                            file_path,
                            lang_label,
                            line_offset,
                            blocks,
                            Some(&name),
                            is_setup,
                        );
                    }
                    return;
                }
            }

            "enum_declaration" => {
                if let Some(block) = self.extract_class(
                    node,
                    source,
                    file_path,
                    lang_label,
                    line_offset,
                    BlockKind::Enum,
                ) {
                    blocks.push(block);
                    return;
                }
            }

            "method_definition" => {
                if let Some(block) = self.extract_method(
                    node,
                    source,
                    file_path,
                    lang_label,
                    line_offset,
                    parent_name,
                ) {
                    blocks.push(block);
                }
                return;
            }

            "function_declaration" | "generator_function_declaration" => {
                if let Some(block) = self.extract_function(
                    node,
                    source,
                    file_path,
                    lang_label,
                    line_offset,
                    parent_name,
                ) {
                    blocks.push(block);
                }
                return;
            }

            "export_statement" => {
                self.handle_export(
                    node,
                    source,
                    file_path,
                    lang_label,
                    line_offset,
                    blocks,
                    parent_name,
                    is_setup,
                );
                return;
            }

            "lexical_declaration" | "variable_declaration" => {
                self.handle_variable_declaration(
                    node,
                    source,
                    file_path,
                    lang_label,
                    line_offset,
                    blocks,
                    parent_name,
                );
                return;
            }

            // <script setup> 中的特殊宏调用：defineProps / defineEmits / defineExpose
            "expression_statement" if is_setup => {
                self.handle_setup_macro(node, source, file_path, lang_label, line_offset, blocks);
            }

            _ => {}
        }

        self.visit_children(
            node,
            source,
            file_path,
            lang_label,
            line_offset,
            blocks,
            parent_name,
            is_setup,
        );
    }

    /// 遍历子节点
    fn visit_children(
        &self,
        node: Node,
        source: &str,
        file_path: &str,
        lang_label: &str,
        line_offset: usize,
        blocks: &mut Vec<CodeBlock>,
        parent_name: Option<&str>,
        is_setup: bool,
    ) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit_node(
                child,
                source,
                file_path,
                lang_label,
                line_offset,
                blocks,
                parent_name,
                is_setup,
            );
        }
    }

    /// 提取 import 语句
    fn extract_import(
        &self,
        node: Node,
        source: &str,
        file_path: &str,
        lang_label: &str,
        line_offset: usize,
    ) -> Option<CodeBlock> {
        let content = node_text(node, source);
        let name = node
            .child_by_field_name("source")
            .map(|n| node_text(n, source))
            .map(|s| s.trim_matches(|c| c == '\'' || c == '"').to_string())
            .unwrap_or_else(|| content.clone());

        Some(CodeBlock {
            file_path: file_path.to_string(),
            start_line: node.start_position().row + line_offset,
            end_line: node.end_position().row + line_offset,
            content,
            language: lang_label.to_string(),
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
        lang_label: &str,
        line_offset: usize,
        kind: BlockKind,
    ) -> Option<CodeBlock> {
        let name = node
            .child_by_field_name("name")
            .map(|n| node_text(n, source))
            .unwrap_or_default();
        let content = node_text(node, source);
        let signature = self.extract_signature_before_body(node, source);

        let mut dependencies = Vec::new();
        if let Some(heritage) = find_child_by_kind(node, "class_heritage") {
            dependencies.push(node_text(heritage, source));
        }
        if let Some(extends) = find_child_by_kind(node, "extends_clause") {
            dependencies.push(node_text(extends, source));
        }

        Some(CodeBlock {
            file_path: file_path.to_string(),
            start_line: node.start_position().row + line_offset,
            end_line: node.end_position().row + line_offset,
            content,
            language: lang_label.to_string(),
            kind,
            name,
            parent: None,
            signature: Some(signature),
            annotations: Vec::new(),
            dependencies,
        })
    }

    /// 提取方法定义
    fn extract_method(
        &self,
        node: Node,
        source: &str,
        file_path: &str,
        lang_label: &str,
        line_offset: usize,
        parent_name: Option<&str>,
    ) -> Option<CodeBlock> {
        let name = node
            .child_by_field_name("name")
            .map(|n| node_text(n, source))
            .unwrap_or_default();
        let content = node_text(node, source);
        let signature = self.extract_function_signature(node, source);

        Some(CodeBlock {
            file_path: file_path.to_string(),
            start_line: node.start_position().row + line_offset,
            end_line: node.end_position().row + line_offset,
            content,
            language: lang_label.to_string(),
            kind: BlockKind::Method,
            name,
            parent: parent_name.map(|s| s.to_string()),
            signature: Some(signature),
            annotations: Vec::new(),
            dependencies: Vec::new(),
        })
    }

    /// 提取函数声明
    fn extract_function(
        &self,
        node: Node,
        source: &str,
        file_path: &str,
        lang_label: &str,
        line_offset: usize,
        parent_name: Option<&str>,
    ) -> Option<CodeBlock> {
        let name = node
            .child_by_field_name("name")
            .map(|n| node_text(n, source))
            .unwrap_or_default();
        let content = node_text(node, source);
        let signature = self.extract_function_signature(node, source);

        Some(CodeBlock {
            file_path: file_path.to_string(),
            start_line: node.start_position().row + line_offset,
            end_line: node.end_position().row + line_offset,
            content,
            language: lang_label.to_string(),
            kind: BlockKind::Method,
            name,
            parent: parent_name.map(|s| s.to_string()),
            signature: Some(signature),
            annotations: Vec::new(),
            dependencies: Vec::new(),
        })
    }

    /// 提取函数/方法签名（不含函数体）
    fn extract_function_signature(&self, node: Node, source: &str) -> String {
        let mut sig = String::new();
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            let kind = child.kind();
            if kind == "statement_block" || kind == "block" {
                break;
            }
            if kind == "decorator" {
                continue;
            }
            if !sig.is_empty() {
                sig.push(' ');
            }
            sig.push_str(&node_text(child, source));
        }
        sig
    }

    /// 提取签名（body 之前的部分）
    fn extract_signature_before_body(&self, node: Node, source: &str) -> String {
        let mut sig = String::new();
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            let kind = child.kind();
            if kind == "class_body"
                || kind == "interface_body"
                || kind == "object_type"
                || kind == "enum_body"
            {
                break;
            }
            if kind == "decorator" {
                continue;
            }
            if !sig.is_empty() {
                sig.push(' ');
            }
            sig.push_str(&node_text(child, source));
        }
        sig
    }

    /// 处理 export 语句
    fn handle_export(
        &self,
        node: Node,
        source: &str,
        file_path: &str,
        lang_label: &str,
        line_offset: usize,
        blocks: &mut Vec<CodeBlock>,
        parent_name: Option<&str>,
        is_setup: bool,
    ) {
        let mut has_declaration = false;
        let mut cursor = node.walk();
        let export_start_line = node.start_position().row + line_offset;
        let export_content = node_text(node, source);

        for child in node.children(&mut cursor) {
            match child.kind() {
                "class_declaration" => {
                    if let Some(mut block) = self.extract_class(
                        child,
                        source,
                        file_path,
                        lang_label,
                        line_offset,
                        BlockKind::Class,
                    ) {
                        block.start_line = export_start_line;
                        block.content = export_content.clone();
                        let class_name = block.name.clone();
                        blocks.push(block);
                        if let Some(body) = child.child_by_field_name("body") {
                            self.visit_children(
                                body,
                                source,
                                file_path,
                                lang_label,
                                line_offset,
                                blocks,
                                Some(&class_name),
                                is_setup,
                            );
                        }
                        has_declaration = true;
                    }
                }
                "interface_declaration" => {
                    if let Some(mut block) = self.extract_class(
                        child,
                        source,
                        file_path,
                        lang_label,
                        line_offset,
                        BlockKind::Interface,
                    ) {
                        block.start_line = export_start_line;
                        block.content = export_content.clone();
                        blocks.push(block);
                        has_declaration = true;
                    }
                }
                "enum_declaration" => {
                    if let Some(mut block) = self.extract_class(
                        child,
                        source,
                        file_path,
                        lang_label,
                        line_offset,
                        BlockKind::Enum,
                    ) {
                        block.start_line = export_start_line;
                        block.content = export_content.clone();
                        blocks.push(block);
                        has_declaration = true;
                    }
                }
                "function_declaration" | "generator_function_declaration" => {
                    if let Some(mut block) = self.extract_function(
                        child,
                        source,
                        file_path,
                        lang_label,
                        line_offset,
                        parent_name,
                    ) {
                        block.start_line = export_start_line;
                        block.content = export_content.clone();
                        blocks.push(block);
                        has_declaration = true;
                    }
                }
                "lexical_declaration" | "variable_declaration" => {
                    let before = blocks.len();
                    self.handle_variable_declaration(
                        child,
                        source,
                        file_path,
                        lang_label,
                        line_offset,
                        blocks,
                        parent_name,
                    );
                    for b in &mut blocks[before..] {
                        b.start_line = export_start_line;
                        b.content = export_content.clone();
                    }
                    has_declaration = true;
                }
                _ => {}
            }
        }

        if !has_declaration {
            let content = node_text(node, source);
            blocks.push(CodeBlock {
                file_path: file_path.to_string(),
                start_line: node.start_position().row + line_offset,
                end_line: node.end_position().row + line_offset,
                content,
                language: lang_label.to_string(),
                kind: BlockKind::Other,
                name: "export".to_string(),
                parent: None,
                signature: None,
                annotations: Vec::new(),
                dependencies: Vec::new(),
            });
        }
    }

    /// 处理变量声明（提取箭头函数赋值 + Vue 编译器宏赋值）
    fn handle_variable_declaration(
        &self,
        node: Node,
        source: &str,
        file_path: &str,
        lang_label: &str,
        line_offset: usize,
        blocks: &mut Vec<CodeBlock>,
        parent_name: Option<&str>,
    ) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() != "variable_declarator" {
                continue;
            }

            let value = match child.child_by_field_name("value") {
                Some(v) => v,
                None => continue,
            };

            // 检查是否是 Vue 编译器宏赋值（如 const emit = defineEmits(...)）
            if value.kind() == "call_expression" {
                if let Some(macro_name) = self.detect_vue_macro(value, source) {
                    let content = node_text(node, source);
                    blocks.push(CodeBlock {
                        file_path: file_path.to_string(),
                        start_line: node.start_position().row + line_offset,
                        end_line: node.end_position().row + line_offset,
                        content: content.clone(),
                        language: lang_label.to_string(),
                        kind: BlockKind::Other,
                        name: macro_name,
                        parent: None,
                        signature: Some(content.trim().trim_end_matches(';').to_string()),
                        annotations: Vec::new(),
                        dependencies: Vec::new(),
                    });
                    continue;
                }
            }

            if value.kind() != "arrow_function" && value.kind() != "function_expression" {
                continue;
            }

            let name = child
                .child_by_field_name("name")
                .map(|n| node_text(n, source))
                .unwrap_or_default();

            let content = node_text(node, source);
            let signature = self.extract_arrow_signature(child, source);

            blocks.push(CodeBlock {
                file_path: file_path.to_string(),
                start_line: node.start_position().row + line_offset,
                end_line: node.end_position().row + line_offset,
                content,
                language: lang_label.to_string(),
                kind: BlockKind::Method,
                name,
                parent: parent_name.map(|s| s.to_string()),
                signature: Some(signature),
                annotations: Vec::new(),
                dependencies: Vec::new(),
            });
        }
    }

    /// 提取箭头函数签名
    fn extract_arrow_signature(&self, declarator: Node, source: &str) -> String {
        let mut sig = String::new();

        if let Some(name) = declarator.child_by_field_name("name") {
            sig.push_str(&node_text(name, source));
        }

        if let Some(type_ann) = declarator.child_by_field_name("type") {
            sig.push_str(": ");
            sig.push_str(&node_text(type_ann, source));
        }

        if let Some(value) = declarator.child_by_field_name("value") {
            if let Some(params) = value.child_by_field_name("parameters") {
                sig.push_str(" = ");
                sig.push_str(&node_text(params, source));
                sig.push_str(" => ...");
            }
        }

        sig
    }

    /// 检测 call_expression 是否是 Vue 编译器宏调用，返回宏名称
    fn detect_vue_macro(&self, call_node: Node, source: &str) -> Option<String> {
        let macros = ["defineProps", "defineEmits", "defineExpose", "defineSlots"];
        let func = call_node.child_by_field_name("function")?;
        let func_text = node_text(func, source);
        // 直接调用：defineEmits(...) 或 withDefaults(defineProps(...), ...)
        for macro_name in macros {
            if func_text == macro_name {
                return Some(macro_name.to_string());
            }
        }
        // withDefaults(defineProps<T>(), { ... })
        if func_text == "withDefaults" {
            return Some("defineProps".to_string());
        }
        None
    }

    /// 处理 <script setup> 中的 Vue 编译器宏（defineProps / defineEmits / defineExpose）
    fn handle_setup_macro(
        &self,
        node: Node,
        source: &str,
        file_path: &str,
        lang_label: &str,
        line_offset: usize,
        blocks: &mut Vec<CodeBlock>,
    ) {
        let content = node_text(node, source);
        let macros = ["defineProps", "defineEmits", "defineExpose", "defineSlots"];
        for macro_name in macros {
            if content.contains(macro_name) {
                blocks.push(CodeBlock {
                    file_path: file_path.to_string(),
                    start_line: node.start_position().row + line_offset,
                    end_line: node.end_position().row + line_offset,
                    content: content.clone(),
                    language: lang_label.to_string(),
                    kind: BlockKind::Other,
                    name: macro_name.to_string(),
                    parent: None,
                    signature: Some(content.trim().trim_end_matches(';').to_string()),
                    annotations: Vec::new(),
                    dependencies: Vec::new(),
                });
                return;
            }
        }
    }
}

impl Parser for VueParser {
    fn parse(&self, file_path: &Path) -> Result<Vec<CodeBlock>> {
        self.parse_vue(file_path)
    }

    fn supported_extensions(&self) -> &[&str] {
        &["vue"]
    }
}

/// 获取节点对应的源代码文本
fn node_text(node: Node, source: &str) -> String {
    source[node.byte_range()].to_string()
}

/// 按类型查找第一个子节点
fn find_child_by_kind<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    let result = node
        .children(&mut cursor)
        .find(|child| child.kind() == kind);
    result
}
