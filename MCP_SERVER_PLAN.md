# Rust MCP Server Migration Plan

## Current project shape

This repository is not a single backend-driven wiki generator today. It is split into two layers:

- `api/` is a Python FastAPI backend that provides:
  - model/config/auth endpoints
  - chat and RAG generation over WebSocket/HTTP fallback
  - repo cloning/indexing via `DatabaseManager`
  - server-side wiki cache persistence under `~/.adalflow/wikicache`
  - processed project listing and wiki export
- `src/app/[owner]/[repo]/page.tsx` is the real wiki orchestrator:
  - fetches repo structure and README
  - prompts the model to create wiki structure XML
  - generates each wiki page one at a time
  - saves the assembled wiki back to backend cache
- `slides/page.tsx` and `workshop/page.tsx` are secondary generators built on top of the cached wiki.

## Key conclusion

To "turn this into an MCP server" properly, the MCP server must absorb the orchestration logic that currently lives in Next.js, not just wrap the existing Python endpoints.

If we only expose the current Python backend through MCP, we would still be missing:

- wiki structure generation
- per-page wiki generation workflow
- complete project lifecycle management
- slides/workshop generation from cached wiki

## Target outcome

Build a Rust MCP server that can:

- create and manage DeepWiki projects directly through MCP tools
- generate the full wiki end-to-end
- support chat / ask over indexed repo context
- export wiki outputs
- generate slides and workshop artifacts
- store project data either:
  - globally in `~/.deepwiki-mcp/{project-id}/...`
  - locally in `<repo>/.deepwiki-mcp/...`

## Proposed storage model

Use a storage mode enum:

- `global`
- `repo_local`

Suggested directory layout:

### Global mode

`~/.deepwiki-mcp/{project-id}/`

- `project.json`
- `repo/`
- `index/`
- `wiki/structure.json`
- `wiki/pages/{page-id}.md`
- `wiki/cache.json`
- `artifacts/slides.md`
- `artifacts/workshop.md`
- `logs/`

### Repo-local mode

`<repo-root>/.deepwiki-mcp/`

- `project.json`
- `index/`
- `wiki/structure.json`
- `wiki/pages/{page-id}.md`
- `wiki/cache.json`
- `artifacts/slides.md`
- `artifacts/workshop.md`
- `logs/`

Notes:

- In `repo_local` mode, do not duplicate the repo checkout.
- In `global` mode, remote repositories can be cloned into `repo/`.
- `project-id` should be stable and derived from normalized repo source plus storage mode.
- Language, provider, model, and filter settings should be versioned in metadata, not encoded into filenames.

## Recommended MCP tool surface

### Project lifecycle

- `deepwiki.create_project`
  - input: repo URL or local path, repo type, storage mode, optional auth token
  - output: project metadata and resolved paths
- `deepwiki.get_project`
- `deepwiki.list_projects`
- `deepwiki.delete_project`
- `deepwiki.refresh_project`
  - re-pull or re-scan repo, invalidate stale wiki/index artifacts

### Repository ingestion

- `deepwiki.fetch_repository`
  - clone or validate local repo
- `deepwiki.scan_repository`
  - file tree, README, branch/default branch, metadata
- `deepwiki.build_index`
  - chunk, embed, and persist retriever index
- `deepwiki.get_repository_context`
  - return summarized structure and source inventory

### Wiki generation

- `deepwiki.plan_wiki`
  - generate wiki structure from repo tree + README + filters
- `deepwiki.generate_wiki_page`
  - generate one page from planned structure and relevant files
- `deepwiki.generate_wiki`
  - end-to-end structure + all pages + persistence
- `deepwiki.get_wiki`
  - retrieve structure + pages
- `deepwiki.export_wiki`
  - markdown or json

### QA / research

- `deepwiki.ask`
  - RAG-backed answer over project index
- `deepwiki.deep_research`
  - multi-turn research workflow, if you want parity with the current tagged mode

### Derived artifacts

- `deepwiki.generate_slides`
  - based on cached wiki
- `deepwiki.generate_workshop`
  - based on cached wiki

### Maintenance / config

- `deepwiki.get_models`
- `deepwiki.validate_provider_config`
- `deepwiki.get_project_status`
  - repo present, index present, wiki present, artifact timestamps

## Rust architecture

Split implementation into focused modules/crates:

- `crates/deepwiki-mcp`
  - MCP server binary and tool registration
- `crates/deepwiki-core`
  - domain models, project lifecycle, wiki planner/generator orchestration
- `crates/deepwiki-storage`
  - storage mode abstraction, metadata persistence, artifact IO
- `crates/deepwiki-repo`
  - git/local repo handling, branch detection, README loading, file tree scanning
- `crates/deepwiki-index`
  - chunking, embeddings, vector index, retrieval
- `crates/deepwiki-llm`
  - provider abstraction for generation and embeddings
- `crates/deepwiki-export`
  - markdown/json export and artifact packaging

Core service interfaces:

- `ProjectStore`
- `RepoSource`
- `IndexStore`
- `Retriever`
- `LlmProvider`
- `WikiPlanner`
- `WikiGenerator`
- `ArtifactGenerator`

## Migration strategy

### Phase 1: Domain extraction

Recreate the current data contracts in Rust first:

- repo info
- wiki structure
- wiki page
- project metadata
- generated artifact metadata

Also define the canonical on-disk layout under `.deepwiki-mcp`.

### Phase 2: Repository and storage layer

Implement:

- remote clone / local repo attach
- default branch detection
- file tree enumeration
- README loading
- storage-mode-aware path resolver

This phase replaces the current mix of:

- frontend GitHub/GitLab/Bitbucket API traversal
- Python `download_repo`
- filename-based cache conventions

### Phase 3: Indexing and retrieval

Move `DatabaseManager` / RAG prep behavior into Rust:

- file inclusion/exclusion filters
- chunking
- embeddings
- persistent retriever index

Important change:

- do not keep the current `~/.adalflow/repos`, `~/.adalflow/databases`, `~/.adalflow/wikicache` split
- unify everything under `.deepwiki-mcp`

### Phase 4: Wiki orchestration

Move the real wiki generation logic from `src/app/[owner]/[repo]/page.tsx` into Rust:

- `plan_wiki`
- parse/validate structure output
- relevant file resolution
- sequential or bounded-concurrency page generation
- persist completed wiki

This is the most important phase. Without it, the MCP server is incomplete.

### Phase 5: Secondary artifact generation

Port:

- slides generation flow
- workshop generation flow
- export flow

These should consume the stored wiki instead of rebuilding context each time.

### Phase 6: Compatibility layer

After the Rust MCP server works end-to-end, decide between:

- keeping the Next.js UI and making it call the Rust service
- or removing the web app entirely and using MCP as the primary interface

## Implementation decisions that should change from the current code

### 1. Replace filename-derived cache identity

Current cache identity is based on filenames like:

- `deepwiki_cache_{repo_type}_{owner}_{repo}_{language}.json`

This is fragile and does not cleanly support:

- multiple storage modes
- provider/model variants
- local repos
- future artifact types

Replace it with `project.json` + directory-based identity.

### 2. Move orchestration out of the frontend

Current wiki generation is embedded in client React code. That is the wrong place if MCP is the target product surface.

Rust should own:

- planning
- generation
- retries
- persistence
- status tracking

### 3. Separate source acquisition from wiki generation

The server should distinguish:

- attach/fetch repo
- build/rebuild index
- generate wiki

This allows incremental workflows through MCP instead of one opaque operation.

### 4. Treat slides/workshop as first-class artifacts

They are currently page-level UI features. In MCP form they should be explicit tools backed by stored wiki state.

## Suggested data model

`ProjectMetadata`

- `project_id`
- `source`
- `repo_type`
- `storage_mode`
- `root_path`
- `repo_path`
- `default_branch`
- `created_at`
- `updated_at`

`GenerationConfig`

- `language`
- `provider`
- `model`
- `embedding_provider`
- `excluded_dirs`
- `excluded_files`
- `included_dirs`
- `included_files`
- `wiki_mode` (`comprehensive` or `concise`)

`ProjectState`

- `repo_fetched`
- `index_built`
- `wiki_planned`
- `wiki_generated`
- `slides_generated`
- `workshop_generated`
- `last_error`

## Risks

- The current repo relies heavily on prompt-driven XML/Markdown generation. Rust migration should preserve outputs first, then improve prompt structure later.
- Embedding and model-provider parity may be the longest part of the rewrite.
- Git host handling is currently split across frontend and backend; unifying this will surface edge cases for GitLab self-hosted and GitHub Enterprise.
- If exact parity with all current providers is required on day one, provider integration will dominate the schedule.

## Recommended delivery order

1. Build Rust MCP server with project storage, repo attach/clone, and scan tools.
2. Implement unified on-disk `.deepwiki-mcp` layout.
3. Implement index build + ask tool.
4. Port wiki planning.
5. Port page generation and full `generate_wiki`.
6. Port export.
7. Port slides/workshop.
8. Retire or thin the Python backend.

## Minimal viable MCP release

If you want the fastest useful Rust MCP version, MVP should include:

- `create_project`
- `build_index`
- `plan_wiki`
- `generate_wiki`
- `get_wiki`
- `ask`
- `list_projects`
- `delete_project`

Slides/workshop can come in the next iteration.

## Practical recommendation

Do not start by translating FastAPI endpoints one-for-one.

Start by extracting the real product workflow:

- repo intake
- scan
- index
- wiki plan
- page generation
- cache/artifact persistence
- ask/export/slides/workshop

Then expose those as MCP tools in Rust.
