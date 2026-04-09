"""Heuristic wiki scaffold and markdown rendering.

The heuristic plan is a fallback only — the MCP client (Claude) is expected
to call `save_wiki_plan` with a better-authored structure. We still need a
reasonable default so `plan_wiki` is callable without any LLM.
"""
from __future__ import annotations

from collections import OrderedDict

from .models import RepoScan, WikiPage, WikiSection, WikiStructure


def plan_wiki(scan: RepoScan, comprehensive: bool) -> WikiStructure:
    title = _repository_name(scan)
    title = f"{title} Wiki" if title else "Project Wiki"
    description = _build_description(scan)

    categorized = _categorize_files(scan.file_tree)

    sections: list[WikiSection] = []
    root_sections: list[str] = []
    pages: list[WikiPage] = []

    page_counter = 1
    section_counter = 1
    for category, file_paths in categorized.items():
        if not file_paths:
            continue
        section_id = f"section-{section_counter}"
        section_counter += 1
        page_id = f"page-{page_counter}"
        page_counter += 1

        title_case = _category_title(category)
        pages.append(
            WikiPage(
                id=page_id,
                title=title_case,
                description=f"Overview of the project's {title_case.lower()} implementation.",
                importance=_importance_for_category(category),
                file_paths=_trim_files(file_paths, comprehensive),
                related_pages=[],
                parent_section=section_id,
                content=None,
            )
        )
        sections.append(
            WikiSection(
                id=section_id,
                title=title_case,
                pages=[page_id],
                subsections=[],
            )
        )
        root_sections.append(section_id)

    if not pages:
        pages.append(
            WikiPage(
                id="page-1",
                title="Repository Overview",
                description="High-level documentation page for the repository.",
                importance="high",
                file_paths=_trim_files(list(scan.file_tree), comprehensive),
                related_pages=[],
                parent_section=None,
                content=None,
            )
        )

    all_ids = [page.id for page in pages]
    for page in pages:
        page.related_pages = [pid for pid in all_ids if pid != page.id][:3]

    return WikiStructure(
        id="wiki",
        title=title,
        description=description,
        sections=sections,
        root_sections=root_sections,
        pages=pages,
    )


def _categorize_files(files: list[str]) -> "OrderedDict[str, list[str]]":
    # Use sorted keys for deterministic output, matching Rust's BTreeMap.
    buckets: dict[str, list[str]] = {}
    for file in files:
        if file.startswith("tests/") or file.startswith("test/"):
            category = "testing"
        elif (
            file.startswith("src/app/")
            or file.startswith("src/components/")
            or file.endswith(".tsx")
            or file.endswith(".jsx")
        ):
            category = "frontend"
        elif file.startswith("api/") or file.endswith(".py"):
            category = "backend"
        elif (
            "config" in file
            or file.endswith(".json")
            or file.endswith(".toml")
            or file.endswith(".yaml")
            or file.endswith(".yml")
        ):
            category = "configuration"
        elif "docker" in file or "compose" in file or "deploy" in file:
            category = "deployment"
        elif file.endswith(".md"):
            category = "documentation"
        else:
            category = "architecture"
        buckets.setdefault(category, []).append(file)
    return OrderedDict(sorted(buckets.items()))


def _trim_files(files: list[str], comprehensive: bool) -> list[str]:
    limit = 12 if comprehensive else 6
    return files[:limit]


def _category_title(category: str) -> str:
    return {
        "frontend": "Frontend Components",
        "backend": "Backend Systems",
        "testing": "Testing Strategy",
        "configuration": "Configuration",
        "deployment": "Deployment and Infrastructure",
        "documentation": "Project Documentation",
    }.get(category, "System Architecture")


def _importance_for_category(category: str) -> str:
    if category in {"architecture", "backend", "frontend"}:
        return "high"
    if category in {"configuration", "deployment"}:
        return "medium"
    return "low"


def _repository_name(scan: RepoScan) -> str | None:
    name = scan.root_path.name
    return name or None


def _build_description(scan: RepoScan) -> str:
    if scan.readme_content:
        for raw in scan.readme_content.splitlines():
            line = raw.strip()
            if not line:
                continue
            if line.startswith("#") or line.startswith("---") or line.startswith("!["):
                continue
            if line.startswith("[!["):
                continue
            return line
    return "Repository documentation generated from source structure."


def render_page_markdown(page: WikiPage, scan: RepoScan) -> str:
    lines: list[str] = []
    lines.append("<details>")
    lines.append("<summary>Relevant source files</summary>")
    lines.append("")
    for file in page.file_paths:
        lines.append(f"- `{file}`")
    lines.append("</details>")
    lines.append("")
    lines.append(f"# {page.title}")
    lines.append("")
    lines.append(page.description)
    lines.append("")
    lines.append("## Summary")
    lines.append("")
    lines.append(
        f"This page was generated from the repository scan for `{scan.root_path}`. "
        "It currently serves as a structured stub and lists the source files that "
        "should drive future LLM-backed generation."
    )
    lines.append("")
    if scan.default_branch:
        lines.append("## Repository Metadata")
        lines.append("")
        lines.append(f"- Default branch: `{scan.default_branch}`")
        lines.append(f"- Scanned files: `{len(scan.file_tree)}`")
        lines.append("")
    lines.append("## Source Inventory")
    lines.append("")
    for file in page.file_paths:
        lines.append(f"- `{file}`")
    return "\n".join(lines) + "\n"


def render_wiki_index(structure: WikiStructure) -> str:
    lines: list[str] = []
    lines.append(f"# {structure.title}")
    lines.append("")
    lines.append(structure.description)
    lines.append("")
    page_by_id = {page.id: page for page in structure.pages}
    for section in structure.sections:
        lines.append(f"## {section.title}")
        lines.append("")
        for page_id in section.pages:
            page = page_by_id.get(page_id)
            if page is not None:
                lines.append(f"- [{page.title}](wiki/pages/{page.id}.md)")
        lines.append("")
    return "\n".join(lines) + "\n"
