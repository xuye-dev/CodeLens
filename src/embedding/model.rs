use crate::error::{CodeLensError, Result};
use ndarray::{Array2, Axis};
use ort::session::Session;
use ort::value::TensorRef;
use std::path::Path;
use std::sync::Mutex;
use tokenizers::Tokenizer;
use tracing::info;

/// 最大 token 序列长度（all-MiniLM-L6-v2 支持最大 256，我们用 128 节省资源）
const MAX_SEQ_LEN: usize = 128;

/// Embedding 向量维度（all-MiniLM-L6-v2 输出 384 维）
pub const EMBEDDING_DIM: usize = 384;

/// Embedding 模型 — 封装 ONNX Runtime session 和 tokenizer
///
/// 线程安全：Session 通过 Mutex 保护，Tokenizer 为 Send + Sync。
pub struct EmbeddingModel {
    session: Mutex<Session>,
    tokenizer: Tokenizer,
}

impl EmbeddingModel {
    /// 从模型目录加载 ONNX 模型和 tokenizer
    pub fn load(model_dir: &Path) -> Result<Self> {
        let model_path = model_dir.join("model_quantized.onnx");
        let tokenizer_path = model_dir.join("tokenizer.json");

        let session = Session::builder()
            .map_err(|e| CodeLensError::Embedding(format!("创建 ONNX session builder 失败: {e}")))?
            .with_intra_threads(2)
            .map_err(|e| CodeLensError::Embedding(format!("设置线程数失败: {e}")))?
            .commit_from_file(&model_path)
            .map_err(|e| {
                CodeLensError::Embedding(format!(
                    "加载 ONNX 模型失败 {}: {e}",
                    model_path.display()
                ))
            })?;

        let tokenizer = Tokenizer::from_file(&tokenizer_path).map_err(|e| {
            CodeLensError::Embedding(format!(
                "加载 tokenizer 失败 {}: {e}",
                tokenizer_path.display()
            ))
        })?;

        info!("Embedding 模型加载完成");
        Ok(Self {
            session: Mutex::new(session),
            tokenizer,
        })
    }

    /// 对单个文本生成 384 维 embedding 向量
    pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let batch = self.embed_batch(&[text])?;
        batch
            .into_iter()
            .next()
            .ok_or_else(|| CodeLensError::Embedding("embed_batch 返回空结果".to_string()))
    }

    /// 批量生成 embedding 向量
    ///
    /// 为了控制内存，内部按 batch_size 分批处理。
    pub fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let batch_size = 32;
        let mut all_embeddings = Vec::with_capacity(texts.len());

        for chunk in texts.chunks(batch_size) {
            let mut batch_result = self.embed_batch_inner(chunk)?;
            all_embeddings.append(&mut batch_result);
        }

        Ok(all_embeddings)
    }

    /// 内部批量推理
    fn embed_batch_inner(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let batch = texts.len();
        if batch == 0 {
            return Ok(Vec::new());
        }

        // 分词
        let encodings: Vec<_> = texts
            .iter()
            .map(|text| {
                self.tokenizer
                    .encode(*text, true)
                    .map_err(|e| CodeLensError::Embedding(format!("tokenizer encode 失败: {e}")))
            })
            .collect::<Result<Vec<_>>>()?;

        // 计算实际最大长度（截断到 MAX_SEQ_LEN）
        let max_len = encodings
            .iter()
            .map(|e| e.get_ids().len().min(MAX_SEQ_LEN))
            .max()
            .unwrap_or(0);

        if max_len == 0 {
            return Ok(vec![vec![0.0; EMBEDDING_DIM]; batch]);
        }

        // 构建 input_ids 和 attention_mask 张量（i64 类型）
        let mut input_ids = Array2::<i64>::zeros((batch, max_len));
        let mut attention_mask = Array2::<i64>::zeros((batch, max_len));
        let token_type_ids = Array2::<i64>::zeros((batch, max_len));

        for (i, encoding) in encodings.iter().enumerate() {
            let ids = encoding.get_ids();
            let mask = encoding.get_attention_mask();
            let len = ids.len().min(max_len);

            for j in 0..len {
                input_ids[[i, j]] = ids[j] as i64;
                attention_mask[[i, j]] = mask[j] as i64;
                // token_type_ids 保持 0（单句输入）
            }
        }

        // 运行 ONNX 推理
        let input_ids_ref = TensorRef::from_array_view(input_ids.view())
            .map_err(|e| CodeLensError::Embedding(format!("创建 input_ids 张量失败: {e}")))?;
        let attention_mask_ref = TensorRef::from_array_view(attention_mask.view())
            .map_err(|e| CodeLensError::Embedding(format!("创建 attention_mask 张量失败: {e}")))?;
        let token_type_ids_ref = TensorRef::from_array_view(token_type_ids.view())
            .map_err(|e| CodeLensError::Embedding(format!("创建 token_type_ids 张量失败: {e}")))?;

        let mut session = self
            .session
            .lock()
            .map_err(|e| CodeLensError::Embedding(format!("获取模型锁失败: {e}")))?;

        let outputs = session
            .run(ort::inputs![
                "input_ids" => input_ids_ref,
                "attention_mask" => attention_mask_ref,
                "token_type_ids" => token_type_ids_ref
            ])
            .map_err(|e| CodeLensError::Embedding(format!("ONNX 推理失败: {e}")))?;

        // 提取输出：last_hidden_state [batch, seq_len, 384]
        let output = outputs
            .get("last_hidden_state")
            .or_else(|| outputs.get("token_embeddings"))
            .unwrap_or(&outputs[0]);

        let embeddings_array = output
            .try_extract_array::<f32>()
            .map_err(|e| CodeLensError::Embedding(format!("提取输出张量失败: {e}")))?;

        // Mean pooling + L2 归一化
        let mut results = Vec::with_capacity(batch);
        for i in 0..batch {
            let token_embeddings = embeddings_array.index_axis(Axis(0), i); // [seq_len, 384]
            let mask_row = attention_mask.index_axis(Axis(0), i); // [seq_len]

            // 加权平均（只对有效 token 做平均）
            let mut pooled = vec![0.0f32; EMBEDDING_DIM];
            let mut total_weight = 0.0f32;

            for (j, mask_val) in mask_row.iter().enumerate() {
                if *mask_val > 0 {
                    let weight = *mask_val as f32;
                    total_weight += weight;
                    for (k, p) in pooled.iter_mut().enumerate() {
                        *p += token_embeddings[[j, k]] * weight;
                    }
                }
            }

            if total_weight > 0.0 {
                for p in &mut pooled {
                    *p /= total_weight;
                }
            }

            // L2 归一化
            let norm: f32 = pooled.iter().map(|x| x * x).sum::<f32>().sqrt();
            if norm > 0.0 {
                for p in &mut pooled {
                    *p /= norm;
                }
            }

            results.push(pooled);
        }

        Ok(results)
    }
}
