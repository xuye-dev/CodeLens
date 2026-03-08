# 更新日志

本文件记录 CodeLens 项目的所有版本变动。格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)。

## [0.1.0] - 2026-03-08

### 新增

- 项目初始化，确定技术栈（Rust + rmcp + tree-sitter + BM25）
- 基础目录结构搭建（mcp / parser / search / indexer / scanner）
- 添加核心依赖（rmcp、tree-sitter、quick-xml、notify、clap、tokio、thiserror 等）
- 需求确认文档与开发计划
- 项目入口与命令行解析（clap 解析 `--path` 参数，tracing 结构化日志，tokio 异步运行时）
- 全局统一错误类型（`CodeLensError`，覆盖 IO / 解析 / 索引 / XML / 文件监听错误）
- 公共数据模型（`CodeBlock` 代码块、`SearchResult` 搜索结果、`BlockKind` 代码块类型枚举）
- Parser trait 统一接口（`parse()` + `supported_extensions()` + 文件扩展名分发逻辑）
- 文件扫描器 Scanner（递归目录扫描、内置忽略规则、`.gitignore` 解析与匹配）
- 内存索引存储 IndexStore（HashMap 按文件路径存储 CodeBlock，支持增删改查与语言筛选）
- Java 解析器（tree-sitter 解析类、方法、构造函数、字段、注解、import，提取签名与依赖信息）
- XML 解析器（quick-xml 解析 MyBatis Mapper XML 的 namespace/select/insert/update/delete，支持通用 XML 配置文件）
- BM25 检索引擎（自实现 BM25 算法，支持关键词相关度排序、语言筛选、名称精确匹配加分）
- 索引构建流程 IndexBuilder（Scanner → Parser → IndexStore 全量构建，支持单文件增量重建）
- 文件监听 FileWatcher（notify 库监听文件创建/修改/删除事件，自动触发增量索引更新）
- MCP Server 实现（rmcp + stdio 传输层，实现 ServerHandler trait，注册 search 工具）
- search 工具（支持 `query`/`lang`/`limit`/`context` 参数，返回结构化代码片段含文件路径、行号、类型、签名）
- 端到端集成（main.rs 串联完整启动流程：命令行解析 → 扫描建索引 → 文件监听 → MCP Server 就绪）
