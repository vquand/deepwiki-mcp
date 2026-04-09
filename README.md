# deepwiki-mcp

MCP server for creating, scanning, and authoring DeepWiki projects. The host
LLM (e.g. Claude via Claude Code) does all planning and content generation by
calling `save_wiki_plan` and `save_wiki_page`. The server owns storage, git
fetch, filesystem scans, and export — nothing else.

## Layout

- `python/` — the MCP server (FastMCP + Pydantic, installed with `uv`)
- `references/deepwiki-open/` — archived reference implementation of the
  previous Next.js + Python web app
- `MCP_SERVER_PLAN.md` — historical design notes (pre-dates the current
  client-authored architecture)

## Quickstart

```sh
cd python
uv sync
uv run deepwiki-mcp          # starts the stdio MCP server
```

See [python/README.md](python/README.md) for the tool surface and the
`~/.mcp.json` registration snippet.
