"""Git operations and local filesystem scanning.

No LLM calls — the server only walks the tree and shells out to git so the
host model has a concrete inventory to work from.
"""
from __future__ import annotations

import os
import subprocess
import time
from pathlib import Path

from .models import RepoScan

README_CANDIDATES = (
    "README.md",
    "README.txt",
    "README.rst",
    "readme.md",
    "readme.txt",
    "readme.rst",
)

IGNORED_DIRS = frozenset(
    {
        ".git",
        ".deepwiki-mcp",
        "node_modules",
        "target",
        "__pycache__",
        ".next",
        "dist",
        "build",
        ".venv",
        "venv",
        ".mypy_cache",
        ".ruff_cache",
        ".pytest_cache",
    }
)


def git_clone(url: str, target_dir: Path) -> None:
    target_dir.parent.mkdir(parents=True, exist_ok=True)
    result = subprocess.run(
        ["git", "clone", "--depth=1", "--single-branch", url, str(target_dir)],
        check=False,
    )
    if result.returncode != 0:
        raise RuntimeError(f"git clone failed for {url}")


def git_pull(repo_dir: Path) -> None:
    result = subprocess.run(
        ["git", "-C", str(repo_dir), "pull", "--ff-only"],
        check=False,
    )
    if result.returncode != 0:
        raise RuntimeError(f"git pull failed for {repo_dir}")


def detect_default_branch(repo_dir: Path) -> str | None:
    try:
        result = subprocess.run(
            ["git", "-C", str(repo_dir), "rev-parse", "--abbrev-ref", "HEAD"],
            check=False,
            capture_output=True,
            text=True,
        )
    except FileNotFoundError:
        return None
    if result.returncode != 0:
        return None
    branch = result.stdout.strip()
    return branch or None


def _load_readme(root: Path) -> tuple[Path | None, str | None]:
    for candidate in README_CANDIDATES:
        path = root / candidate
        if path.exists():
            try:
                return path, path.read_text(encoding="utf-8", errors="replace")
            except OSError:
                return path, None
    return None, None


def _collect_file_tree(root: Path) -> list[str]:
    files: list[str] = []
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = [d for d in dirnames if d not in IGNORED_DIRS]
        current = Path(dirpath)
        for name in filenames:
            rel = (current / name).relative_to(root).as_posix()
            files.append(rel)
    files.sort()
    return files


def scan_local_repository(repo_path: Path) -> RepoScan:
    resolved = repo_path.resolve()
    if not resolved.exists():
        raise FileNotFoundError(f"repository not found: {resolved}")
    file_tree = _collect_file_tree(resolved)
    readme_path, readme_content = _load_readme(resolved)
    default_branch = detect_default_branch(resolved)
    return RepoScan(
        scanned_at_epoch_ms=int(time.time() * 1000),
        root_path=resolved,
        default_branch=default_branch,
        readme_path=readme_path,
        readme_content=readme_content,
        file_tree=file_tree,
    )
