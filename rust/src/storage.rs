use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::domain::{ProjectMetadata, RepoSource, StorageMode};

pub const PROJECT_FILE_NAME: &str = "project.json";
pub const REPO_SCAN_FILE_NAME: &str = "repo_scan.json";
pub const INDEX_FILE_NAME: &str = "chunks.json";
pub const WIKI_STRUCTURE_FILE_NAME: &str = "structure.json";
pub const WIKI_CACHE_FILE_NAME: &str = "cache.json";

#[derive(Debug, Clone)]
pub struct ProjectPaths {
    pub root: PathBuf,
    pub repo: PathBuf,
    pub index: PathBuf,
    pub wiki_dir: PathBuf,
    pub wiki_pages_dir: PathBuf,
    pub artifacts_dir: PathBuf,
    pub logs_dir: PathBuf,
}

impl ProjectPaths {
    pub fn for_project(
        global_root: &Path,
        storage_mode: &StorageMode,
        project_id: &str,
        source: &RepoSource,
    ) -> io::Result<Self> {
        let root = match storage_mode {
            StorageMode::Global => global_root.join(project_id),
            StorageMode::RepoLocal => {
                let repo_root = source.local_path().ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "repo_local storage mode requires a local repository path",
                    )
                })?;
                repo_root.join(".deepwiki-mcp")
            }
        };

        let repo = match (storage_mode, source) {
            (StorageMode::Global, RepoSource::Remote { .. }) => root.join("repo"),
            (StorageMode::Global, RepoSource::Local { path }) => path.to_path_buf(),
            (StorageMode::RepoLocal, _) => source
                .local_path()
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "repo_local storage mode requires a local repository path",
                    )
                })?
                .to_path_buf(),
        };

        let wiki_dir = root.join("wiki");
        Ok(Self {
            root: root.clone(),
            repo,
            index: root.join("index"),
            wiki_pages_dir: wiki_dir.join("pages"),
            artifacts_dir: root.join("artifacts"),
            logs_dir: root.join("logs"),
            wiki_dir,
        })
    }

    pub fn ensure_layout(&self) -> io::Result<()> {
        fs::create_dir_all(&self.root)?;
        fs::create_dir_all(&self.index)?;
        fs::create_dir_all(&self.wiki_dir)?;
        fs::create_dir_all(&self.wiki_pages_dir)?;
        fs::create_dir_all(&self.artifacts_dir)?;
        fs::create_dir_all(&self.logs_dir)?;
        Ok(())
    }
}

pub fn default_global_root() -> io::Result<PathBuf> {
    let home = std::env::var_os("HOME").ok_or_else(|| {
        io::Error::new(io::ErrorKind::NotFound, "HOME environment variable is not set")
    })?;
    Ok(PathBuf::from(home).join(".deepwiki-mcp"))
}

pub fn save_project_metadata(metadata: &ProjectMetadata) -> io::Result<()> {
    let content = serde_json::to_string_pretty(metadata).map_err(io::Error::other)?;
    fs::write(&metadata.root_path.join(PROJECT_FILE_NAME), content)
}

pub fn load_project_metadata(project_file: &Path) -> io::Result<ProjectMetadata> {
    let content = fs::read_to_string(project_file)?;
    serde_json::from_str(&content).map_err(io::Error::other)
}

pub fn list_project_files(global_root: &Path) -> io::Result<Vec<PathBuf>> {
    if !global_root.exists() {
        return Ok(Vec::new());
    }

    let mut project_files = Vec::new();
    for entry in fs::read_dir(global_root)? {
        let entry = entry?;
        let path = entry.path().join(PROJECT_FILE_NAME);
        if path.exists() {
            project_files.push(path);
        }
    }

    project_files.sort();
    Ok(project_files)
}

pub fn save_json_pretty<T: serde::Serialize>(path: &Path, value: &T) -> io::Result<()> {
    let content = serde_json::to_string_pretty(value).map_err(io::Error::other)?;
    fs::write(path, content)
}

pub fn load_json<T: serde::de::DeserializeOwned>(path: &Path) -> io::Result<T> {
    let content = fs::read_to_string(path)?;
    serde_json::from_str(&content).map_err(io::Error::other)
}
