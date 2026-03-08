use crate::indexer::store::IndexStore;
use crate::search::bm25::Bm25Engine;
use rmcp::handler::server::tool::schema_for_type;
use rmcp::model::*;
use rmcp::service::{RequestContext, RoleServer};
use rmcp::{Error as McpError, ServerHandler};
use schemars::JsonSchema;
use serde::Deserialize;
use std::sync::{Arc, Mutex};

/// CodeLens MCP Server — 通过 stdio 提供 search 工具
#[derive(Clone)]
pub struct CodeLensServer {
    store: Arc<Mutex<IndexStore>>,
    engine: Arc<Bm25Engine>,
}

/// search 工具的请求参数
#[derive(Debug, Deserialize, JsonSchema)]
struct SearchParams {
    /// 搜索关键词
    query: String,
    /// 可选语言筛选（如 "java"、"xml"）
    #[serde(default)]
    lang: Option<String>,
    /// 返回结果数量，默认 10
    #[serde(default = "default_limit")]
    limit: usize,
    /// 上下文模式："full"（完整代码块）或数字 N（匹配行 ± N 行）
    #[serde(default = "default_context")]
    context: String,
}

fn default_limit() -> usize {
    10
}

fn default_context() -> String {
    "full".to_string()
}

impl ServerHandler for CodeLensServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities {
                tools: Some(ToolsCapability { list_changed: None }),
                ..Default::default()
            },
            server_info: Implementation {
                name: "codelens".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
            instructions: Some(
                "CodeLens 是本地代码上下文检索服务。使用 search 工具搜索代码片段。".to_string(),
            ),
        }
    }

    fn list_tools(
        &self,
        _request: PaginatedRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListToolsResult, McpError>> + Send + '_ {
        let schema = schema_for_type::<SearchParams>();
        let tools = vec![Tool::new(
            "search",
            "搜索代码 — 根据关键词搜索匹配的代码片段，返回文件路径、行号、上下文代码，支持按语言筛选",
            schema,
        )];
        std::future::ready(Ok(ListToolsResult {
            tools,
            next_cursor: None,
        }))
    }

    fn call_tool(
        &self,
        request: CallToolRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<CallToolResult, McpError>> + Send + '_ {
        async move {
            if request.name != "search" {
                return Err(McpError::invalid_params(
                    format!("未知工具: {}", request.name),
                    None,
                ));
            }

            // 解析参数
            let params: SearchParams = if let Some(args) = request.arguments {
                serde_json::from_value(serde_json::Value::Object(args.into_iter().collect()))
                    .map_err(|e| McpError::invalid_params(format!("参数解析失败: {e}"), None))?
            } else {
                return Err(McpError::invalid_params("缺少参数", None));
            };

            // 执行搜索
            let store = self
                .store
                .lock()
                .map_err(|e| McpError::internal_error(format!("索引锁获取失败: {e}"), None))?;

            let all_blocks = store.all_blocks();
            let results = self.engine.search(
                &params.query,
                &all_blocks,
                params.lang.as_deref(),
                params.limit,
            );

            if results.is_empty() {
                return Ok(CallToolResult::success(vec![Content::text(
                    "未找到匹配的代码片段。",
                )]));
            }

            // 格式化输出
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

                output.push_str("```\n");
                output.push_str(&block.content);
                output.push_str("\n```\n\n");
            }

            Ok(CallToolResult::success(vec![Content::text(output)]))
        }
    }
}

impl CodeLensServer {
    /// 创建新的 MCP Server 实例
    pub fn new(store: Arc<Mutex<IndexStore>>) -> Self {
        Self {
            store,
            engine: Arc::new(Bm25Engine::new()),
        }
    }
}
