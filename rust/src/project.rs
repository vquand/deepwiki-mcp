use std::io;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::domain::{
    AskResponse, GenerationConfig, ProjectMetadata, ProjectState, RepoScan, RepoSource, RepoType,
    StorageMode, WikiStructure,
};
use crate::export::export_wiki_markdown;
use crate::index::{ask_index, build_index};
use crate::provider::{maybe_enhance_ask_response, maybe_provider};
use crate::repo::scan_local_repository;
use crate::remote::{git_clone, git_pull};
use crate::storage::{
    default_global_root, list_project_files, load_json, load_project_metadata, save_json_pretty,
    save_project_metadata, ProjectPaths, INDEX_FILE_NAME, REPO_SCAN_FILE_NAME, WIKI_CACHE_FILE_NAME,
    WIKI_STRUCTURE_FILE_NAME,
};
use crate::wiki::{plan_wiki, render_page_markdown};

pub struct CreateProjectRequest {
    pub source: RepoSource,
    pub repo_type: RepoType,
    pub storage_mode: StorageMode,
    pub generation_config: GenerationConfig,
}

pub struct ProjectService {
    global_root: PathBuf,
}

impl ProjectService {
    pub fn new() -> io::Result<Self> {
        Ok(Self {
            global_root: default_global_root()?,
        })
    }

    pub fn create_project(&self, request: CreateProjectRequest) -> io::Result<ProjectMetadata> {
        let project_id = ProjectMetadata::derive_project_id(&request.source, &request.storage_mode);
        let paths = ProjectPaths::for_project(
            &self.global_root,
            &request.storage_mode,
            &project_id,
            &request.source,
        )?;
        paths.ensure_layout()?;

        let now = current_epoch_ms()?;
        let metadata = ProjectMetadata {
            project_id,
            source: request.source,
            repo_type: request.repo_type,
            storage_mode: request.storage_mode,
            root_path: paths.root,
            repo_path: paths.repo,
            default_branch: None,
            created_at_epoch_ms: now,
            updated_at_epoch_ms: now,
            generation_config: request.generation_config,
            state: ProjectState::default(),
        };

        save_project_metadata(&metadata)?;
        Ok(metadata)
    }

    pub fn list_global_projects(&self) -> io::Result<Vec<ProjectMetadata>> {
        let mut projects = Vec::new();
        for project_file in list_project_files(&self.global_root)? {
            let metadata = load_project_metadata(&project_file)?;
            projects.push(metadata);
        }

        projects.sort_by(|left, right| {
            right
                .updated_at_epoch_ms
                .cmp(&left.updated_at_epoch_ms)
                .then_with(|| left.project_id.cmp(&right.project_id))
        });
        Ok(projects)
    }

    pub fn load_global_project(&self, project_id: &str) -> io::Result<ProjectMetadata> {
        let project_file = self.global_root.join(project_id).join("project.json");
        load_project_metadata(&project_file)
    }

    pub fn configure_project(
        &self,
        project_id: &str,
        provider: Option<String>,
        model: Option<String>,
        embedding_provider: Option<String>,
        embedding_model: Option<String>,
    ) -> io::Result<ProjectMetadata> {
        let mut metadata = self.load_global_project(project_id)?;
        metadata.generation_config.provider = provider;
        metadata.generation_config.model = model;
        metadata.generation_config.embedding_provider = embedding_provider;
        metadata.generation_config.embedding_model = embedding_model;
        metadata.updated_at_epoch_ms = current_epoch_ms()?;
        save_project_metadata(&metadata)?;
        Ok(metadata)
    }

    pub fn scan_project(&self, project_id: &str) -> io::Result<RepoScan> {
        let mut metadata = self.load_global_project(project_id)?;
        self.fetch_project(project_id)?;
        let scan = scan_local_repository(&metadata.repo_path)?;
        metadata.default_branch = scan.default_branch.clone();
        metadata.updated_at_epoch_ms = current_epoch_ms()?;
        metadata.state.repo_scanned = true;
        metadata.state.last_error = None;
        save_project_metadata(&metadata)?;
        save_json_pretty(&metadata.root_path.join(REPO_SCAN_FILE_NAME), &scan)?;
        Ok(scan)
    }

    pub fn fetch_project(&self, project_id: &str) -> io::Result<()> {
        let mut metadata = self.load_global_project(project_id)?;
        match (&metadata.source, &metadata.storage_mode) {
            (RepoSource::Remote { url }, StorageMode::Global) => {
                if metadata.repo_path.exists() && metadata.repo_path.join(".git").exists() {
                    git_pull(&metadata.repo_path)?;
                } else {
                    if let Some(parent) = metadata.repo_path.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    git_clone(url, &metadata.repo_path)?;
                }
                metadata.state.repo_fetched = true;
            }
            (RepoSource::Local { path }, _) => {
                if !path.exists() {
                    return Err(io::Error::new(
                        io::ErrorKind::NotFound,
                        format!("local repository path not found: {}", path.display()),
                    ));
                }
                metadata.state.repo_fetched = true;
            }
            (RepoSource::Remote { .. }, StorageMode::RepoLocal) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "repo_local mode does not support remote repositories",
                ));
            }
        }

        metadata.updated_at_epoch_ms = current_epoch_ms()?;
        metadata.state.last_error = None;
        save_project_metadata(&metadata)
    }

    pub fn build_project_index(&self, project_id: &str) -> io::Result<usize> {
        let mut metadata = self.load_global_project(project_id)?;
        let scan: RepoScan = {
            let scan_path = metadata.root_path.join(REPO_SCAN_FILE_NAME);
            if scan_path.exists() {
                load_json(&scan_path)?
            } else {
                self.scan_project(project_id)?
            }
        };

        let index_path = metadata.root_path.join("index").join(INDEX_FILE_NAME);
        let chunks = build_index(
            &scan.root_path,
            &scan.file_tree,
            &index_path,
            &metadata.generation_config,
        )?;
        metadata.updated_at_epoch_ms = current_epoch_ms()?;
        metadata.state.index_built = true;
        metadata.state.last_error = None;
        save_project_metadata(&metadata)?;
        Ok(chunks.len())
    }

    pub fn ask_project(&self, project_id: &str, query: &str) -> io::Result<AskResponse> {
        let metadata = self.load_global_project(project_id)?;
        let index_path = metadata.root_path.join("index").join(INDEX_FILE_NAME);
        if !index_path.exists() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "project index not found; run build-index first",
            ));
        }
        let lexical = ask_index(&index_path, query, 5, &metadata.generation_config)?;
        maybe_enhance_ask_response(query, lexical, &metadata.generation_config)
    }

    pub fn plan_project_wiki(&self, project_id: &str) -> io::Result<WikiStructure> {
        let mut metadata = self.load_global_project(project_id)?;
        let scan: RepoScan = {
            let scan_path = metadata.root_path.join(REPO_SCAN_FILE_NAME);
            if scan_path.exists() {
                let content = std::fs::read_to_string(&scan_path)?;
                serde_json::from_str(&content).map_err(io::Error::other)?
            } else {
                self.scan_project(project_id)?
            }
        };

        let structure = if let Some(provider) = maybe_provider(&metadata.generation_config) {
            provider
                .plan_wiki(&scan, &metadata.generation_config)
                .unwrap_or_else(|_| {
                    plan_wiki(
                        &scan,
                        metadata.generation_config.wiki_mode == "comprehensive",
                    )
                })
        } else {
            plan_wiki(
                &scan,
                metadata.generation_config.wiki_mode == "comprehensive",
            )
        };
        metadata.updated_at_epoch_ms = current_epoch_ms()?;
        metadata.state.wiki_planned = true;
        metadata.state.last_error = None;
        save_project_metadata(&metadata)?;
        save_json_pretty(
            &metadata.root_path.join("wiki").join(WIKI_STRUCTURE_FILE_NAME),
            &structure,
        )?;
        Ok(structure)
    }

    pub fn generate_project_wiki(&self, project_id: &str) -> io::Result<WikiStructure> {
        let mut metadata = self.load_global_project(project_id)?;
        let scan: RepoScan = {
            let scan_path = metadata.root_path.join(REPO_SCAN_FILE_NAME);
            if scan_path.exists() {
                let content = std::fs::read_to_string(&scan_path)?;
                serde_json::from_str(&content).map_err(io::Error::other)?
            } else {
                self.scan_project(project_id)?
            }
        };

        let mut structure = {
            let structure_path = metadata.root_path.join("wiki").join(WIKI_STRUCTURE_FILE_NAME);
            if structure_path.exists() {
                let content = std::fs::read_to_string(&structure_path)?;
                serde_json::from_str(&content).map_err(io::Error::other)?
            } else {
                self.plan_project_wiki(project_id)?
            }
        };

        for page in &mut structure.pages {
            page.content = Some(if let Some(provider) = maybe_provider(&metadata.generation_config) {
                provider
                    .generate_page(&scan, page, &metadata.generation_config)
                    .unwrap_or_else(|_| render_page_markdown(page, &scan))
            } else {
                render_page_markdown(page, &scan)
            });
            let page_path = metadata
                .root_path
                .join("wiki")
                .join("pages")
                .join(format!("{}.md", page.id));
            if let Some(content) = &page.content {
                std::fs::write(page_path, content)?;
            }
        }

        export_wiki_markdown(
            &metadata.root_path.join("wiki").join("index.md"),
            &structure,
        )?;
        save_json_pretty(
            &metadata.root_path.join("wiki").join(WIKI_CACHE_FILE_NAME),
            &structure,
        )?;

        metadata.updated_at_epoch_ms = current_epoch_ms()?;
        metadata.state.wiki_generated = true;
        metadata.state.last_error = None;
        save_project_metadata(&metadata)?;
        save_json_pretty(
            &metadata.root_path.join("wiki").join(WIKI_STRUCTURE_FILE_NAME),
            &structure,
        )?;

        Ok(structure)
    }

    pub fn export_project_wiki_markdown(&self, project_id: &str) -> io::Result<std::path::PathBuf> {
        let metadata = self.load_global_project(project_id)?;
        let structure: WikiStructure = load_json(&metadata.root_path.join("wiki").join(WIKI_STRUCTURE_FILE_NAME))?;
        let output_path = metadata.root_path.join("wiki").join("export.md");
        export_wiki_markdown(&output_path, &structure)?;
        Ok(output_path)
    }

    pub fn generate_slides(&self, project_id: &str) -> io::Result<std::path::PathBuf> {
        let mut metadata = self.load_global_project(project_id)?;
        let structure: WikiStructure = load_json(&metadata.root_path.join("wiki").join(WIKI_STRUCTURE_FILE_NAME))?;
        let output_path = metadata.root_path.join("artifacts").join("slides.md");
        std::fs::write(&output_path, crate::artifacts::generate_slides_markdown(&structure))?;
        metadata.updated_at_epoch_ms = current_epoch_ms()?;
        metadata.state.slides_generated = true;
        save_project_metadata(&metadata)?;
        Ok(output_path)
    }

    pub fn generate_workshop(&self, project_id: &str) -> io::Result<std::path::PathBuf> {
        let mut metadata = self.load_global_project(project_id)?;
        let structure: WikiStructure = load_json(&metadata.root_path.join("wiki").join(WIKI_STRUCTURE_FILE_NAME))?;
        let output_path = metadata.root_path.join("artifacts").join("workshop.md");
        std::fs::write(
            &output_path,
            crate::artifacts::generate_workshop_markdown(&structure),
        )?;
        metadata.updated_at_epoch_ms = current_epoch_ms()?;
        metadata.state.workshop_generated = true;
        save_project_metadata(&metadata)?;
        Ok(output_path)
    }

    pub fn global_root(&self) -> &PathBuf {
        &self.global_root
    }
}

fn current_epoch_ms() -> io::Result<u128> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .map_err(io::Error::other)
}
