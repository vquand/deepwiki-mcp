"""FastMCP tool surface for deepwiki.

All generation is driven by the host LLM (the MCP client). This server owns
storage, git fetch, and filesystem scanning only. Tools return JSON-friendly
dicts so the client can reason about state without parsing our Pydantic types.
"""
from __future__ import annotations

from pathlib import Path
from typing import Any

from mcp.server.fastmcp import FastMCP

from .models import (
    GenerationConfig,
    RepoSource,
    RepoType,
    StorageMode,
    WikiStructure,
)
from .projects import ProjectService

mcp = FastMCP("deepwiki")
_service = ProjectService()


def _metadata_to_dict(metadata: Any) -> dict[str, Any]:
    return metadata.model_dump(mode="json")


def _resolve_repo_type(value: str | None, source: RepoSource) -> RepoType:
    if value:
        try:
            return RepoType(value.lower())
        except ValueError as err:
            raise ValueError(
                f"repo_type must be one of: github, gitlab, bitbucket, local, web (got {value!r})"
            ) from err
    if source.kind == "local":
        return RepoType.LOCAL
    url = (source.url or "").lower()
    if "github.com" in url:
        return RepoType.GITHUB
    if "gitlab.com" in url:
        return RepoType.GITLAB
    if "bitbucket.org" in url:
        return RepoType.BITBUCKET
    return RepoType.WEB


@mcp.tool()
def create_project(
    url: str | None = None,
    path: str | None = None,
    storage_mode: str = "global",
    repo_type: str | None = None,
    language: str = "en",
    wiki_mode: str = "comprehensive",
) -> dict[str, Any]:
    """Create a deepwiki project.

    Provide either `url` (for a remote repo) or `path` (for a local one).
    `storage_mode` is 'global' (stored under ~/.deepwiki-mcp) or 'repo_local'
    (stored under <repo>/.deepwiki-mcp and discoverable via a pointer file).
    """
    if bool(url) == bool(path):
        raise ValueError("provide exactly one of 'url' or 'path'")
    if url is not None:
        source = RepoSource.remote(url)
    else:
        source = RepoSource.local(Path(path).expanduser().resolve())  # type: ignore[arg-type]

    try:
        mode = StorageMode(storage_mode)
    except ValueError as err:
        raise ValueError("storage_mode must be 'global' or 'repo_local'") from err

    resolved_repo_type = _resolve_repo_type(repo_type, source)
    if wiki_mode not in {"comprehensive", "concise"}:
        raise ValueError("wiki_mode must be 'comprehensive' or 'concise'")

    gen_config = GenerationConfig(language=language, wiki_mode=wiki_mode)  # type: ignore[arg-type]
    metadata = _service.create_project(source, resolved_repo_type, mode, gen_config)
    return _metadata_to_dict(metadata)


@mcp.tool()
def list_projects() -> list[dict[str, Any]]:
    """List all known deepwiki projects (global + repo_local via pointers)."""
    return [_metadata_to_dict(meta) for meta in _service.list_projects()]


@mcp.tool()
def get_project(project_id: str) -> dict[str, Any]:
    """Load a project's metadata by id."""
    return _metadata_to_dict(_service.load_project(project_id))


@mcp.tool()
def configure_project(
    project_id: str,
    language: str | None = None,
    wiki_mode: str | None = None,
    excluded_dirs: list[str] | None = None,
    excluded_files: list[str] | None = None,
    included_dirs: list[str] | None = None,
    included_files: list[str] | None = None,
) -> dict[str, Any]:
    """Update a project's generation config (language, wiki_mode, inclusions)."""
    metadata = _service.configure_project(
        project_id,
        language=language,
        wiki_mode=wiki_mode,
        excluded_dirs=excluded_dirs,
        excluded_files=excluded_files,
        included_dirs=included_dirs,
        included_files=included_files,
    )
    return _metadata_to_dict(metadata)


@mcp.tool()
def delete_project(project_id: str) -> dict[str, Any]:
    """Delete a project's .deepwiki-mcp directory and pointer (not the repo itself)."""
    _service.delete_project(project_id)
    return {"project_id": project_id, "deleted": True}


@mcp.tool()
def fetch_repository(project_id: str) -> dict[str, Any]:
    """Clone (or pull) a remote repo, or verify the path for a local one."""
    metadata = _service.fetch_repository(project_id)
    return _metadata_to_dict(metadata)


@mcp.tool()
def scan_repository(project_id: str) -> dict[str, Any]:
    """Walk the file tree, read the README, and detect the default branch."""
    scan = _service.scan_repository(project_id)
    return scan.model_dump(mode="json")


@mcp.tool()
def plan_wiki(project_id: str) -> dict[str, Any]:
    """Generate a heuristic wiki scaffold. The MCP client should override with save_wiki_plan."""
    structure = _service.plan_wiki(project_id)
    return structure.model_dump(mode="json")


@mcp.tool()
def save_wiki_plan(project_id: str, structure: dict[str, Any]) -> dict[str, Any]:
    """Persist a client-authored WikiStructure.

    This is the intended entry point for LLM-driven wiki authoring: the
    client (e.g. Claude) produces a tailored plan and stores it here.
    """
    validated = WikiStructure.model_validate(structure)
    saved = _service.save_wiki_plan(project_id, validated)
    return saved.model_dump(mode="json")


@mcp.tool()
def save_wiki_page(project_id: str, page_id: str, content: str) -> dict[str, Any]:
    """Persist markdown content for one page in the wiki structure."""
    path = _service.save_wiki_page(project_id, page_id, content)
    return {"project_id": project_id, "page_id": page_id, "path": str(path)}


@mcp.tool()
def get_wiki(project_id: str) -> dict[str, Any]:
    """Read the persisted wiki structure for a project."""
    structure = _service.get_wiki(project_id)
    return structure.model_dump(mode="json")


@mcp.tool()
def export_wiki(project_id: str) -> dict[str, Any]:
    """Write wiki/index.md from the persisted wiki structure."""
    path = _service.export_wiki(project_id)
    return {"project_id": project_id, "path": str(path)}


@mcp.tool()
def save_slides(project_id: str, content: str) -> dict[str, Any]:
    """Persist client-authored slides markdown under artifacts/slides.md."""
    path = _service.save_slides(project_id, content)
    return {"project_id": project_id, "path": str(path)}


@mcp.tool()
def save_workshop(project_id: str, content: str) -> dict[str, Any]:
    """Persist client-authored workshop markdown under artifacts/workshop.md."""
    path = _service.save_workshop(project_id, content)
    return {"project_id": project_id, "path": str(path)}


def main() -> None:
    mcp.run()


if __name__ == "__main__":
    main()
