# 更新日志

本文件记录 CodeLens 项目的所有版本变动。格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)。

## [0.2.0] - 2026-03-08

### 新增

- 新增 JavaScript/TypeScript 解析器（`src/parser/js.rs`），支持 `.js`/`.jsx`/`.ts`/`.tsx` 文件
- 提取 class、interface（TS）、enum（TS）、function、method、field、import、export 代码块
- 使用 tree-sitter-javascript 0.23 + tree-sitter-typescript 0.23 进行语法解析
- 支持装饰器（decorators）提取、箭头函数赋值识别、export 语句内部声明展开

## [0.1.1] - 2026-03-08

### 变更

- 升级 rmcp 从 0.1 到 1.1（MCP 协议 SDK 大版本更新）
- 升级 schemars 从 0.8 到 1（匹配 rmcp 1.x 的依赖要求）
- 使用 `#[tool_router]` + `#[tool_handler]` + `#[tool]` 宏重构 MCP Server，替代手动实现 `call_tool`/`list_tools`
- 使用 `Parameters<T>` 自动参数提取，替代手动 `serde_json::from_value`
- 使用 `ServerInfo::new()` builder 方法链构造服务器信息，替代直接结构体构造
- 协议版本从 `ProtocolVersion::V_2024_11_05` 升级为 `ProtocolVersion::LATEST`

### 优化

- BM25 类型权重重新调优（Class/Interface/Enum ×2.0 > Method/Constructor ×1.3 > XmlNode ×1.2 > XmlNamespace ×1.1 > Field ×1.0 > Import ×0.4），大幅提升类/接口定义在部分名称匹配时的排序优先级
- XML 解析器新增 `resultMap`、`sql` 定义标签索引，MyBatis Mapper 索引覆盖范围从仅 CRUD 扩展到包含映射定义和 SQL 片段
- 实现 `context` 参数的行截取模式：传入数字 N 时仅输出匹配行 ±N 行（不连续区间用 `...` 分隔），传入 `"full"` 时输出完整代码块

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
