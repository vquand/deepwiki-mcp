use std::collections::BTreeMap;
use std::io::{self, BufRead, Read, Write};

use serde_json::{json, Value};

use crate::domain::{GenerationConfig, RepoSource, RepoType, StorageMode};
use crate::project::{CreateProjectRequest, ProjectService};

pub fn serve_stdio(service: &ProjectService) -> io::Result<()> {
    let stdin = io::stdin();
    let mut reader = io::BufReader::new(stdin.lock());
    let stdout = io::stdout();
    let mut writer = io::BufWriter::new(stdout.lock());

    loop {
        let Some(message) = read_message(&mut reader)? else {
            break;
        };

        let response = handle_message(service, message);
        if let Some(response) = response {
            write_message(&mut writer, &response)?;
        }
    }

    Ok(())
}

fn handle_message(service: &ProjectService, message: Value) -> Option<Value> {
    let id = message.get("id").cloned();
    let method = message.get("method")?.as_str()?;
    let params = message.get("params").cloned().unwrap_or_else(|| json!({}));

    let result = match method {
        "initialize" => Ok(json!({
            "protocolVersion": "2024-11-05",
            "serverInfo": {
                "name": "deepwiki-mcp-server",
                "version": "0.1.0"
            },
            "capabilities": {
                "tools": {}
            }
        })),
        "notifications/initialized" => return None,
        "tools/list" => Ok(json!({
            "tools": tools_list()
        })),
        "tools/call" => call_tool(service, &params),
        _ => Err(format!("unsupported method: {method}")),
    };

    Some(match result {
        Ok(result) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result
        }),
        Err(error) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": -32000,
                "message": error
            }
        }),
    })
}

fn tools_list() -> Vec<Value> {
    vec![
        tool_create_project(),
        tool_simple("deepwiki.list_projects", "List known global DeepWiki projects."),
        tool_with_project_id("deepwiki.get_project", "Get a DeepWiki project by ID."),
        tool_configure_project(),
        tool_with_project_id("deepwiki.fetch_repository", "Fetch or refresh a project repository."),
        tool_with_project_id("deepwiki.scan_repository", "Scan a project repository."),
        tool_with_project_id("deepwiki.build_index", "Build an embeddings-based index for a project."),
        tool_ask(),
        tool_with_project_id("deepwiki.plan_wiki", "Plan the wiki structure."),
        tool_with_project_id("deepwiki.generate_wiki", "Generate wiki markdown artifacts."),
        tool_export(),
        tool_with_project_id("deepwiki.generate_slides", "Generate slides markdown artifact."),
        tool_with_project_id("deepwiki.generate_workshop", "Generate workshop markdown artifact."),
    ]
}

fn tool_simple(name: &str, description: &str) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": {
            "type": "object",
            "properties": {},
            "additionalProperties": false
        }
    })
}

fn tool_with_project_id(name: &str, description: &str) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": {
            "type": "object",
            "properties": {
                "project_id": {
                    "type": "string",
                    "description": "Stable DeepWiki project identifier."
                }
            },
            "required": ["project_id"],
            "additionalProperties": false
        }
    })
}

fn tool_create_project() -> Value {
    json!({
        "name": "deepwiki.create_project",
        "description": "Create a DeepWiki project.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "storage_mode": {
                    "type": "string",
                    "enum": ["global", "repo_local"],
                    "description": "Where project artifacts are stored."
                },
                "repo_type": {
                    "type": "string",
                    "enum": ["github", "gitlab", "bitbucket", "local", "web"],
                    "description": "Repository source type."
                },
                "source": {
                    "type": "string",
                    "description": "Repository URL or local path."
                }
            },
            "required": ["storage_mode", "repo_type", "source"],
            "additionalProperties": false
        }
    })
}

fn tool_configure_project() -> Value {
    json!({
        "name": "deepwiki.configure_project",
        "description": "Set provider, model, and embedding configuration on a project.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "project_id": {
                    "type": "string",
                    "description": "Stable DeepWiki project identifier."
                },
                "provider": {
                    "type": "string",
                    "enum": ["openai", "openai_compatible"],
                    "description": "Optional generation provider."
                },
                "model": {
                    "type": "string",
                    "description": "Optional generation model."
                },
                "embedding_provider": {
                    "type": "string",
                    "enum": ["openai", "openai_compatible", "local_hash"],
                    "description": "Optional embedding provider."
                },
                "embedding_model": {
                    "type": "string",
                    "description": "Optional embedding model."
                }
            },
            "required": ["project_id"],
            "additionalProperties": false
        }
    })
}

fn tool_ask() -> Value {
    json!({
        "name": "deepwiki.ask",
        "description": "Search the embeddings index and synthesize an answer.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "project_id": {
                    "type": "string",
                    "description": "Stable DeepWiki project identifier."
                },
                "query": {
                    "type": "string",
                    "description": "Natural-language question or search query."
                }
            },
            "required": ["project_id", "query"],
            "additionalProperties": false
        }
    })
}

fn tool_export() -> Value {
    json!({
        "name": "deepwiki.export_wiki",
        "description": "Export the generated wiki markdown index file.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "project_id": {
                    "type": "string",
                    "description": "Stable DeepWiki project identifier."
                }
            },
            "required": ["project_id"],
            "additionalProperties": false
        }
    })
}

fn call_tool(service: &ProjectService, params: &Value) -> Result<Value, String> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing tool name".to_string())?;
    let arguments = params.get("arguments").cloned().unwrap_or_else(|| json!({}));

    match name {
        "deepwiki.create_project" => {
            let storage_mode = parse_storage_mode(&arguments)?;
            let repo_type = parse_repo_type(&arguments)?;
            let source = parse_source(&arguments, &storage_mode, &repo_type)?;
            let metadata = service
                .create_project(CreateProjectRequest {
                    source,
                    repo_type,
                    storage_mode,
                    generation_config: GenerationConfig::default(),
                })
                .map_err(|error| error.to_string())?;
            Ok(tool_text(serde_json::to_string_pretty(&metadata).map_err(|error| error.to_string())?))
        }
        "deepwiki.list_projects" => {
            let projects = service.list_global_projects().map_err(|error| error.to_string())?;
            Ok(tool_text(serde_json::to_string_pretty(&projects).map_err(|error| error.to_string())?))
        }
        "deepwiki.get_project" => {
            let project_id = required_string(&arguments, "project_id")?;
            let project = service.load_global_project(&project_id).map_err(|error| error.to_string())?;
            Ok(tool_text(serde_json::to_string_pretty(&project).map_err(|error| error.to_string())?))
        }
        "deepwiki.configure_project" => {
            let project_id = required_string(&arguments, "project_id")?;
            let provider = optional_string(&arguments, "provider");
            let model = optional_string(&arguments, "model");
            let embedding_provider = optional_string(&arguments, "embedding_provider");
            let embedding_model = optional_string(&arguments, "embedding_model");
            let project = service
                .configure_project(&project_id, provider, model, embedding_provider, embedding_model)
                .map_err(|error| error.to_string())?;
            Ok(tool_text(serde_json::to_string_pretty(&project).map_err(|error| error.to_string())?))
        }
        "deepwiki.fetch_repository" => {
            let project_id = required_string(&arguments, "project_id")?;
            service.fetch_project(&project_id).map_err(|error| error.to_string())?;
            Ok(tool_text(format!("Fetched repository for project {project_id}")))
        }
        "deepwiki.scan_repository" => {
            let project_id = required_string(&arguments, "project_id")?;
            let scan = service.scan_project(&project_id).map_err(|error| error.to_string())?;
            Ok(tool_text(serde_json::to_string_pretty(&scan).map_err(|error| error.to_string())?))
        }
        "deepwiki.build_index" => {
            let project_id = required_string(&arguments, "project_id")?;
            let chunks = service.build_project_index(&project_id).map_err(|error| error.to_string())?;
            Ok(tool_text(format!("Built index with {chunks} chunks for project {project_id}")))
        }
        "deepwiki.ask" => {
            let project_id = required_string(&arguments, "project_id")?;
            let query = required_string(&arguments, "query")?;
            let response = service.ask_project(&project_id, &query).map_err(|error| error.to_string())?;
            Ok(tool_text(serde_json::to_string_pretty(&response).map_err(|error| error.to_string())?))
        }
        "deepwiki.plan_wiki" => {
            let project_id = required_string(&arguments, "project_id")?;
            let structure = service.plan_project_wiki(&project_id).map_err(|error| error.to_string())?;
            Ok(tool_text(serde_json::to_string_pretty(&structure).map_err(|error| error.to_string())?))
        }
        "deepwiki.generate_wiki" => {
            let project_id = required_string(&arguments, "project_id")?;
            let structure = service.generate_project_wiki(&project_id).map_err(|error| error.to_string())?;
            Ok(tool_text(serde_json::to_string_pretty(&structure).map_err(|error| error.to_string())?))
        }
        "deepwiki.export_wiki" => {
            let project_id = required_string(&arguments, "project_id")?;
            let output = service
                .export_project_wiki_markdown(&project_id)
                .map_err(|error| error.to_string())?;
            Ok(tool_text(output.display().to_string()))
        }
        "deepwiki.generate_slides" => {
            let project_id = required_string(&arguments, "project_id")?;
            let output = service.generate_slides(&project_id).map_err(|error| error.to_string())?;
            Ok(tool_text(output.display().to_string()))
        }
        "deepwiki.generate_workshop" => {
            let project_id = required_string(&arguments, "project_id")?;
            let output = service
                .generate_workshop(&project_id)
                .map_err(|error| error.to_string())?;
            Ok(tool_text(output.display().to_string()))
        }
        _ => Err(format!("unsupported tool: {name}")),
    }
}

fn tool_text(text: String) -> Value {
    json!({
        "content": [
            {
                "type": "text",
                "text": text
            }
        ]
    })
}

fn parse_storage_mode(arguments: &Value) -> Result<StorageMode, String> {
    StorageMode::parse(&required_string(arguments, "storage_mode")?)
        .ok_or_else(|| "invalid storage_mode".to_string())
}

fn parse_repo_type(arguments: &Value) -> Result<RepoType, String> {
    RepoType::parse(&required_string(arguments, "repo_type")?).ok_or_else(|| "invalid repo_type".to_string())
}

fn parse_source(arguments: &Value, storage_mode: &StorageMode, repo_type: &RepoType) -> Result<RepoSource, String> {
    if matches!(repo_type, RepoType::Local) || matches!(storage_mode, StorageMode::RepoLocal) {
        Ok(RepoSource::Local {
            path: required_string(arguments, "source")?.into(),
        })
    } else {
        Ok(RepoSource::Remote {
            url: required_string(arguments, "source")?,
        })
    }
}

fn required_string(arguments: &Value, key: &str) -> Result<String, String> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| format!("missing string argument: {key}"))
}

fn optional_string(arguments: &Value, key: &str) -> Option<String> {
    arguments.get(key).and_then(Value::as_str).map(ToOwned::to_owned)
}

fn read_message<R: BufRead + Read>(reader: &mut R) -> io::Result<Option<Value>> {
    let mut headers = BTreeMap::new();
    loop {
        let mut line = String::new();
        let bytes = reader.read_line(&mut line)?;
        if bytes == 0 {
            return Ok(None);
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some((key, value)) = line.split_once(':') {
            headers.insert(key.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }

    let content_length: usize = headers
        .get("content-length")
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing content-length header"))?
        .parse()
        .map_err(io::Error::other)?;
    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body)?;
    let message = serde_json::from_slice(&body).map_err(io::Error::other)?;
    Ok(Some(message))
}

fn write_message<W: Write>(writer: &mut W, message: &Value) -> io::Result<()> {
    let body = serde_json::to_vec(message).map_err(io::Error::other)?;
    write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
    writer.write_all(&body)?;
    writer.flush()
}
