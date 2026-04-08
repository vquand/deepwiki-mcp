use std::collections::hash_map::DefaultHasher;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StorageMode {
    Global,
    RepoLocal,
}

impl StorageMode {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "global" => Some(Self::Global),
            "repo_local" => Some(Self::RepoLocal),
            _ => None,
        }
    }
}

impl fmt::Display for StorageMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Global => write!(f, "global"),
            Self::RepoLocal => write!(f, "repo_local"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RepoType {
    Github,
    Gitlab,
    Bitbucket,
    Local,
    Web,
}

impl RepoType {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "github" => Some(Self::Github),
            "gitlab" => Some(Self::Gitlab),
            "bitbucket" => Some(Self::Bitbucket),
            "local" => Some(Self::Local),
            "web" => Some(Self::Web),
            _ => None,
        }
    }
}

impl fmt::Display for RepoType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Github => write!(f, "github"),
            Self::Gitlab => write!(f, "gitlab"),
            Self::Bitbucket => write!(f, "bitbucket"),
            Self::Local => write!(f, "local"),
            Self::Web => write!(f, "web"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RepoSource {
    Remote { url: String },
    Local { path: PathBuf },
}

impl RepoSource {
    pub fn normalized_key(&self) -> String {
        match self {
            Self::Remote { url } => url.trim().trim_end_matches('/').to_ascii_lowercase(),
            Self::Local { path } => normalize_path(path),
        }
    }

    pub fn local_path(&self) -> Option<&Path> {
        match self {
            Self::Local { path } => Some(path.as_path()),
            Self::Remote { .. } => None,
        }
    }

    pub fn display(&self) -> String {
        match self {
            Self::Remote { url } => url.clone(),
            Self::Local { path } => path.display().to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationConfig {
    pub language: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub embedding_provider: Option<String>,
    pub embedding_model: Option<String>,
    pub excluded_dirs: Vec<String>,
    pub excluded_files: Vec<String>,
    pub included_dirs: Vec<String>,
    pub included_files: Vec<String>,
    pub wiki_mode: String,
}

impl Default for GenerationConfig {
    fn default() -> Self {
        Self {
            language: "en".to_string(),
            provider: None,
            model: None,
            embedding_provider: None,
            embedding_model: None,
            excluded_dirs: Vec::new(),
            excluded_files: Vec::new(),
            included_dirs: Vec::new(),
            included_files: Vec::new(),
            wiki_mode: "comprehensive".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoScan {
    pub scanned_at_epoch_ms: u128,
    pub root_path: PathBuf,
    pub default_branch: Option<String>,
    pub readme_path: Option<PathBuf>,
    pub readme_content: Option<String>,
    pub file_tree: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiPage {
    pub id: String,
    pub title: String,
    pub description: String,
    pub importance: String,
    pub file_paths: Vec<String>,
    pub related_pages: Vec<String>,
    pub parent_section: Option<String>,
    pub content: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiSection {
    pub id: String,
    pub title: String,
    pub pages: Vec<String>,
    pub subsections: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiStructure {
    pub id: String,
    pub title: String,
    pub description: String,
    pub sections: Vec<WikiSection>,
    pub root_sections: Vec<String>,
    pub pages: Vec<WikiPage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexChunk {
    pub id: String,
    pub file_path: String,
    pub text: String,
    #[serde(default)]
    pub vector: Vec<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub file_path: String,
    pub score: f32,
    pub snippet: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AskResponse {
    pub answer: String,
    pub results: Vec<SearchResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectState {
    pub repo_fetched: bool,
    pub repo_scanned: bool,
    pub index_built: bool,
    pub wiki_planned: bool,
    pub wiki_generated: bool,
    pub slides_generated: bool,
    pub workshop_generated: bool,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectMetadata {
    pub project_id: String,
    pub source: RepoSource,
    pub repo_type: RepoType,
    pub storage_mode: StorageMode,
    pub root_path: PathBuf,
    pub repo_path: PathBuf,
    pub default_branch: Option<String>,
    pub created_at_epoch_ms: u128,
    pub updated_at_epoch_ms: u128,
    pub generation_config: GenerationConfig,
    pub state: ProjectState,
}

impl ProjectMetadata {
    pub fn derive_project_id(source: &RepoSource, storage_mode: &StorageMode) -> String {
        let mut hasher = DefaultHasher::new();
        source.normalized_key().hash(&mut hasher);
        storage_mode.to_string().hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    }
}

fn normalize_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("/")
}
