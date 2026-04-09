"""Filesystem layout, project ID derivation, and JSON I/O helpers.

Repo-local projects are discoverable from the global root via a `pointer.json`
file. This fixes the bug where `load_project` only looked at
`~/.deepwiki-mcp/<id>/project.json` even for `repo_local` projects.
"""
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path

from .models import ProjectMetadata, RepoSource, StorageMode

PROJECT_FILE_NAME = "project.json"
POINTER_FILE_NAME = "pointer.json"
REPO_SCAN_FILE_NAME = "repo_scan.json"
WIKI_STRUCTURE_FILE_NAME = "structure.json"
WIKI_CACHE_FILE_NAME = "cache.json"


def default_global_root() -> Path:
    home = os.environ.get("HOME")
    if not home:
        raise RuntimeError("HOME environment variable is not set")
    return Path(home) / ".deepwiki-mcp"


def derive_project_id(source: RepoSource, storage_mode: StorageMode) -> str:
    """Stable 16-char project id from the normalized source + storage mode."""
    hasher = hashlib.blake2b(digest_size=8)
    hasher.update(source.normalized_key().encode("utf-8"))
    hasher.update(b"|")
    hasher.update(storage_mode.value.encode("utf-8"))
    return hasher.hexdigest()


class ProjectPaths:
    def __init__(
        self,
        global_root: Path,
        storage_mode: StorageMode,
        project_id: str,
        source: RepoSource,
    ) -> None:
        if storage_mode == StorageMode.REPO_LOCAL:
            local = source.local_path()
            if local is None:
                raise ValueError("repo_local mode requires a local repository path")
            self.root = local / ".deepwiki-mcp"
            self.repo = local
        else:
            self.root = global_root / project_id
            if source.kind == "remote":
                self.repo = self.root / "repo"
            else:
                local = source.local_path()
                if local is None:
                    raise ValueError("local source missing path")
                self.repo = local

        self.index = self.root / "index"
        self.wiki_dir = self.root / "wiki"
        self.wiki_pages_dir = self.wiki_dir / "pages"
        self.artifacts_dir = self.root / "artifacts"
        self.logs_dir = self.root / "logs"

    def ensure_layout(self) -> None:
        for directory in (
            self.root,
            self.index,
            self.wiki_dir,
            self.wiki_pages_dir,
            self.artifacts_dir,
            self.logs_dir,
        ):
            directory.mkdir(parents=True, exist_ok=True)


def save_project_metadata(metadata: ProjectMetadata) -> None:
    content = metadata.model_dump_json(indent=2)
    (metadata.root_path / PROJECT_FILE_NAME).write_text(content)


def load_project_metadata(project_file: Path) -> ProjectMetadata:
    return ProjectMetadata.model_validate_json(project_file.read_text())


def save_project_pointer(global_root: Path, project_id: str, target: Path) -> None:
    """Write a pointer file under the global root so repo_local projects are
    discoverable by project_id without knowing the original local path."""
    pointer_dir = global_root / project_id
    pointer_dir.mkdir(parents=True, exist_ok=True)
    payload = {"target": str(target)}
    (pointer_dir / POINTER_FILE_NAME).write_text(json.dumps(payload, indent=2))


def resolve_project_file(global_root: Path, project_id: str) -> Path:
    """Find the canonical project.json for an id, following a pointer if present."""
    direct = global_root / project_id / PROJECT_FILE_NAME
    if direct.exists():
        return direct
    pointer_path = global_root / project_id / POINTER_FILE_NAME
    if pointer_path.exists():
        payload = json.loads(pointer_path.read_text())
        target = payload.get("target")
        if not target:
            raise FileNotFoundError(f"pointer missing target: {pointer_path}")
        return Path(target)
    raise FileNotFoundError(f"project not found: {project_id}")


def list_project_files(global_root: Path) -> list[Path]:
    if not global_root.exists():
        return []
    files: list[Path] = []
    for entry in sorted(global_root.iterdir()):
        direct = entry / PROJECT_FILE_NAME
        if direct.exists():
            files.append(direct)
            continue
        pointer = entry / POINTER_FILE_NAME
        if pointer.exists():
            try:
                payload = json.loads(pointer.read_text())
                target = Path(payload["target"])
                if target.exists():
                    files.append(target)
            except (json.JSONDecodeError, KeyError, OSError):
                continue
    return files
