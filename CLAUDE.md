# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 项目概述

CodeLens 是一个用 Rust 编写的**通用本地代码上下文检索 MCP Server**。通过 stdio 提供 `search` 工具，供 AI 编程助手（如 Claude Code）查询代码语义，代码不出本地。启动时通过 `--path` 指定目标项目目录，支持任意代码仓库。MVP 仅适配 Linux。

## 技术栈

| 类别 | 技术 | 版本 | 用途 |
|---|---|---|---|
| **核心** | Rust | edition 2021 | 开发语言 |
| **核心** | rmcp | 1.1 | MCP 协议 SDK（JSON-RPC + stdio 通信） |
| **核心** | tokio | 1 | 异步运行时 |
| **检索** | BM25（自实现） | — | 关键词相关度排序 + 类型权重加分 |
| **解析** | tree-sitter | 0.24 | 通用源码解析框架（语法树） |
| **解析** | quick-xml | 0.37 | XML/配置文件解析 |
| **索引** | notify | 7 | 文件监听（inotify），增量更新 |
| **基础** | clap | 4 | 命令行参数解析 |
| **基础** | serde / serde_json | 1 | JSON 序列化/反序列化 |
| **基础** | tracing | 0.1 | 结构化日志 |

### 已实现的语言解析器

| 语言 | 解析库 | 提取内容 |
|---|---|---|
| Java | tree-sitter-java 0.23 | 类、接口、枚举、方法、构造函数、字段、注解、import、继承关系 |
| JavaScript/TypeScript | tree-sitter-javascript 0.23 + tree-sitter-typescript 0.23 | 类、接口（TS）、枚举（TS）、函数、方法、字段、装饰器、import、export |
| Vue | tree-sitter-javascript 0.23 + tree-sitter-typescript 0.23 | SFC template 区块、script/script setup 中的 JS/TS 代码（复用 JS/TS 解析器）、Vue 编译器宏（defineProps/defineEmits/defineExpose/defineSlots） |
| XML | quick-xml 0.37 | MyBatis Mapper（namespace/CRUD/resultMap/sql）、通用 XML 配置 |

### 扩展新语言

新增语言只需在 `src/parser/` 下创建新文件并实现 `Parser` trait：

```rust
pub trait Parser {
    fn parse(&self, file_path: &Path) -> Result<Vec<CodeBlock>>;
    fn supported_extensions(&self) -> &[&str];
}
```

然后在 `src/parser/mod.rs` 的分发逻辑中注册扩展名即可，**无需改动检索和索引模块**。

## 构建与运行命令

```bash
cargo build                        # 调试构建
cargo build --release              # 发布构建（单一可执行文件）
cargo run -- --path /your/project  # 启动 MCP Server，指定目标项目路径
cargo test                         # 运行全部测试
cargo test test_name               # 运行单个测试
cargo clippy                       # 代码检查
cargo fmt                          # 格式化代码
cargo fmt -- --check               # 仅检查格式，不修改
```

## 架构

```
src/
├── mcp/        # MCP 协议层（Server 生命周期、工具定义）
├── parser/     # 多语言解析器（每种语言一个文件，统一 trait 接口）
│   ├── mod.rs  #   Parser trait 定义 + 扩展名分发
│   ├── java.rs #   Java 解析器（tree-sitter）
│   ├── js.rs   #   JavaScript/TypeScript 解析器（tree-sitter）
│   ├── vue.rs  #   Vue SFC 解析器（复用 JS/TS tree-sitter）
│   └── xml.rs  #   XML 解析器（quick-xml）
├── search/     # 检索引擎（BM25 关键词排序 + 类型权重）
├── indexer/    # 索引管理（内存存储 + 文件监听增量更新）
└── scanner/    # 文件扫描（.gitignore 解析 + 内置忽略规则）
```

**数据流：** Scanner 发现文件 → Parser 按扩展名分发提取结构化代码块 → Indexer 存入内存 → Search 通过 BM25 排序 → MCP Server 经 stdio 返回结果。

## 编码规范

- 遵循 Rust 标准命名：函数/变量用 `snake_case`，类型/trait 用 `PascalCase`，常量用 `SCREAMING_SNAKE_CASE`。
- 格式化使用 `rustfmt` 默认配置。
- 使用 `clippy` 默认 lint 规则，提交前修复所有警告。
- 注释使用 Rust 文档注释（公开项用 `///`，内部用 `//`），不写作者和时间信息。
- 错误处理使用 `Result<T, E>` 配合 `thiserror` 或自定义错误类型，生产代码禁止 `.unwrap()`。

## 上下文检索优先级

2. 精确文本/正则匹配 → `Grep`
3. 项目结构/目录浏览 → `tree`
4. 深度多轮探索 → `Explore` agent
5. 第三方库文档与用法 → **Context7 MCP**（`mcp__context7`）

## 关键设计决策

- **通用多语言架构**：Parser trait 接口 + 扩展名分发，新增语言不影响检索和索引模块。
- **单文件分发**：编译为单个可执行文件，在 MCP 客户端配置中通过 `--path` 指定目标目录即可使用。
- **纯内存索引（MVP）**：无磁盘持久化；启动时全量扫描，运行中通过文件监听增量更新。
- **仅提供 search 工具**：不提供文件摘要或目录结构工具，AI 可直接用 Read/find 查看。
- **search 参数**：`query`（关键词）、`lang`（可选语言筛选，支持逗号分隔多语言如 `"vue,javascript,typescript"`）、`limit`（默认 10）、`context`（完整代码块或匹配行 ±N 行）、`path`（可选目录筛选）。
- **BM25 类型权重**：Class/Interface/Enum ×2.0 > Method/Constructor ×1.3 > XmlNode ×1.2 > XmlNamespace ×1.1 > Field ×1.0 > Import ×0.4，确保搜索结果以核心定义为主。
