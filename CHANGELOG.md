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
