"""ProjectService: the orchestration layer behind the MCP tool handlers.

Responsibilities:
- Derive project IDs and paths
- Persist metadata (with pointer files for repo_local discoverability)
- Shell out to git for remote repositories
- Walk the filesystem via `scan_local_repository`
- Store host-LLM-authored wiki plans, pages, slides, and workshops
"""
from __future__ import annotations

import re
import shutil
import time
from pathlib import Path
from typing import Optional

# Safe slug for filenames: alphanumeric, underscore, hyphen. Starts with an
# alphanumeric. Max 64 chars. This rejects `..`, `/`, `\`, leading dots, and
# any character that could redirect writes outside wiki/pages/.
_PAGE_ID_PATTERN = re.compile(r"^[A-Za-z0-9][A-Za-z0-9_-]{0,63}$")


def _validate_page_id(page_id: str) -> None:
    if not _PAGE_ID_PATTERN.fullmatch(page_id):
        raise ValueError(
            f"invalid page id {page_id!r}: must match ^[A-Za-z0-9][A-Za-z0-9_-]{{0,63}}$"
        )

from .models import (
    GenerationConfig,
    ProjectMetadata,
    ProjectState,
    RepoScan,
    RepoSource,
    RepoType,
    StorageMode,
    WikiStructure,
)
from .repo import git_clone, git_pull, scan_local_repository
from .storage import (
    PROJECT_FILE_NAME,
    REPO_SCAN_FILE_NAME,
    WIKI_STRUCTURE_FILE_NAME,
    ProjectPaths,
    default_global_root,
    derive_project_id,
    list_project_files,
    load_project_metadata,
    resolve_project_file,
    save_project_metadata,
    save_project_pointer,
)
from .wiki import plan_wiki, render_wiki_index


def _now_ms() -> int:
    return int(time.time() * 1000)


class ProjectNotFoundError(FileNotFoundError):
    pass


class ProjectService:
    def __init__(self, global_root: Path | None = None) -> None:
        self.global_root = global_root or default_global_root()
        self.global_root.mkdir(parents=True, exist_ok=True)

    # ---- metadata CRUD -------------------------------------------------

    def create_project(
        self,
        source: RepoSource,
        repo_type: RepoType,
        storage_mode: StorageMode,
        generation_config: GenerationConfig | None = None,
    ) -> ProjectMetadata:
        gen_config = generation_config or GenerationConfig()

        # Reuse an existing project if one already matches this source + mode.
        # This keeps create_project idempotent AND preserves compatibility with
        # projects created by the legacy Rust binary (which used a different
        # hash algorithm — DefaultHasher / SipHash — that is not portable to
        # Python). We match on the normalized source key rather than re-deriving
        # the id so ids minted by the old implementation remain usable.
        normalized = source.normalized_key()
        for existing in self.list_projects():
            if (
                existing.storage_mode == storage_mode
                and existing.source.normalized_key() == normalized
            ):
                existing.updated_at_epoch_ms = _now_ms()
                save_project_metadata(existing)
                return existing

        project_id = derive_project_id(source, storage_mode)
        paths = ProjectPaths(self.global_root, storage_mode, project_id, source)
        paths.ensure_layout()

        now = _now_ms()
        metadata = ProjectMetadata(
            project_id=project_id,
            source=source,
            repo_type=repo_type,
            storage_mode=storage_mode,
            root_path=paths.root,
            repo_path=paths.repo,
            default_branch=None,
            created_at_epoch_ms=now,
            updated_at_epoch_ms=now,
            generation_config=gen_config,
            state=ProjectState(),
        )
        save_project_metadata(metadata)
        if storage_mode == StorageMode.REPO_LOCAL:
            save_project_pointer(
                self.global_root,
                project_id,
                metadata.root_path / PROJECT_FILE_NAME,
            )
        return metadata

    def list_projects(self) -> list[ProjectMetadata]:
        projects: list[ProjectMetadata] = []
        for project_file in list_project_files(self.global_root):
            try:
                projects.append(load_project_metadata(project_file))
            except (OSError, ValueError):
                continue
        projects.sort(
            key=lambda meta: (-meta.updated_at_epoch_ms, meta.project_id),
        )
        return projects

    def load_project(self, project_id: str) -> ProjectMetadata:
        try:
            project_file = resolve_project_file(self.global_root, project_id)
        except FileNotFoundError as err:
            raise ProjectNotFoundError(str(err)) from err
        return load_project_metadata(project_file)

    def configure_project(
        self,
        project_id: str,
        language: Optional[str] = None,
        wiki_mode: Optional[str] = None,
        excluded_dirs: Optional[list[str]] = None,
        excluded_files: Optional[list[str]] = None,
        included_dirs: Optional[list[str]] = None,
        included_files: Optional[list[str]] = None,
    ) -> ProjectMetadata:
        metadata = self.load_project(project_id)
        if language is not None:
            metadata.generation_config.language = language
        if wiki_mode is not None:
            if wiki_mode not in {"comprehensive", "concise"}:
                raise ValueError("wiki_mode must be 'comprehensive' or 'concise'")
            metadata.generation_config.wiki_mode = wiki_mode  # type: ignore[assignment]
        if excluded_dirs is not None:
            metadata.generation_config.excluded_dirs = excluded_dirs
        if excluded_files is not None:
            metadata.generation_config.excluded_files = excluded_files
        if included_dirs is not None:
            metadata.generation_config.included_dirs = included_dirs
        if included_files is not None:
            metadata.generation_config.included_files = included_files
        metadata.updated_at_epoch_ms = _now_ms()
        save_project_metadata(metadata)
        return metadata

    def delete_project(self, project_id: str) -> None:
        metadata = self.load_project(project_id)
        if metadata.root_path.exists():
            # Don't nuke a user's repo working tree in repo_local mode — only
            # clean our own .deepwiki-mcp subdirectory.
            shutil.rmtree(metadata.root_path, ignore_errors=True)
        pointer_dir = self.global_root / project_id
        if pointer_dir.exists() and pointer_dir != metadata.root_path:
            shutil.rmtree(pointer_dir, ignore_errors=True)

    # ---- repository ops ------------------------------------------------

    def fetch_repository(self, project_id: str) -> ProjectMetadata:
        metadata = self.load_project(project_id)
        if metadata.source.kind == "remote":
            if metadata.storage_mode == StorageMode.REPO_LOCAL:
                raise ValueError("repo_local mode does not support remote repositories")
            url = metadata.source.url or ""
            if not url:
                raise ValueError("remote source missing url")
            if metadata.repo_path.exists() and (metadata.repo_path / ".git").exists():
                git_pull(metadata.repo_path)
            else:
                git_clone(url, metadata.repo_path)
        else:
            local = metadata.source.local_path()
            if local is None or not local.exists():
                raise FileNotFoundError(f"local repository not found: {local}")
        metadata.state.repo_fetched = True
        metadata.state.last_error = None
        metadata.updated_at_epoch_ms = _now_ms()
        save_project_metadata(metadata)
        return metadata

    def scan_repository(self, project_id: str) -> RepoScan:
        metadata = self.load_project(project_id)
        if not metadata.state.repo_fetched:
            self.fetch_repository(project_id)
            metadata = self.load_project(project_id)
        scan = scan_local_repository(metadata.repo_path)
        metadata.default_branch = scan.default_branch
        metadata.state.repo_scanned = True
        metadata.state.last_error = None
        metadata.updated_at_epoch_ms = _now_ms()
        save_project_metadata(metadata)
        scan_path = metadata.root_path / REPO_SCAN_FILE_NAME
        scan_path.write_text(scan.model_dump_json(indent=2))
        return scan

    def _load_scan(self, metadata: ProjectMetadata) -> RepoScan:
        scan_path = metadata.root_path / REPO_SCAN_FILE_NAME
        if scan_path.exists():
            return RepoScan.model_validate_json(scan_path.read_text())
        return self.scan_repository(metadata.project_id)

    # ---- wiki lifecycle ------------------------------------------------

    def plan_wiki(self, project_id: str) -> WikiStructure:
        metadata = self.load_project(project_id)
        scan = self._load_scan(metadata)
        comprehensive = metadata.generation_config.wiki_mode == "comprehensive"
        structure = plan_wiki(scan, comprehensive)

        wiki_dir = metadata.root_path / "wiki"
        wiki_dir.mkdir(parents=True, exist_ok=True)
        (wiki_dir / WIKI_STRUCTURE_FILE_NAME).write_text(
            structure.model_dump_json(indent=2)
        )

        metadata.state.wiki_planned = True
        metadata.state.last_error = None
        metadata.updated_at_epoch_ms = _now_ms()
        save_project_metadata(metadata)
        return structure

    def save_wiki_plan(self, project_id: str, structure: WikiStructure) -> WikiStructure:
        # Validate every page id up front — save_wiki_page writes to
        # wiki/pages/<id>.md, so an unsafe id planted here would escape the
        # pages directory or fail to write when used later.
        for page in structure.pages:
            _validate_page_id(page.id)
        metadata = self.load_project(project_id)
        wiki_dir = metadata.root_path / "wiki"
        wiki_dir.mkdir(parents=True, exist_ok=True)
        (wiki_dir / WIKI_STRUCTURE_FILE_NAME).write_text(
            structure.model_dump_json(indent=2)
        )
        metadata.state.wiki_planned = True
        metadata.state.last_error = None
        metadata.updated_at_epoch_ms = _now_ms()
        save_project_metadata(metadata)
        return structure

    def save_wiki_page(self, project_id: str, page_id: str, content: str) -> Path:
        # Defense in depth: reject path-like or traversal page ids even if the
        # stored wiki structure somehow carries one.
        _validate_page_id(page_id)
        metadata = self.load_project(project_id)
        structure_path = metadata.root_path / "wiki" / WIKI_STRUCTURE_FILE_NAME
        if not structure_path.exists():
            raise FileNotFoundError(
                "wiki structure not found — call save_wiki_plan or plan_wiki first"
            )
        structure = WikiStructure.model_validate_json(structure_path.read_text())
        found = False
        for page in structure.pages:
            if page.id == page_id:
                page.content = content
                found = True
                break
        if not found:
            raise ValueError(f"page not found in wiki structure: {page_id}")

        pages_dir = metadata.root_path / "wiki" / "pages"
        pages_dir.mkdir(parents=True, exist_ok=True)
        page_path = pages_dir / f"{page_id}.md"
        page_path.write_text(content)

        structure_path.write_text(structure.model_dump_json(indent=2))
        metadata.state.wiki_generated = any(p.content for p in structure.pages)
        metadata.state.last_error = None
        metadata.updated_at_epoch_ms = _now_ms()
        save_project_metadata(metadata)
        return page_path

    def get_wiki(self, project_id: str) -> WikiStructure:
        metadata = self.load_project(project_id)
        structure_path = metadata.root_path / "wiki" / WIKI_STRUCTURE_FILE_NAME
        if not structure_path.exists():
            raise FileNotFoundError("wiki structure not found")
        return WikiStructure.model_validate_json(structure_path.read_text())

    def export_wiki(self, project_id: str) -> Path:
        metadata = self.load_project(project_id)
        structure = self.get_wiki(project_id)
        index_path = metadata.root_path / "wiki" / "index.md"
        index_path.parent.mkdir(parents=True, exist_ok=True)
        index_path.write_text(render_wiki_index(structure))
        return index_path

    # ---- artifacts -----------------------------------------------------

    def save_slides(self, project_id: str, content: str) -> Path:
        metadata = self.load_project(project_id)
        artifacts_dir = metadata.root_path / "artifacts"
        artifacts_dir.mkdir(parents=True, exist_ok=True)
        path = artifacts_dir / "slides.md"
        path.write_text(content)
        metadata.state.slides_generated = True
        metadata.state.last_error = None
        metadata.updated_at_epoch_ms = _now_ms()
        save_project_metadata(metadata)
        return path

    def save_workshop(self, project_id: str, content: str) -> Path:
        metadata = self.load_project(project_id)
        artifacts_dir = metadata.root_path / "artifacts"
        artifacts_dir.mkdir(parents=True, exist_ok=True)
        path = artifacts_dir / "workshop.md"
        path.write_text(content)
        metadata.state.workshop_generated = True
        metadata.state.last_error = None
        metadata.updated_at_epoch_ms = _now_ms()
        save_project_metadata(metadata)
        return path
