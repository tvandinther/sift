use anyhow::{Context, Result};
use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config as BertConfig};
use hf_hub::api::sync::Api;
use tokenizers::Tokenizer;

const MODEL_REPO: &str = "sentence-transformers/all-MiniLM-L6-v2";
const EMBEDDING_DIM: usize = 384;

/// Embedding model for generating semantic vectors from text.
pub struct EmbeddingModel {
    model: BertModel,
    tokenizer: Tokenizer,
    device: Device,
}

impl EmbeddingModel {
    /// Load the embedding model from cache or download from HuggingFace.
    ///
    /// The model is cached in the default HuggingFace cache directory
    /// (~/.cache/huggingface or $HF_HOME).
    pub fn load() -> Result<Self> {
        // Detect device (Metal on Apple Silicon, CPU otherwise)
        let device = Self::detect_device()?;

        println!("Loading embedding model on {:?}...", device);

        // Set up HuggingFace API
        let api = Api::new()?;
        let repo = api.model(MODEL_REPO.to_string());

        // Download model files
        println!("Downloading model files from HuggingFace Hub...");
        let config_path = repo.get("config.json")
            .with_context(|| format!("Failed to download config.json from {}", MODEL_REPO))?;
        let tokenizer_path = repo.get("tokenizer.json")
            .with_context(|| format!("Failed to download tokenizer.json from {}", MODEL_REPO))?;
        let weights_path = repo.get("model.safetensors")
            .with_context(|| format!("Failed to download model.safetensors from {}", MODEL_REPO))?;

        // Load config
        let config: BertConfig = serde_json::from_str(
            &std::fs::read_to_string(&config_path).context("Failed to read config.json")?,
        )
        .context("Failed to parse config.json")?;

        // Load tokenizer
        let tokenizer =
            Tokenizer::from_file(&tokenizer_path).map_err(|e| anyhow::anyhow!("{}", e))?;

        // Load model weights
        let vb =
            unsafe { VarBuilder::from_mmaped_safetensors(&[weights_path], DType::F32, &device)? };
        let model = BertModel::load(vb, &config)?;

        Ok(Self {
            model,
            tokenizer,
            device,
        })
    }

    /// Generate embedding vector for a text string.
    pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
        // Tokenize
        let encoding = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| anyhow::anyhow!("Tokenization failed: {}", e))?;

        let tokens = encoding.get_ids();
        let token_ids = Tensor::new(tokens, &self.device)?
            .unsqueeze(0)
            .context("Failed to create token tensor")?;

        // Create attention mask (all ones for simplicity)
        let attention_mask = Tensor::ones_like(&token_ids)?;

        // Generate embeddings
        let embeddings = self
            .model
            .forward(&token_ids, &attention_mask, None)
            .context("Model forward pass failed")?;

        // Mean pooling over sequence dimension
        let (_batch_size, seq_len, _hidden_size) = embeddings.dims3()?;
        let pooled = (embeddings.sum(1)? / (seq_len as f64))?;

        // Extract as Vec<f32>
        let embedding_vec = pooled.squeeze(0)?.to_vec1::<f32>()?;

        if embedding_vec.len() != EMBEDDING_DIM {
            anyhow::bail!(
                "Unexpected embedding dimension: got {}, expected {}",
                embedding_vec.len(),
                EMBEDDING_DIM
            );
        }

        Ok(embedding_vec)
    }

    /// Detect the best available device (Metal on Apple Silicon, CPU otherwise).
    fn detect_device() -> Result<Device> {
        #[cfg(target_os = "macos")]
        {
            // Try Metal on macOS
            if let Ok(device) = Device::new_metal(0) {
                return Ok(device);
            }
        }

        // Fallback to CPU
        Ok(Device::Cpu)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore] // Requires model download
    fn test_embed_basic() {
        let model = EmbeddingModel::load().unwrap();

        let text = "This is a test sentence for embedding generation.";
        let embedding = model.embed(text).unwrap();

        assert_eq!(embedding.len(), EMBEDDING_DIM);

        // Verify the embedding contains non-zero values
        let has_non_zero = embedding.iter().any(|&v| v.abs() > 1e-6);
        assert!(has_non_zero, "Embedding should contain non-zero values");
    }
}
