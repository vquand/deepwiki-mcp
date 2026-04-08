use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io;
use std::path::Path;

use crate::domain::{AskResponse, GenerationConfig, IndexChunk, SearchResult};
use crate::provider::{embed_texts, embed_query};
use crate::storage::{load_json, save_json_pretty};

const MAX_FILE_BYTES: usize = 128 * 1024;
const MAX_CHUNK_CHARS: usize = 2400;
const HASH_EMBED_DIM: usize = 256;
const READABLE_EXTENSIONS: &[&str] = &[
    "rs", "py", "ts", "tsx", "js", "jsx", "md", "json", "toml", "yml", "yaml", "css", "html",
    "txt", "sh",
];

pub fn build_index(
    repo_root: &Path,
    file_tree: &[String],
    output_path: &Path,
    config: &GenerationConfig,
) -> io::Result<Vec<IndexChunk>> {
    let mut chunks = Vec::new();

    for file_path in file_tree {
        if !is_indexable(file_path) {
            continue;
        }

        let absolute = repo_root.join(file_path);
        let metadata = match fs::metadata(&absolute) {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };
        if metadata.len() as usize > MAX_FILE_BYTES {
            continue;
        }

        let content = match fs::read_to_string(&absolute) {
            Ok(content) => content,
            Err(_) => continue,
        };

        if content.trim().is_empty() {
            continue;
        }

        for (index, chunk) in chunk_text(&content).into_iter().enumerate() {
            chunks.push(IndexChunk {
                id: format!("{}#{}", file_path, index + 1),
                file_path: file_path.clone(),
                text: chunk,
                vector: Vec::new(),
            });
        }
    }

    let texts: Vec<String> = chunks.iter().map(|chunk| chunk.text.clone()).collect();
    let vectors = embed_texts(config, &texts)?;
    for (chunk, vector) in chunks.iter_mut().zip(vectors.into_iter()) {
        chunk.vector = vector;
    }

    save_json_pretty(output_path, &chunks)?;
    Ok(chunks)
}

pub fn ask_index(
    index_path: &Path,
    query: &str,
    top_k: usize,
    config: &GenerationConfig,
) -> io::Result<AskResponse> {
    let chunks: Vec<IndexChunk> = load_json(index_path)?;
    let query_vector = embed_query(config, query)?;
    let mut scored: Vec<(f32, &IndexChunk)> = chunks
        .iter()
        .map(|chunk| (cosine_similarity(&query_vector, &chunk.vector), chunk))
        .filter(|(score, _)| *score > 0.0)
        .collect();

    scored.sort_by(|left, right| {
        right
            .0
            .partial_cmp(&left.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.1.file_path.cmp(&right.1.file_path))
    });

    let results: Vec<SearchResult> = scored
        .into_iter()
        .take(top_k)
        .map(|(score, chunk)| SearchResult {
            file_path: chunk.file_path.clone(),
            score,
            snippet: first_lines(&chunk.text, 12),
        })
        .collect();

    let answer = if results.is_empty() {
        format!("No embedding matches found for query: {query}")
    } else {
        let mut answer = format!("Top embedding matches for query: {query}\n\n");
        for result in &results {
            answer.push_str(&format!(
                "- {} (score: {:.4})\n{}\n\n",
                result.file_path, result.score, result.snippet
            ));
        }
        answer
    };

    Ok(AskResponse { answer, results })
}

pub fn fallback_hash_embeddings(texts: &[String]) -> Vec<Vec<f32>> {
    texts.iter().map(|text| fallback_hash_embedding(text)).collect()
}

pub fn fallback_hash_embedding(text: &str) -> Vec<f32> {
    let mut vector = vec![0.0_f32; HASH_EMBED_DIM];
    for token in tokenize(text) {
        let mut hasher = DefaultHasher::new();
        token.hash(&mut hasher);
        let hash = hasher.finish() as usize;
        let index = hash % HASH_EMBED_DIM;
        vector[index] += 1.0;
    }
    normalize(vector)
}

fn is_indexable(file_path: &str) -> bool {
    if let Some(extension) = Path::new(file_path).extension().and_then(|value| value.to_str()) {
        READABLE_EXTENSIONS.iter().any(|allowed| *allowed == extension)
    } else {
        false
    }
}

fn chunk_text(text: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();

    for line in text.lines() {
        if current.len() + line.len() + 1 > MAX_CHUNK_CHARS && !current.is_empty() {
            chunks.push(current.trim().to_string());
            current.clear();
        }
        current.push_str(line);
        current.push('\n');
    }

    if !current.trim().is_empty() {
        chunks.push(current.trim().to_string());
    }

    chunks
}

fn tokenize(text: &str) -> Vec<String> {
    text.split(|character: char| !character.is_alphanumeric() && character != '_')
        .filter(|token| token.len() >= 3)
        .map(|token| token.to_ascii_lowercase())
        .collect()
}

fn cosine_similarity(left: &[f32], right: &[f32]) -> f32 {
    if left.is_empty() || right.is_empty() || left.len() != right.len() {
        return 0.0;
    }

    let mut dot = 0.0_f32;
    let mut left_norm = 0.0_f32;
    let mut right_norm = 0.0_f32;

    for (l, r) in left.iter().zip(right.iter()) {
        dot += l * r;
        left_norm += l * l;
        right_norm += r * r;
    }

    if left_norm == 0.0 || right_norm == 0.0 {
        0.0
    } else {
        dot / (left_norm.sqrt() * right_norm.sqrt())
    }
}

fn normalize(mut vector: Vec<f32>) -> Vec<f32> {
    let norm = vector.iter().map(|value| value * value).sum::<f32>().sqrt();
    if norm > 0.0 {
        for value in &mut vector {
            *value /= norm;
        }
    }
    vector
}

fn first_lines(text: &str, max_lines: usize) -> String {
    text.lines().take(max_lines).collect::<Vec<_>>().join("\n")
}
