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
    pub num_inputs: usize,          // 2 or 3 — whether model accepts token_type_ids
}

pub const MODELS: &[EmbeddingModel] = &[
    // ── Small / Fast (384d) ──────────────────────────────────────────
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
        num_inputs: 3,
    },
    EmbeddingModel {
        id: "bge-small-en-v1.5",
        name: "BGE Small EN v1.5",
        dim: 384,
        size_mb: 33,
        description: "Better quality than MiniLM, same speed. English only.",
        onnx_url: "https://huggingface.co/Xenova/bge-small-en-v1.5/resolve/main/onnx/model_quantized.onnx",
        tokenizer_url: "https://huggingface.co/Xenova/bge-small-en-v1.5/resolve/main/tokenizer.json",
        requires_prefix: false,
        query_prefix: None,
        num_inputs: 3,
    },
    EmbeddingModel {
        id: "gte-small",
        name: "GTE Small",
        dim: 384,
        size_mb: 32,
        description: "Alibaba GTE. Strong quality at small size. English.",
        onnx_url: "https://huggingface.co/Xenova/gte-small/resolve/main/onnx/model_quantized.onnx",
        tokenizer_url: "https://huggingface.co/Xenova/gte-small/resolve/main/tokenizer.json",
        requires_prefix: false,
        query_prefix: None,
        num_inputs: 3,
    },
    EmbeddingModel {
        id: "e5-small-v2",
        name: "E5 Small v2",
        dim: 384,
        size_mb: 32,
        description: "Microsoft E5. Solid English embeddings. Needs 'query: ' prefix.",
        onnx_url: "https://huggingface.co/Xenova/e5-small-v2/resolve/main/onnx/model_quantized.onnx",
        tokenizer_url: "https://huggingface.co/Xenova/e5-small-v2/resolve/main/tokenizer.json",
        requires_prefix: true,
        query_prefix: Some("query: "),
        num_inputs: 3,
    },

    // ── Medium (512d) ────────────────────────────────────────────────
    EmbeddingModel {
        id: "jina-embeddings-v2-small-en",
        name: "Jina v2 Small EN",
        dim: 512,
        size_mb: 31,
        description: "Jina AI. 8192 token context. Tiny but punches above its weight.",
        onnx_url: "https://huggingface.co/Xenova/jina-embeddings-v2-small-en/resolve/main/onnx/model_quantized.onnx",
        tokenizer_url: "https://huggingface.co/Xenova/jina-embeddings-v2-small-en/resolve/main/tokenizer.json",
        requires_prefix: false,
        query_prefix: None,
        num_inputs: 3,
    },

    // ── Base (768d) ──────────────────────────────────────────────────
    EmbeddingModel {
        id: "bge-base-en-v1.5",
        name: "BGE Base EN v1.5",
        dim: 768,
        size_mb: 110,
        description: "High quality English embeddings. Significant quality bump over small models.",
        onnx_url: "https://huggingface.co/Xenova/bge-base-en-v1.5/resolve/main/onnx/model_quantized.onnx",
        tokenizer_url: "https://huggingface.co/Xenova/bge-base-en-v1.5/resolve/main/tokenizer.json",
        requires_prefix: false,
        query_prefix: None,
        num_inputs: 3,
    },
    EmbeddingModel {
        id: "gte-base",
        name: "GTE Base",
        dim: 768,
        size_mb: 104,
        description: "Alibaba GTE base. Excellent quality for English.",
        onnx_url: "https://huggingface.co/Xenova/gte-base/resolve/main/onnx/model_quantized.onnx",
        tokenizer_url: "https://huggingface.co/Xenova/gte-base/resolve/main/tokenizer.json",
        requires_prefix: false,
        query_prefix: None,
        num_inputs: 3,
    },
    EmbeddingModel {
        id: "jina-embeddings-v2-base-en",
        name: "Jina v2 Base EN",
        dim: 768,
        size_mb: 131,
        description: "Jina AI base. 8192 token context. Great for longer documents.",
        onnx_url: "https://huggingface.co/Xenova/jina-embeddings-v2-base-en/resolve/main/onnx/model_quantized.onnx",
        tokenizer_url: "https://huggingface.co/Xenova/jina-embeddings-v2-base-en/resolve/main/tokenizer.json",
        requires_prefix: false,
        query_prefix: None,
        num_inputs: 3,
    },
    EmbeddingModel {
        id: "snowflake-arctic-embed-m-v2.0",
        name: "Snowflake Arctic Embed M v2",
        dim: 768,
        size_mb: 296,
        description: "Excellent multilingual support. 8192 token context. GTE-based.",
        onnx_url: "https://huggingface.co/Snowflake/snowflake-arctic-embed-m-v2.0/resolve/main/onnx/model_quantized.onnx",
        tokenizer_url: "https://huggingface.co/Snowflake/snowflake-arctic-embed-m-v2.0/resolve/main/tokenizer.json",
        requires_prefix: false,
        query_prefix: Some("query: "),
        num_inputs: 2, // GTE-based, no token_type_ids
    },

    // ── Large / Multilingual (768-1024d) ─────────────────────────────
    EmbeddingModel {
        id: "multilingual-e5-small",
        name: "Multilingual E5 Small",
        dim: 384,
        size_mb: 112,
        description: "Microsoft E5 multilingual. 100+ languages at small model size.",
        onnx_url: "https://huggingface.co/Xenova/multilingual-e5-small/resolve/main/onnx/model_quantized.onnx",
        tokenizer_url: "https://huggingface.co/Xenova/multilingual-e5-small/resolve/main/tokenizer.json",
        requires_prefix: true,
        query_prefix: Some("query: "),
        num_inputs: 3,
    },
    EmbeddingModel {
        id: "multilingual-e5-base",
        name: "Multilingual E5 Base",
        dim: 768,
        size_mb: 265,
        description: "Microsoft E5 multilingual base. 100+ languages, XLM-RoBERTa based.",
        onnx_url: "https://huggingface.co/Xenova/multilingual-e5-base/resolve/main/onnx/model_quantized.onnx",
        tokenizer_url: "https://huggingface.co/Xenova/multilingual-e5-base/resolve/main/tokenizer.json",
        requires_prefix: true,
        query_prefix: Some("query: "),
        num_inputs: 2, // XLM-RoBERTa based
    },
    EmbeddingModel {
        id: "bge-m3",
        name: "BGE M3 (Multi-lingual)",
        dim: 1024,
        size_mb: 570,
        description: "Top-tier multilingual. 100+ languages. Dense + sparse retrieval. Large download.",
        onnx_url: "https://huggingface.co/Xenova/bge-m3/resolve/main/onnx/model_quantized.onnx",
        tokenizer_url: "https://huggingface.co/Xenova/bge-m3/resolve/main/tokenizer.json",
        requires_prefix: false,
        query_prefix: None,
        num_inputs: 2, // XLM-RoBERTa based
    },
];

pub fn get_model(id: &str) -> Option<&'static EmbeddingModel> {
    MODELS.iter().find(|m| m.id == id)
}

pub fn default_model() -> &'static EmbeddingModel {
    &MODELS[0]
}
