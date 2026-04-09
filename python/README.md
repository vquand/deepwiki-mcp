# deepwiki-mcp (Python)

MCP server for creating, scanning, and authoring DeepWiki projects. The host
LLM (e.g. Claude via Claude Code) does all planning and content generation by
calling `save_wiki_plan` and `save_wiki_page`. The server only owns storage,
git fetch, filesystem scans, and export.

## Install

```sh
uv sync
```

## Run

```sh
uv run deepwiki-mcp
```

## Register with Claude Code

Add to `~/.mcp.json`:

```json
{
  "mcpServers": {
    "deepwiki": {
      "command": "uv",
      "args": [
        "run",
        "--directory",
        "/absolute/path/to/deepwiki-mcp/python",
        "deepwiki-mcp"
      ]
    }
  }
}
```

## Storage layout

- `~/.deepwiki-mcp/<project_id>/project.json` — global projects
- `<repo>/.deepwiki-mcp/project.json` — repo-local projects (discoverable via
  a `pointer.json` at `~/.deepwiki-mcp/<project_id>/pointer.json`)

Each project directory contains `repo/` (for cloned remotes), `wiki/`,
`artifacts/`, and `logs/`.

## Tools

| Tool | Purpose |
| --- | --- |
| `create_project` | Create a project (global or repo-local) |
| `list_projects` | List all known projects |
| `get_project` | Load project metadata |
| `configure_project` | Update generation config |
| `delete_project` | Remove a project and its artifacts |
| `fetch_repository` | Clone or pull the repo |
| `scan_repository` | Walk the file tree + read README |
| `plan_wiki` | Heuristic scaffold — client should override |
| `save_wiki_plan` | Persist a client-authored `WikiStructure` |
| `save_wiki_page` | Persist markdown for one page |
| `get_wiki` | Read the persisted wiki structure |
| `export_wiki` | Write `wiki/index.md` |
| `save_slides` | Persist slides markdown authored by the client |
| `save_workshop` | Persist workshop markdown authored by the client |
