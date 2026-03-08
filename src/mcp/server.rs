use crate::indexer::store::IndexStore;
use crate::search::bm25::Bm25Engine;
use rmcp::handler::server::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::*;
use rmcp::{tool, tool_handler, tool_router, ErrorData as McpError, ServerHandler};
use schemars::JsonSchema;
use serde::Deserialize;
use std::sync::{Arc, Mutex};

/// CodeLens MCP Server — 通过 stdio 提供 search 工具
#[derive(Clone)]
pub struct CodeLensServer {
    store: Arc<Mutex<IndexStore>>,
    engine: Arc<Bm25Engine>,
    tool_router: ToolRouter<Self>,
}

/// search 工具的请求参数
#[derive(Debug, Deserialize, JsonSchema)]
struct SearchParams {
    /// 搜索关键词
    query: String,
    /// 可选语言筛选,支持逗号分隔多语言(如 "java"、"xml"、"vue,javascript,typescript")
    #[serde(default)]
    lang: Option<String>,
    /// 返回结果数量，默认 10
    #[serde(default = "default_limit")]
    limit: usize,
    /// 上下文模式："full"（完整代码块）或数字 N（匹配行 ± N 行）
    #[serde(default = "default_context")]
    context: String,
    /// 可选目录筛选，仅搜索该目录下的文件（如 "src/api"、"src/components"）
    #[serde(default)]
    path: Option<String>,
}

fn default_limit() -> usize {
    10
}

fn default_context() -> String {
    "full".to_string()
}

#[tool_router]
impl CodeLensServer {
    #[tool(
        name = "search",
        description = "搜索代码 — 根据关键词搜索匹配的代码片段,返回文件路径、行号、上下文代码,支持按语言筛选。基于 BM25 关键词匹配(非语义搜索),请使用精确的类名、方法名、变量名等代码标识符作为关键词,不支持自然语言描述。支持文件类型: Java, JavaScript, TypeScript, Vue, XML"
    )]
    async fn search(&self, params: Parameters<SearchParams>) -> Result<CallToolResult, McpError> {
        let params = params.0;

        let store = self
            .store
            .lock()
            .map_err(|e| McpError::internal_error(format!("索引锁获取失败: {e}"), None))?;

        let all_blocks = store.all_blocks();

        // 按目录前缀过滤
        let filtered: Vec<&crate::models::CodeBlock>;
        let search_blocks = if let Some(ref path_filter) = params.path {
            filtered = all_blocks
                .iter()
                .filter(|b| b.file_path.contains(path_filter.as_str()))
                .copied()
                .collect();
            &filtered[..]
        } else {
            &all_blocks[..]
        };

        let results = self.engine.search(
            &params.query,
            search_blocks,
            params.lang.as_deref(),
            params.limit,
        );

        if results.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text(
                "未找到匹配的代码片段。",
            )]));
        }

        // 解析 context 参数：数字 N 表示匹配行 ±N 行，"full" 表示完整代码块
        let context_lines: Option<usize> = params.context.parse::<usize>().ok();

        let mut output = String::new();
        for (i, result) in results.iter().enumerate() {
            let block = &result.block;

            output.push_str(&format!(
                "--- 结果 {} (分数: {:.2}) ---\n",
                i + 1,
                result.score
            ));
            output.push_str(&format!(
                "文件: {}  行: {}-{}\n",
                block.file_path, block.start_line, block.end_line
            ));
            output.push_str(&format!("类型: {:?}  名称: {}\n", block.kind, block.name));

            if let Some(ref parent) = block.parent {
                output.push_str(&format!("所属: {parent}\n"));
            }
            if let Some(ref sig) = block.signature {
                output.push_str(&format!("签名: {sig}\n"));
            }
            if !block.annotations.is_empty() {
                output.push_str(&format!("注解: {}\n", block.annotations.join(", ")));
            }

            // 根据 context 参数决定输出内容
            let display_content = if let Some(n) = context_lines {
                extract_context_lines(&block.content, &params.query, n)
            } else {
                block.content.clone()
            };

            output.push_str("```\n");
            output.push_str(&display_content);
            output.push_str("\n```\n\n");
        }

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }
}

#[tool_handler]
impl ServerHandler for CodeLensServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("codelens", env!("CARGO_PKG_VERSION")))
            .with_instructions("CodeLens 是本地代码上下文检索服务。使用 search 工具按关键词检索代码片段（BM25 关键词匹配，非语义搜索）。支持的文件类型：Java、JavaScript、TypeScript、Vue、XML。不索引 Markdown 等文档文件，文档请用 Read/Grep 工具直接读取。")
            .with_protocol_version(ProtocolVersion::LATEST)
    }
}

impl CodeLensServer {
    /// 创建新的 MCP Server 实例
    pub fn new(store: Arc<Mutex<IndexStore>>) -> Self {
        Self {
            store,
            engine: Arc::new(Bm25Engine::new()),
            tool_router: Self::tool_router(),
        }
    }
}

/// 从代码内容中提取匹配行 ± N 行的上下文
fn extract_context_lines(content: &str, query: &str, n: usize) -> String {
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        return content.to_string();
    }

    let query_lower = query.to_lowercase();
    let query_terms: Vec<&str> = query_lower.split_whitespace().collect();

    // 找到所有匹配行的索引
    let mut match_indices: Vec<usize> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let line_lower = line.to_lowercase();
        if query_terms.iter().any(|term| line_lower.contains(term)) {
            match_indices.push(i);
        }
    }

    // 如果没有匹配行，返回完整内容
    if match_indices.is_empty() {
        return content.to_string();
    }

    // 合并所有匹配行的 ±N 行范围
    let mut included = vec![false; lines.len()];
    for &idx in &match_indices {
        let start = idx.saturating_sub(n);
        let end = (idx + n + 1).min(lines.len());
        for item in included.iter_mut().take(end).skip(start) {
            *item = true;
        }
    }

    // 输出连续区间，不连续处用 "..." 分隔
    let mut result = String::new();
    let mut in_block = false;
    for (i, line) in lines.iter().enumerate() {
        if included[i] {
            if !in_block && !result.is_empty() {
                result.push_str("  ...\n");
            }
            result.push_str(line);
            result.push('\n');
            in_block = true;
        } else {
            in_block = false;
        }
    }

    // 移除末尾多余换行
    if result.ends_with('\n') {
        result.pop();
    }
    result
}
