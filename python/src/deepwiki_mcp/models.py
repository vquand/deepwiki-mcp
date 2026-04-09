"""Pydantic models for DeepWiki projects, wikis, and repository scans.

Field layout is intentionally compatible with the previous Rust serialization
so existing `~/.deepwiki-mcp/<id>/project.json` files load without migration.
Unknown fields (e.g. the legacy `provider`, `model`, `index_built`) are ignored.
"""
from __future__ import annotations

from enum import Enum
from pathlib import Path
from typing import Literal, Optional

from pydantic import BaseModel, ConfigDict, Field


class StorageMode(str, Enum):
    GLOBAL = "global"
    REPO_LOCAL = "repo_local"


class RepoType(str, Enum):
    GITHUB = "github"
    GITLAB = "gitlab"
    BITBUCKET = "bitbucket"
    LOCAL = "local"
    WEB = "web"


class RepoSource(BaseModel):
    """Tagged union matching Rust's `#[serde(tag = "kind", rename_all = "snake_case")]`."""

    model_config = ConfigDict(extra="ignore")

    kind: Literal["remote", "local"]
    url: Optional[str] = None
    path: Optional[Path] = None

    @classmethod
    def remote(cls, url: str) -> "RepoSource":
        return cls(kind="remote", url=url)

    @classmethod
    def local(cls, path: Path) -> "RepoSource":
        return cls(kind="local", path=path)

    def normalized_key(self) -> str:
        if self.kind == "remote":
            return (self.url or "").strip().rstrip("/").lower()
        return str(self.path or "")

    def local_path(self) -> Optional[Path]:
        return self.path if self.kind == "local" else None


class GenerationConfig(BaseModel):
    model_config = ConfigDict(extra="ignore")

    language: str = "en"
    excluded_dirs: list[str] = Field(default_factory=list)
    excluded_files: list[str] = Field(default_factory=list)
    included_dirs: list[str] = Field(default_factory=list)
    included_files: list[str] = Field(default_factory=list)
    wiki_mode: Literal["comprehensive", "concise"] = "comprehensive"


class ProjectState(BaseModel):
    model_config = ConfigDict(extra="ignore")

    repo_fetched: bool = False
    repo_scanned: bool = False
    wiki_planned: bool = False
    wiki_generated: bool = False
    slides_generated: bool = False
    workshop_generated: bool = False
    last_error: Optional[str] = None


class ProjectMetadata(BaseModel):
    model_config = ConfigDict(extra="ignore")

    project_id: str
    source: RepoSource
    repo_type: RepoType
    storage_mode: StorageMode
    root_path: Path
    repo_path: Path
    default_branch: Optional[str] = None
    created_at_epoch_ms: int
    updated_at_epoch_ms: int
    generation_config: GenerationConfig = Field(default_factory=GenerationConfig)
    state: ProjectState = Field(default_factory=ProjectState)


class RepoScan(BaseModel):
    model_config = ConfigDict(extra="ignore")

    scanned_at_epoch_ms: int
    root_path: Path
    default_branch: Optional[str] = None
    readme_path: Optional[Path] = None
    readme_content: Optional[str] = None
    file_tree: list[str]


class WikiPage(BaseModel):
    model_config = ConfigDict(extra="ignore")

    id: str
    title: str
    description: str
    importance: Literal["high", "medium", "low"] = "medium"
    file_paths: list[str] = Field(default_factory=list)
    related_pages: list[str] = Field(default_factory=list)
    parent_section: Optional[str] = None
    content: Optional[str] = None


class WikiSection(BaseModel):
    model_config = ConfigDict(extra="ignore")

    id: str
    title: str
    pages: list[str] = Field(default_factory=list)
    subsections: list[str] = Field(default_factory=list)


class WikiStructure(BaseModel):
    model_config = ConfigDict(extra="ignore")

    id: str = "wiki"
    title: str
    description: str
    sections: list[WikiSection] = Field(default_factory=list)
    root_sections: list[str] = Field(default_factory=list)
    pages: list[WikiPage] = Field(default_factory=list)
