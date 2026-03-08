use crate::error::Result;
use std::fs;
use std::path::{Path, PathBuf};

/// 内置默认忽略目录
const DEFAULT_IGNORE_DIRS: &[&str] = &[
    ".git",
    ".svn",
    ".hg",
    "target",
    "build",
    "dist",
    "out",
    "node_modules",
    ".idea",
    ".vscode",
    ".gradle",
    ".settings",
    "__pycache__",
    ".DS_Store",
];

/// 内置默认忽略文件扩展名
const DEFAULT_IGNORE_EXTENSIONS: &[&str] = &[
    "class", "jar", "war", "ear", "zip", "tar", "gz", "rar", "7z", "png", "jpg", "jpeg", "gif",
    "bmp", "ico", "svg", "mp3", "mp4", "avi", "mov", "pdf", "doc", "docx", "xls", "xlsx", "ppt",
    "pptx", "exe", "dll", "so", "dylib", "o", "a", "lock",
];

/// 文件扫描器 — 递归扫描项目目录，返回待索引的文件列表
pub struct Scanner {
    /// 项目根目录
    root: PathBuf,
    /// .gitignore 规则（简化版：仅处理目录和文件模式）
    gitignore_patterns: Vec<GitignorePattern>,
}

/// .gitignore 匹配模式
#[derive(Debug)]
struct GitignorePattern {
    /// 原始模式字符串
    pattern: String,
    /// 是否为取反规则（以 ! 开头）
    negated: bool,
    /// 是否仅匹配目录（以 / 结尾）
    dir_only: bool,
}

impl Scanner {
    /// 创建扫描器，自动读取项目 .gitignore
    pub fn new(root: &Path) -> Result<Self> {
        let root = root.canonicalize()?;
        let gitignore_patterns = Self::load_gitignore(&root);

        Ok(Self {
            root,
            gitignore_patterns,
        })
    }

    /// 扫描项目目录，返回所有待索引的文件路径
    pub fn scan(&self) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();
        self.scan_dir(&self.root, &mut files)?;
        Ok(files)
    }

    /// 获取项目根目录
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// 递归扫描目录
    fn scan_dir(&self, dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
        let entries = fs::read_dir(dir)?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            let file_name = entry.file_name();
            let name = file_name.to_string_lossy();

            if path.is_dir() {
                // 跳过默认忽略目录
                if DEFAULT_IGNORE_DIRS.contains(&name.as_ref()) {
                    continue;
                }
                // 跳过 .gitignore 匹配的目录
                if self.is_gitignored(&path, true) {
                    continue;
                }
                self.scan_dir(&path, files)?;
            } else if path.is_file() {
                // 跳过默认忽略扩展名
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if DEFAULT_IGNORE_EXTENSIONS.contains(&ext) {
                        continue;
                    }
                }
                // 跳过 .gitignore 匹配的文件
                if self.is_gitignored(&path, false) {
                    continue;
                }
                files.push(path);
            }
        }

        Ok(())
    }

    /// 读取并解析 .gitignore 文件
    fn load_gitignore(root: &Path) -> Vec<GitignorePattern> {
        let gitignore_path = root.join(".gitignore");
        let content = match fs::read_to_string(&gitignore_path) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        content
            .lines()
            .filter(|line| {
                let trimmed = line.trim();
                !trimmed.is_empty() && !trimmed.starts_with('#')
            })
            .map(|line| {
                let mut pattern = line.trim().to_string();
                let negated = pattern.starts_with('!');
                if negated {
                    pattern = pattern[1..].to_string();
                }
                let dir_only = pattern.ends_with('/');
                if dir_only {
                    pattern = pattern[..pattern.len() - 1].to_string();
                }
                GitignorePattern {
                    pattern,
                    negated,
                    dir_only,
                }
            })
            .collect()
    }

    /// 检查路径是否被 .gitignore 规则匹配
    fn is_gitignored(&self, path: &Path, is_dir: bool) -> bool {
        let relative = match path.strip_prefix(&self.root) {
            Ok(r) => r,
            Err(_) => return false,
        };

        let rel_str = relative.to_string_lossy();
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        let mut ignored = false;

        for rule in &self.gitignore_patterns {
            if rule.dir_only && !is_dir {
                continue;
            }

            let matched = if rule.pattern.contains('/') {
                // 包含路径分隔符时，按相对路径匹配
                rel_str.starts_with(&rule.pattern)
                    || rel_str.contains(&format!("/{}", &rule.pattern))
            } else {
                // 不含路径分隔符时，按文件/目录名匹配
                file_name == rule.pattern || simple_glob_match(&rule.pattern, &file_name)
            };

            if matched {
                ignored = !rule.negated;
            }
        }

        ignored
    }
}

/// 简单的 glob 模式匹配（仅支持 * 通配符）
fn simple_glob_match(pattern: &str, text: &str) -> bool {
    if !pattern.contains('*') {
        return pattern == text;
    }

    let parts: Vec<&str> = pattern.split('*').collect();

    if parts.len() == 2 {
        let (prefix, suffix) = (parts[0], parts[1]);
        return text.starts_with(prefix)
            && text.ends_with(suffix)
            && text.len() >= prefix.len() + suffix.len();
    }

    // 多个 * 的情况，回退到简单检查
    let mut remaining = text;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if i == 0 {
            if !remaining.starts_with(part) {
                return false;
            }
            remaining = &remaining[part.len()..];
        } else if let Some(pos) = remaining.find(part) {
            remaining = &remaining[pos + part.len()..];
        } else {
            return false;
        }
    }
    true
}
