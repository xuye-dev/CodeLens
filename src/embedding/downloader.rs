use crate::error::{CodeLensError, Result};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::info;

const MODEL_URL: &str =
    "https://huggingface.co/Xenova/all-MiniLM-L6-v2/resolve/main/onnx/model_quantized.onnx";
const TOKENIZER_URL: &str =
    "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/tokenizer.json";

const MODEL_FILENAME: &str = "model_quantized.onnx";
const TOKENIZER_FILENAME: &str = "tokenizer.json";

/// 确保模型文件存在，缺失则自动下载
///
/// 返回模型目录路径（包含 model_quantized.onnx 和 tokenizer.json）。
pub async fn ensure_model_files(model_dir: Option<&Path>) -> Result<PathBuf> {
    let dir = match model_dir {
        Some(d) => d.to_path_buf(),
        None => default_model_dir()?,
    };

    fs::create_dir_all(&dir)
        .map_err(|e| CodeLensError::Download(format!("无法创建模型目录 {}: {e}", dir.display())))?;

    let model_path = dir.join(MODEL_FILENAME);
    let tokenizer_path = dir.join(TOKENIZER_FILENAME);

    if !model_path.exists() {
        info!(
            url = MODEL_URL,
            "正在下载 ONNX 模型（首次运行，约 23MB）..."
        );
        download_file(MODEL_URL, &model_path).await?;
        info!("模型下载完成");
    }

    if !tokenizer_path.exists() {
        info!(url = TOKENIZER_URL, "正在下载 tokenizer...");
        download_file(TOKENIZER_URL, &tokenizer_path).await?;
        info!("tokenizer 下载完成");
    }

    Ok(dir)
}

/// 获取默认模型缓存目录：~/.cache/codelens/models/
fn default_model_dir() -> Result<PathBuf> {
    let cache_dir = dirs::cache_dir()
        .ok_or_else(|| CodeLensError::Download("无法获取用户缓存目录（~/.cache）".to_string()))?;
    Ok(cache_dir.join("codelens").join("models"))
}

/// 下载文件到指定路径
async fn download_file(url: &str, dest: &Path) -> Result<()> {
    let response = reqwest::get(url)
        .await
        .map_err(|e| CodeLensError::Download(format!("下载失败 {url}: {e}")))?;

    if !response.status().is_success() {
        return Err(CodeLensError::Download(format!(
            "下载失败 {url}: HTTP {}",
            response.status()
        )));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| CodeLensError::Download(format!("读取响应体失败: {e}")))?;

    // 写入临时文件再重命名，避免中断导致的损坏文件
    let tmp_path = dest.with_extension("tmp");
    fs::write(&tmp_path, &bytes)
        .map_err(|e| CodeLensError::Download(format!("写入文件失败: {e}")))?;
    fs::rename(&tmp_path, dest)
        .map_err(|e| CodeLensError::Download(format!("重命名文件失败: {e}")))?;

    Ok(())
}
