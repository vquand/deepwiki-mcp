use std::env;
use std::io;

use reqwest::blocking::Client;
use serde_json::json;

use crate::domain::{AskResponse, GenerationConfig, RepoScan, SearchResult, WikiPage, WikiStructure};
use crate::index::{fallback_hash_embedding, fallback_hash_embeddings};

pub trait LlmProvider {
    fn plan_wiki(&self, scan: &RepoScan, config: &GenerationConfig) -> io::Result<WikiStructure>;
    fn generate_page(
        &self,
        scan: &RepoScan,
        page: &WikiPage,
        config: &GenerationConfig,
    ) -> io::Result<String>;
    fn synthesize_answer(
        &self,
        query: &str,
        results: &[SearchResult],
        config: &GenerationConfig,
    ) -> io::Result<String>;
}

pub trait EmbeddingProvider {
    fn embed_texts(&self, texts: &[String], config: &GenerationConfig) -> io::Result<Vec<Vec<f32>>>;
    fn embed_query(&self, text: &str, config: &GenerationConfig) -> io::Result<Vec<f32>>;
}

pub struct OpenAiCompatibleProvider {
    client: Client,
    api_key: String,
    base_url: String,
}

impl OpenAiCompatibleProvider {
    pub fn from_env() -> Option<Self> {
        let api_key = env::var("OPENAI_API_KEY").ok()?;
        let base_url = env::var("OPENAI_BASE_URL").unwrap_or_else(|_| "https://api.openai.com/v1".to_string());
        Some(Self {
            client: Client::new(),
            api_key,
            base_url,
        })
    }

    fn model<'a>(&self, config: &'a GenerationConfig) -> &'a str {
        config.model.as_deref().unwrap_or("gpt-4o-mini")
    }

    fn complete(&self, system_prompt: &str, user_prompt: &str, model: &str) -> io::Result<String> {
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let payload = json!({
            "model": model,
            "messages": [
                {
                    "role": "system",
                    "content": system_prompt
                },
                {
                    "role": "user",
                    "content": user_prompt
                }
            ],
            "temperature": 0.2
        });

        let response = self
            .client
            .post(url)
            .bearer_auth(&self.api_key)
            .json(&payload)
            .send()
            .and_then(|response| response.error_for_status())
            .map_err(io::Error::other)?;

        let body: serde_json::Value = response.json().map_err(io::Error::other)?;
        body["choices"][0]["message"]["content"]
            .as_str()
            .map(ToOwned::to_owned)
            .ok_or_else(|| io::Error::other("missing content in provider response"))
    }

    fn embedding_model<'a>(&self, config: &'a GenerationConfig) -> &'a str {
        config.embedding_model.as_deref().unwrap_or("text-embedding-3-small")
    }

    fn embeddings(&self, texts: &[String], model: &str) -> io::Result<Vec<Vec<f32>>> {
        let url = format!("{}/embeddings", self.base_url.trim_end_matches('/'));
        let payload = json!({
            "model": model,
            "input": texts,
        });

        let response = self
            .client
            .post(url)
            .bearer_auth(&self.api_key)
            .json(&payload)
            .send()
            .and_then(|response| response.error_for_status())
            .map_err(io::Error::other)?;

        let body: serde_json::Value = response.json().map_err(io::Error::other)?;
        let data = body["data"]
            .as_array()
            .ok_or_else(|| io::Error::other("missing embeddings data in provider response"))?;

        let mut vectors = Vec::with_capacity(data.len());
        for item in data {
            let embedding = item["embedding"]
                .as_array()
                .ok_or_else(|| io::Error::other("missing embedding vector"))?
                .iter()
                .map(|value| value.as_f64().unwrap_or_default() as f32)
                .collect::<Vec<_>>();
            vectors.push(embedding);
        }
        Ok(vectors)
    }
}

impl LlmProvider for OpenAiCompatibleProvider {
    fn plan_wiki(&self, scan: &RepoScan, config: &GenerationConfig) -> io::Result<WikiStructure> {
        let system_prompt = "You generate structured repository wiki plans in JSON. Return only valid JSON.";
        let file_tree = scan.file_tree.iter().take(200).cloned().collect::<Vec<_>>().join("\n");
        let readme = scan
            .readme_content
            .as_deref()
            .unwrap_or("No README available.")
            .chars()
            .take(10_000)
            .collect::<String>();
        let user_prompt = format!(
            "Analyze this repository and produce a JSON object with fields: id, title, description, sections, root_sections, pages.\n\
             Each section must include id, title, pages, subsections.\n\
             Each page must include id, title, description, importance, file_paths, related_pages, parent_section, content.\n\
             Keep the structure concise and practical. Use 5-8 pages.\n\
             Wiki mode: {}\n\n\
             README:\n{}\n\n\
             FILE TREE:\n{}",
            config.wiki_mode, readme, file_tree
        );

        let content = self.complete(system_prompt, &user_prompt, self.model(config))?;
        serde_json::from_str(&content).map_err(io::Error::other)
    }

    fn generate_page(
        &self,
        scan: &RepoScan,
        page: &WikiPage,
        config: &GenerationConfig,
    ) -> io::Result<String> {
        let system_prompt = "You generate technical wiki markdown. Return markdown only.";
        let repo_summary = scan
            .readme_content
            .as_deref()
            .unwrap_or("No README available.")
            .chars()
            .take(4_000)
            .collect::<String>();
        let file_list = page.file_paths.join("\n");
        let user_prompt = format!(
            "Write a technical wiki page in markdown.\n\
             Page title: {}\n\
             Description: {}\n\
             Repository summary:\n{}\n\n\
             Relevant files:\n{}\n\n\
             Requirements:\n\
             - Start with a <details> block listing relevant files\n\
             - Use concise technical prose\n\
             - Include sections for summary, key files, and implementation notes\n\
             - Do not invent code details not present in the prompt",
            page.title, page.description, repo_summary, file_list
        );

        self.complete(system_prompt, &user_prompt, self.model(config))
    }

    fn synthesize_answer(
        &self,
        query: &str,
        results: &[SearchResult],
        config: &GenerationConfig,
    ) -> io::Result<String> {
        let system_prompt = "You answer repository questions using retrieved snippets. Stay grounded in the provided snippets.";
        let snippets = results
            .iter()
            .map(|result| format!("FILE: {}\nSCORE: {}\n{}\n", result.file_path, result.score, result.snippet))
            .collect::<Vec<_>>()
            .join("\n---\n");
        let user_prompt = format!(
            "Answer this repository question using the retrieved snippets only.\n\
             Question: {}\n\n\
             Snippets:\n{}",
            query, snippets
        );
        self.complete(system_prompt, &user_prompt, self.model(config))
    }
}

impl EmbeddingProvider for OpenAiCompatibleProvider {
    fn embed_texts(&self, texts: &[String], config: &GenerationConfig) -> io::Result<Vec<Vec<f32>>> {
        self.embeddings(texts, self.embedding_model(config))
    }

    fn embed_query(&self, text: &str, config: &GenerationConfig) -> io::Result<Vec<f32>> {
        let mut vectors = self.embeddings(&[text.to_string()], self.embedding_model(config))?;
        vectors
            .pop()
            .ok_or_else(|| io::Error::other("missing query embedding"))
    }
}

pub fn maybe_provider(config: &GenerationConfig) -> Option<Box<dyn LlmProvider>> {
    match config.provider.as_deref() {
        Some("openai") | Some("openai_compatible") => {
            OpenAiCompatibleProvider::from_env().map(|provider| Box::new(provider) as Box<dyn LlmProvider>)
        }
        _ => None,
    }
}

pub fn embed_texts(config: &GenerationConfig, texts: &[String]) -> io::Result<Vec<Vec<f32>>> {
    match config.embedding_provider.as_deref().or(config.provider.as_deref()) {
        Some("openai") | Some("openai_compatible") => {
            if let Some(provider) = OpenAiCompatibleProvider::from_env() {
                provider
                    .embed_texts(texts, config)
                    .or_else(|_| Ok(fallback_hash_embeddings(texts)))
            } else {
                Ok(fallback_hash_embeddings(texts))
            }
        }
        _ => Ok(fallback_hash_embeddings(texts)),
    }
}

pub fn embed_query(config: &GenerationConfig, text: &str) -> io::Result<Vec<f32>> {
    match config.embedding_provider.as_deref().or(config.provider.as_deref()) {
        Some("openai") | Some("openai_compatible") => {
            if let Some(provider) = OpenAiCompatibleProvider::from_env() {
                provider
                    .embed_query(text, config)
                    .or_else(|_| Ok(fallback_hash_embedding(text)))
            } else {
                Ok(fallback_hash_embedding(text))
            }
        }
        _ => Ok(fallback_hash_embedding(text)),
    }
}

pub fn maybe_enhance_ask_response(
    query: &str,
    lexical: AskResponse,
    config: &GenerationConfig,
) -> io::Result<AskResponse> {
    let Some(provider) = maybe_provider(config) else {
        return Ok(lexical);
    };
    if lexical.results.is_empty() {
        return Ok(lexical);
    }

    let answer = provider.synthesize_answer(query, &lexical.results, config)?;
    Ok(AskResponse {
        answer,
        results: lexical.results,
    })
}
