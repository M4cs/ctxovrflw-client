pub struct EmbeddingModel {
    pub id: &'static str,           // "all-MiniLM-L6-v2"
    pub name: &'static str,         // "MiniLM v2 (Default)"
    pub dim: usize,                 // 384
    pub size_mb: usize,             // 23
    pub description: &'static str,  // "Fast, lightweight. Good for English."
    pub onnx_url: &'static str,     // HuggingFace URL to quantized ONNX
    pub tokenizer_url: &'static str, // HuggingFace URL to tokenizer.json
    pub requires_prefix: bool,      // Some models need "query: " or "passage: " prefix
    pub query_prefix: Option<&'static str>,  // e.g. Some("query: ")
}

pub const MODELS: &[EmbeddingModel] = &[
    EmbeddingModel {
        id: "all-MiniLM-L6-v2",
        name: "MiniLM v2 (Default)",
        dim: 384,
        size_mb: 23,
        description: "Fast and lightweight. Great for English text. Recommended for most users.",
        onnx_url: "https://huggingface.co/Xenova/all-MiniLM-L6-v2/resolve/main/onnx/model_quantized.onnx",
        tokenizer_url: "https://huggingface.co/Xenova/all-MiniLM-L6-v2/resolve/main/tokenizer.json",
        requires_prefix: false,
        query_prefix: None,
    },
    EmbeddingModel {
        id: "bge-small-en-v1.5",
        name: "BGE Small EN v1.5",
        dim: 384,
        size_mb: 33,
        description: "Better quality than MiniLM, same speed. English only.",
        // NOTE: Need to verify this ONNX URL exists - may need adjustment
        onnx_url: "https://huggingface.co/Xenova/bge-small-en-v1.5/resolve/main/onnx/model_quantized.onnx",
        tokenizer_url: "https://huggingface.co/Xenova/bge-small-en-v1.5/resolve/main/tokenizer.json",
        requires_prefix: false,
        query_prefix: None,
    },
    EmbeddingModel {
        id: "bge-base-en-v1.5",
        name: "BGE Base EN v1.5",
        dim: 768,
        size_mb: 110,
        description: "High quality English embeddings. Significant quality bump over small models.",
        // NOTE: Need to verify this ONNX URL exists - may need adjustment
        onnx_url: "https://huggingface.co/Xenova/bge-base-en-v1.5/resolve/main/onnx/model_quantized.onnx",
        tokenizer_url: "https://huggingface.co/Xenova/bge-base-en-v1.5/resolve/main/tokenizer.json",
        requires_prefix: false,
        query_prefix: None,
    },
    EmbeddingModel {
        id: "snowflake-arctic-embed-m-v2.0",
        name: "Snowflake Arctic Embed M v2",
        dim: 768,
        size_mb: 113,
        description: "Excellent multilingual support. Compression-friendly. 8192 token context.",
        // NOTE: Need to verify this ONNX URL exists - may need adjustment
        onnx_url: "https://huggingface.co/Xenova/snowflake-arctic-embed-m-v2.0/resolve/main/onnx/model_quantized.onnx",
        tokenizer_url: "https://huggingface.co/Xenova/snowflake-arctic-embed-m-v2.0/resolve/main/tokenizer.json",
        requires_prefix: false,
        query_prefix: Some("query: "),
    },
    EmbeddingModel {
        id: "bge-m3",
        name: "BGE M3 (Multi-lingual)",
        dim: 1024,
        size_mb: 570,
        description: "Top-tier multilingual. 100+ languages. Dense + sparse retrieval. Large download.",
        // NOTE: Need to verify this ONNX URL exists - may need adjustment
        onnx_url: "https://huggingface.co/Xenova/bge-m3/resolve/main/onnx/model_quantized.onnx",
        tokenizer_url: "https://huggingface.co/Xenova/bge-m3/resolve/main/tokenizer.json",
        requires_prefix: false,
        query_prefix: None,
    },
];

pub fn get_model(id: &str) -> Option<&'static EmbeddingModel> {
    MODELS.iter().find(|m| m.id == id)
}

pub fn default_model() -> &'static EmbeddingModel {
    &MODELS[0]
}