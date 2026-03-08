use crate::error::{CodeLensError, Result};
use crate::models::{BlockKind, CodeBlock};
use crate::parser::Parser;
use std::fs;
use std::path::Path;
use tree_sitter::{Node, Tree};

/// JavaScript/TypeScript 语言解析器 — 使用 tree-sitter 解析 .js/.jsx/.ts/.tsx 文件
pub struct JsParser {
    js_language: tree_sitter::Language,
    ts_language: tree_sitter::Language,
    tsx_language: tree_sitter::Language,
}

impl JsParser {
    pub fn new() -> Self {
        Self {
            js_language: tree_sitter_javascript::LANGUAGE.into(),
            ts_language: tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            tsx_language: tree_sitter_typescript::LANGUAGE_TSX.into(),
        }
    }

    /// 根据文件扩展名选择对应的 tree-sitter 语言
    fn language_for_ext(&self, ext: &str) -> &tree_sitter::Language {
        match ext {
            "ts" => &self.ts_language,
            "tsx" => &self.tsx_language,
            _ => &self.js_language, // js, jsx
        }
    }

    /// 根据文件扩展名判断语言标签
    fn lang_label(ext: &str) -> &'static str {
        match ext {
            "ts" | "tsx" => "typescript",
            _ => "javascript",
        }
    }

    /// 判断是否为 TypeScript 文件（支持 interface/enum 提取）
    fn is_typescript(ext: &str) -> bool {
        matches!(ext, "ts" | "tsx")
    }

    /// 解析语法树，提取代码块
    fn extract_blocks(
        &self,
        tree: &Tree,
        source: &str,
        file_path: &str,
        ext: &str,
    ) -> Vec<CodeBlock> {
        let mut blocks = Vec::new();
        self.visit_node(tree.root_node(), source, file_path, ext, &mut blocks, None);
        blocks
    }

    /// 递归遍历语法树节点
    fn visit_node(
        &self,
        node: Node,
        source: &str,
        file_path: &str,
        ext: &str,
        blocks: &mut Vec<CodeBlock>,
        parent_name: Option<&str>,
    ) {
        match node.kind() {
            // Import 语句
            "import_statement" => {
                if let Some(block) = self.extract_import(node, source, file_path, ext) {
                    blocks.push(block);
                }
                return;
            }

            // 类定义
            "class_declaration" => {
                if let Some(block) =
                    self.extract_class(node, source, file_path, ext, BlockKind::Class)
                {
                    let class_name = block.name.clone();
                    blocks.push(block);
                    if let Some(body) = node.child_by_field_name("body") {
                        self.visit_children(
                            body,
                            source,
                            file_path,
                            ext,
                            blocks,
                            Some(&class_name),
                        );
                    }
                    return;
                }
            }

            // TS 接口定义
            "interface_declaration" if Self::is_typescript(ext) => {
                if let Some(block) =
                    self.extract_class(node, source, file_path, ext, BlockKind::Interface)
                {
                    let name = block.name.clone();
                    blocks.push(block);
                    if let Some(body) = node.child_by_field_name("body") {
                        self.visit_children(body, source, file_path, ext, blocks, Some(&name));
                    }
                    return;
                }
            }

            // TS 枚举定义
            "enum_declaration" if Self::is_typescript(ext) => {
                if let Some(block) =
                    self.extract_class(node, source, file_path, ext, BlockKind::Enum)
                {
                    blocks.push(block);
                    return;
                }
            }

            // 类方法定义
            "method_definition" => {
                if let Some(block) =
                    self.extract_method_definition(node, source, file_path, ext, parent_name)
                {
                    blocks.push(block);
                }
                return;
            }

            // 类字段定义
            "field_definition" | "public_field_definition" => {
                if let Some(block) = self.extract_field(node, source, file_path, ext, parent_name) {
                    blocks.push(block);
                }
                return;
            }

            // 顶层函数声明
            "function_declaration" | "generator_function_declaration" => {
                if let Some(block) =
                    self.extract_function(node, source, file_path, ext, parent_name)
                {
                    blocks.push(block);
                }
                return;
            }

            // export 语句 — 递归处理内部声明
            "export_statement" => {
                self.handle_export(node, source, file_path, ext, blocks, parent_name);
                return;
            }

            // 变量声明（可能包含箭头函数赋值）
            "lexical_declaration" | "variable_declaration" => {
                self.handle_variable_declaration(node, source, file_path, ext, blocks, parent_name);
                return;
            }

            _ => {}
        }

        self.visit_children(node, source, file_path, ext, blocks, parent_name);
    }

    /// 遍历子节点
    fn visit_children(
        &self,
        node: Node,
        source: &str,
        file_path: &str,
        ext: &str,
        blocks: &mut Vec<CodeBlock>,
        parent_name: Option<&str>,
    ) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit_node(child, source, file_path, ext, blocks, parent_name);
        }
    }

    /// 提取 import 语句
    fn extract_import(
        &self,
        node: Node,
        source: &str,
        file_path: &str,
        ext: &str,
    ) -> Option<CodeBlock> {
        let content = node_text(node, source);
        // 提取 import 来源（from 后面的模块路径）
        let name = node
            .child_by_field_name("source")
            .map(|n| node_text(n, source))
            .map(|s| s.trim_matches(|c| c == '\'' || c == '"').to_string())
            .unwrap_or_else(|| content.clone());

        Some(CodeBlock {
            file_path: file_path.to_string(),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            content,
            language: Self::lang_label(ext).to_string(),
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
        ext: &str,
        kind: BlockKind,
    ) -> Option<CodeBlock> {
        let name = node
            .child_by_field_name("name")
            .map(|n| node_text(n, source))
            .unwrap_or_default();

        let content = node_text(node, source);
        let annotations = self.collect_decorators(node, source);
        let signature = self.extract_class_signature(node, source);

        // 提取继承依赖（extends / implements）
        let mut dependencies = Vec::new();
        // class Foo extends Bar
        if let Some(heritage) = find_child_by_kind(node, "class_heritage") {
            dependencies.push(node_text(heritage, source));
        }
        // interface Foo extends Bar
        if let Some(extends) = find_child_by_kind(node, "extends_clause") {
            dependencies.push(node_text(extends, source));
        }

        Some(CodeBlock {
            file_path: file_path.to_string(),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            content,
            language: Self::lang_label(ext).to_string(),
            kind,
            name,
            parent: None,
            signature: Some(signature),
            annotations,
            dependencies,
        })
    }

    /// 提取类签名（不含方法体）
    fn extract_class_signature(&self, node: Node, source: &str) -> String {
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
            // 跳过装饰器
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

    /// 提取类方法定义（method_definition 节点）
    fn extract_method_definition(
        &self,
        node: Node,
        source: &str,
        file_path: &str,
        ext: &str,
        parent_name: Option<&str>,
    ) -> Option<CodeBlock> {
        let name = node
            .child_by_field_name("name")
            .map(|n| node_text(n, source))
            .unwrap_or_default();

        let content = node_text(node, source);
        let annotations = self.collect_decorators(node, source);
        let signature = self.extract_function_signature(node, source);

        Some(CodeBlock {
            file_path: file_path.to_string(),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            content,
            language: Self::lang_label(ext).to_string(),
            kind: BlockKind::Method,
            name,
            parent: parent_name.map(|s| s.to_string()),
            signature: Some(signature),
            annotations,
            dependencies: Vec::new(),
        })
    }

    /// 提取顶层函数声明
    fn extract_function(
        &self,
        node: Node,
        source: &str,
        file_path: &str,
        ext: &str,
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
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            content,
            language: Self::lang_label(ext).to_string(),
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

    /// 提取类字段定义
    fn extract_field(
        &self,
        node: Node,
        source: &str,
        file_path: &str,
        ext: &str,
        parent_name: Option<&str>,
    ) -> Option<CodeBlock> {
        let content = node_text(node, source);
        let name = node
            .child_by_field_name("property")
            .or_else(|| node.child_by_field_name("name"))
            .map(|n| node_text(n, source))
            .unwrap_or_default();

        let annotations = self.collect_decorators(node, source);
        let signature = content.trim().trim_end_matches(';').to_string();

        Some(CodeBlock {
            file_path: file_path.to_string(),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            content,
            language: Self::lang_label(ext).to_string(),
            kind: BlockKind::Field,
            name,
            parent: parent_name.map(|s| s.to_string()),
            signature: Some(signature),
            annotations,
            dependencies: Vec::new(),
        })
    }

    /// 处理 export 语句 — 提取内部声明并标记 export
    fn handle_export(
        &self,
        node: Node,
        source: &str,
        file_path: &str,
        ext: &str,
        blocks: &mut Vec<CodeBlock>,
        parent_name: Option<&str>,
    ) {
        let mut has_declaration = false;
        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            match child.kind() {
                // export class Foo {}
                "class_declaration" => {
                    if let Some(block) =
                        self.extract_class(child, source, file_path, ext, BlockKind::Class)
                    {
                        let class_name = block.name.clone();
                        blocks.push(block);
                        if let Some(body) = child.child_by_field_name("body") {
                            self.visit_children(
                                body,
                                source,
                                file_path,
                                ext,
                                blocks,
                                Some(&class_name),
                            );
                        }
                        has_declaration = true;
                    }
                }
                // export interface Foo {}
                "interface_declaration" if Self::is_typescript(ext) => {
                    if let Some(block) =
                        self.extract_class(child, source, file_path, ext, BlockKind::Interface)
                    {
                        let name = block.name.clone();
                        blocks.push(block);
                        if let Some(body) = child.child_by_field_name("body") {
                            self.visit_children(body, source, file_path, ext, blocks, Some(&name));
                        }
                        has_declaration = true;
                    }
                }
                // export enum Foo {}
                "enum_declaration" if Self::is_typescript(ext) => {
                    if let Some(block) =
                        self.extract_class(child, source, file_path, ext, BlockKind::Enum)
                    {
                        blocks.push(block);
                        has_declaration = true;
                    }
                }
                // export function foo() {}
                "function_declaration" | "generator_function_declaration" => {
                    if let Some(block) =
                        self.extract_function(child, source, file_path, ext, parent_name)
                    {
                        blocks.push(block);
                        has_declaration = true;
                    }
                }
                // export const foo = ...
                "lexical_declaration" | "variable_declaration" => {
                    self.handle_variable_declaration(
                        child,
                        source,
                        file_path,
                        ext,
                        blocks,
                        parent_name,
                    );
                    has_declaration = true;
                }
                _ => {}
            }
        }

        // 没有内部声明的 export（如 export default xxx, export { a, b }）
        if !has_declaration {
            let content = node_text(node, source);
            // 提取 export 名称
            let name = self.extract_export_name(node, source);
            blocks.push(CodeBlock {
                file_path: file_path.to_string(),
                start_line: node.start_position().row + 1,
                end_line: node.end_position().row + 1,
                content,
                language: Self::lang_label(ext).to_string(),
                kind: BlockKind::Other,
                name,
                parent: None,
                signature: None,
                annotations: Vec::new(),
                dependencies: Vec::new(),
            });
        }
    }

    /// 提取 export 语句的名称
    fn extract_export_name(&self, node: Node, source: &str) -> String {
        // export default xxx
        if let Some(default_export) = find_child_by_kind(node, "default_export") {
            let mut cursor = default_export.walk();
            for child in default_export.children(&mut cursor) {
                if child.kind() == "identifier" {
                    return format!("default:{}", node_text(child, source));
                }
            }
            return "default".to_string();
        }
        // export { a, b }
        if let Some(named_exports) = find_child_by_kind(node, "named_exports") {
            return node_text(named_exports, source);
        }
        "export".to_string()
    }

    /// 处理变量声明，提取箭头函数赋值
    fn handle_variable_declaration(
        &self,
        node: Node,
        source: &str,
        file_path: &str,
        ext: &str,
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

            // 仅提取箭头函数和函数表达式赋值
            if value.kind() != "arrow_function" && value.kind() != "function_expression" {
                continue;
            }

            let name = child
                .child_by_field_name("name")
                .map(|n| node_text(n, source))
                .unwrap_or_default();

            // 使用整个变量声明语句作为内容（包含 const/let）
            let content = node_text(node, source);
            let signature = self.extract_arrow_signature(child, source);

            blocks.push(CodeBlock {
                file_path: file_path.to_string(),
                start_line: node.start_position().row + 1,
                end_line: node.end_position().row + 1,
                content,
                language: Self::lang_label(ext).to_string(),
                kind: BlockKind::Method,
                name,
                parent: parent_name.map(|s| s.to_string()),
                signature: Some(signature),
                annotations: Vec::new(),
                dependencies: Vec::new(),
            });
        }
    }

    /// 提取箭头函数签名（变量名 + 参数列表 + 类型注解）
    fn extract_arrow_signature(&self, declarator: Node, source: &str) -> String {
        let mut sig = String::new();

        // 变量名
        if let Some(name) = declarator.child_by_field_name("name") {
            sig.push_str(&node_text(name, source));
        }

        // 类型注解（TypeScript）
        if let Some(type_ann) = declarator.child_by_field_name("type") {
            sig.push_str(": ");
            sig.push_str(&node_text(type_ann, source));
        }

        // 箭头函数参数
        if let Some(value) = declarator.child_by_field_name("value") {
            if let Some(params) = value.child_by_field_name("parameters") {
                sig.push_str(" = ");
                sig.push_str(&node_text(params, source));
                sig.push_str(" => ...");
            }
        }

        sig
    }

    /// 收集节点前面的装饰器（decorators）
    fn collect_decorators(&self, node: Node, source: &str) -> Vec<String> {
        let mut decorators = Vec::new();

        // 检查前一个兄弟节点
        let mut prev = node.prev_sibling();
        while let Some(sibling) = prev {
            if sibling.kind() == "decorator" {
                decorators.push(node_text(sibling, source));
            } else {
                break;
            }
            prev = sibling.prev_sibling();
        }

        // 检查子节点中的装饰器
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "decorator" {
                decorators.push(node_text(child, source));
            }
        }

        decorators
    }
}

impl Parser for JsParser {
    fn parse(&self, file_path: &Path) -> Result<Vec<CodeBlock>> {
        let ext = file_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("js");

        let source = fs::read_to_string(file_path).map_err(CodeLensError::Io)?;

        let language = self.language_for_ext(ext);
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(language)
            .map_err(|e| CodeLensError::Parse {
                path: file_path.display().to_string(),
                message: format!("设置 JS/TS 语言失败: {e}"),
            })?;

        let tree = parser
            .parse(&source, None)
            .ok_or_else(|| CodeLensError::Parse {
                path: file_path.display().to_string(),
                message: "解析 JS/TS 文件失败".to_string(),
            })?;

        let file_path_str = file_path.to_string_lossy().to_string();
        Ok(self.extract_blocks(&tree, &source, &file_path_str, ext))
    }

    fn supported_extensions(&self) -> &[&str] {
        &["js", "jsx", "ts", "tsx"]
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
