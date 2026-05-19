use anyhow::{Context, Result};
use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config as BertConfig};
use tokenizers::Tokenizer;
use std::path::Path;

const MODEL_NAME: &str = "sentence-transformers/all-MiniLM-L6-v2";
const BASE_URL: &str = "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main";
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
    /// The model is cached in the model_cache directory from config.
    pub fn load(model_cache: &Path, verbose: bool) -> Result<Self> {
        // Detect device (Metal on Apple Silicon, CPU otherwise)
        let device = Self::detect_device()?;

        if verbose {
            println!("Loading embedding model on {:?}...", device);
        }

        // Check if model is already cached
        let is_cached = Self::check_model_cached(model_cache);

        if !is_cached {
            // Ask for confirmation before downloading
            println!("\nEmbedding model not found in cache.");
            println!("Model: {}", MODEL_NAME);
            println!("Size: ~90 MB");
            println!("\nDownload now? (y/N) ");

            use std::io::{self, Write};
            io::stdout().flush()?;

            let mut response = String::new();
            io::stdin().read_line(&mut response)?;

            if !response.trim().eq_ignore_ascii_case("y") {
                anyhow::bail!("Model download cancelled. Use --lexical-only to search without embeddings.");
            }

            println!("Downloading model files from HuggingFace...");
            Self::download_model(model_cache, verbose)?;
        } else if verbose {
            println!("Loading model from cache...");
        }

        // Load model files
        let config_path = model_cache.join("config.json");
        let tokenizer_path = model_cache.join("tokenizer.json");
        let weights_path = model_cache.join("model.safetensors");

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

    /// Download model files from HuggingFace.
    fn download_model(model_cache: &Path, verbose: bool) -> Result<()> {
        // Create cache directory
        std::fs::create_dir_all(model_cache)
            .context("Failed to create model cache directory")?;

        let files = vec![
            ("config.json", "config.json"),
            ("tokenizer.json", "tokenizer.json"),
            ("model.safetensors", "model.safetensors"),
        ];

        for (filename, output_name) in files {
            let url = format!("{}/{}", BASE_URL, filename);
            let output_path = model_cache.join(output_name);

            if verbose {
                println!("Downloading {}...", filename);
            }

            let response = ureq::get(&url)
                .call()
                .with_context(|| format!("Failed to download {}", url))?;

            let mut file = std::fs::File::create(&output_path)
                .with_context(|| format!("Failed to create file {}", output_path.display()))?;

            std::io::copy(&mut response.into_reader(), &mut file)
                .with_context(|| format!("Failed to write {}", filename))?;

            if verbose {
                println!("  → {}", output_path.display());
            }
        }

        Ok(())
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

    /// Check if the model is already cached locally.
    fn check_model_cached(model_cache: &Path) -> bool {
        let required_files = ["config.json", "tokenizer.json", "model.safetensors"];

        required_files.iter().all(|f| model_cache.join(f).exists())
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
    use std::path::PathBuf;

    #[test]
    #[ignore] // Requires model download
    fn test_embed_basic() {
        let model_cache = PathBuf::from("/tmp/sift-test-models");
        let model = EmbeddingModel::load(&model_cache, true).unwrap();

        let text = "This is a test sentence for embedding generation.";
        let embedding = model.embed(text).unwrap();

        assert_eq!(embedding.len(), EMBEDDING_DIM);

        // Verify the embedding contains non-zero values
        let has_non_zero = embedding.iter().any(|&v| v.abs() > 1e-6);
        assert!(has_non_zero, "Embedding should contain non-zero values");
    }
}
