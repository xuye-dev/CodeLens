# CodeLens

A local code context retrieval MCP Server written in Rust. It provides a `search` tool via stdio for AI coding assistants (e.g., Claude Code) to query code semantics — all data stays on your machine.

## Features

- **BM25 keyword search** — Relevance-ranked code retrieval with type-aware boosting, deduplication (parent-child overlap removal + per-file diversity limit)
- **Java support** — Parses classes, methods, constructors, fields, annotations, and imports via tree-sitter
- **JavaScript/TypeScript support** — Parses classes, interfaces (TS), enums (TS), functions, methods, fields, decorators, imports, and exports via tree-sitter
- **Vue support** — Parses `.vue` Single File Components, extracts `<template>` and `<script>`/`<script setup>` blocks with `lang="ts"` auto-detection
- **XML support** — Parses MyBatis Mapper XML (namespace, select/insert/update/delete, resultMap, sql fragments) and generic XML configs
- **Live indexing** — Full scan on startup + incremental updates via filesystem watcher (inotify)
- **Single binary** — Compiles to one executable, just point your MCP client config at it
- **Privacy first** — Pure local processing, no network calls, your code never leaves your machine

## Quick Start

### Build

```bash
cargo build --release
```

The binary is at `target/release/codelens`.

### Run

```bash
./target/release/codelens                      # uses current working directory
./target/release/codelens --path /your/project  # or specify a project path
```

### MCP Client Configuration

Add to your MCP client config (e.g., Claude Code `.mcp.json`):

```json
{
  "mcpServers": {
    "codelens": {
      "command": "/absolute/path/to/codelens"
    }
  }
}
```

The server defaults to the current working directory. To specify a different project path:

```json
{
  "mcpServers": {
    "codelens": {
      "command": "/absolute/path/to/codelens",
      "args": ["--path", "/your/project/path"]
    }
  }
}
```

## Search Tool

CodeLens exposes a single `search` tool via MCP with the following parameters:

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `query` | string | yes | — | Search keywords |
| `lang` | string | no | null | Language filter, supports comma-separated multi-lang (`"java"`, `"vue"`, `"javascript"`, `"typescript"`, `"xml"`, or `"vue,javascript,typescript"`) |
| `limit` | uint | no | 10 | Max number of results |
| `context` | string | no | `"full"` | `"full"` for complete code blocks, or a number N for ±N lines around matches |
| `path` | string | no | null | Directory filter, only search files under this path (e.g., `"src/api"`) |

### Example Results

```
--- Result 1 (score: 27.67) ---
File: src/main/java/com/example/UserService.java  Line: 15-28
Type: Method  Name: findUserById
Parent: UserService
Signature: public User findUserById(Long id)
```

## Architecture

```
src/
├── mcp/        # MCP protocol layer (server lifecycle, tool definition)
├── parser/     # Multi-language parsers (one file per language, unified trait)
├── search/     # Search engine (BM25 keyword ranking)
├── indexer/    # Index management (in-memory store + file watcher)
└── scanner/    # File scanner (.gitignore parsing + built-in ignore rules)
```

**Data flow:** Scanner discovers files → Parser extracts structured code blocks → Indexer stores in memory → Search ranks via BM25 → MCP Server returns results over stdio.

## Tech Stack

| Technology | Version | Purpose |
|------------|---------|---------|
| Rust | 2021 edition | Language |
| rmcp | 1.1 | MCP protocol SDK (JSON-RPC + stdio) |
| tree-sitter | 0.24 | Source code parsing (AST for Java, JS/TS, Vue) |
| quick-xml | 0.37 | XML parsing (MyBatis Mapper, etc.) |
| notify | 7 | Filesystem watcher (inotify) |
| tokio | 1 | Async runtime |

## Extending Language Support

To add a new language, create a new file in `src/parser/` and implement the `Parser` trait:

```rust
pub trait Parser {
    fn parse(&self, file_path: &Path) -> Result<Vec<CodeBlock>>;
    fn supported_extensions(&self) -> &[&str];
}
```

No changes needed in the search or indexing modules.

## License

MIT
