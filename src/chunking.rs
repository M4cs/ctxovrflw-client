/// Split long text into overlapping chunks by character boundaries.
/// Keeps chunks around `max_chars` with `overlap_chars` context carry-over.
pub fn split_text_with_overlap(text: &str, max_chars: usize, overlap_chars: usize) -> Vec<String> {
    if text.chars().count() <= max_chars {
        return vec![text.to_string()];
    }

    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut chunks = Vec::new();

    let mut start = 0usize;
    while start < len {
        let mut end = (start + max_chars).min(len);

        // Prefer splitting on a whitespace near the end for cleaner chunks.
        if end < len {
            let window_start = end.saturating_sub(120);
            if let Some(split_at) = (window_start..end).rev().find(|&i| chars[i].is_whitespace()) {
                if split_at > start + max_chars / 2 {
                    end = split_at;
                }
            }
        }

        let chunk: String = chars[start..end].iter().collect::<String>().trim().to_string();
        if !chunk.is_empty() {
            chunks.push(chunk);
        }

        if end >= len {
            break;
        }

        let stride = max_chars.saturating_sub(overlap_chars).max(1);
        start = start.saturating_add(stride);
    }

    chunks
}
