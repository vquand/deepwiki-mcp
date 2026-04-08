mod domain;
mod artifacts;
mod export;
mod index;
mod mcp;
mod project;
mod provider;
mod repo;
mod remote;
mod storage;
mod wiki;

use std::env;
use std::io;
use std::path::PathBuf;

use crate::domain::{GenerationConfig, RepoSource, RepoType, StorageMode};
use crate::project::{CreateProjectRequest, ProjectService};

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run() -> io::Result<()> {
    let args: Vec<String> = env::args().collect();
    let service = ProjectService::new()?;

    match args.get(1).map(String::as_str) {
        Some("init") => handle_init(&service, &args),
        Some("list") => handle_list(&service),
        Some("show") => handle_show(&service, &args),
        Some("config") => handle_config(&service, &args),
        Some("fetch") => handle_fetch(&service, &args),
        Some("scan") => handle_scan(&service, &args),
        Some("build-index") => handle_build_index(&service, &args),
        Some("ask") => handle_ask(&service, &args),
        Some("plan") => handle_plan(&service, &args),
        Some("generate") => handle_generate(&service, &args),
        Some("export") => handle_export(&service, &args),
        Some("slides") => handle_slides(&service, &args),
        Some("workshop") => handle_workshop(&service, &args),
        Some("serve") => mcp::serve_stdio(&service),
        Some(command) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unknown command: {command}"),
        )),
        None => {
            print_usage(&service);
            Ok(())
        }
    }
}

fn handle_init(service: &ProjectService, args: &[String]) -> io::Result<()> {
    if args.len() < 5 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: cargo run -- init <storage-mode> <repo-type> <source>",
        ));
    }

    let storage_mode = StorageMode::parse(&args[2]).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "storage mode must be one of: global, repo_local",
        )
    })?;
    let repo_type = RepoType::parse(&args[3]).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "repo type must be one of: github, gitlab, bitbucket, local, web",
        )
    })?;
    let source = if repo_type == RepoType::Local || storage_mode == StorageMode::RepoLocal {
        RepoSource::Local {
            path: PathBuf::from(&args[4]),
        }
    } else {
        RepoSource::Remote {
            url: args[4].clone(),
        }
    };

    let metadata = service.create_project(CreateProjectRequest {
        source,
        repo_type,
        storage_mode,
        generation_config: GenerationConfig::default(),
    })?;

    println!("created project: {}", metadata.project_id);
    println!("root: {}", metadata.root_path.display());
    println!("repo: {}", metadata.repo_path.display());
    Ok(())
}

fn handle_list(service: &ProjectService) -> io::Result<()> {
    let projects = service.list_global_projects()?;
    if projects.is_empty() {
        println!("no global projects found under {}", service.global_root().display());
        return Ok(());
    }

    for project in projects {
        println!(
            "{}  {}  {}  {}",
            project.project_id,
            project.storage_mode,
            project.repo_type,
            project.source.display()
        );
    }
    Ok(())
}

fn handle_show(service: &ProjectService, args: &[String]) -> io::Result<()> {
    let project_id = args.get(2).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: cargo run -- show <project-id>",
        )
    })?;
    let project = service.load_global_project(project_id)?;
    let content = serde_json::to_string_pretty(&project).map_err(io::Error::other)?;
    println!("{content}");
    Ok(())
}

fn handle_config(service: &ProjectService, args: &[String]) -> io::Result<()> {
    let project_id = args.get(2).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: cargo run -- config <project-id> [provider|none] [model|none] [embedding-provider|none] [embedding-model|none]",
        )
    })?;
    let provider = args
        .get(3)
        .and_then(|value| if value == "none" { None } else { Some(value.clone()) });
    let model = args
        .get(4)
        .and_then(|value| if value == "none" { None } else { Some(value.clone()) });
    let embedding_provider = args
        .get(5)
        .and_then(|value| if value == "none" { None } else { Some(value.clone()) });
    let embedding_model = args
        .get(6)
        .and_then(|value| if value == "none" { None } else { Some(value.clone()) });
    let project = service.configure_project(
        project_id,
        provider,
        model,
        embedding_provider,
        embedding_model,
    )?;
    let content = serde_json::to_string_pretty(&project).map_err(io::Error::other)?;
    println!("{content}");
    Ok(())
}

fn handle_scan(service: &ProjectService, args: &[String]) -> io::Result<()> {
    let project_id = args.get(2).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: cargo run -- scan <project-id>",
        )
    })?;
    let scan = service.scan_project(project_id)?;
    println!("scanned: {}", scan.root_path.display());
    println!("files: {}", scan.file_tree.len());
    if let Some(branch) = scan.default_branch {
        println!("default branch: {branch}");
    }
    if let Some(readme_path) = scan.readme_path {
        println!("readme: {}", readme_path.display());
    }
    Ok(())
}

fn handle_fetch(service: &ProjectService, args: &[String]) -> io::Result<()> {
    let project_id = args.get(2).ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "usage: cargo run -- fetch <project-id>")
    })?;
    service.fetch_project(project_id)?;
    println!("fetched repository for {project_id}");
    Ok(())
}

fn handle_build_index(service: &ProjectService, args: &[String]) -> io::Result<()> {
    let project_id = args.get(2).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: cargo run -- build-index <project-id>",
        )
    })?;
    let chunks = service.build_project_index(project_id)?;
    println!("index chunks: {chunks}");
    Ok(())
}

fn handle_ask(service: &ProjectService, args: &[String]) -> io::Result<()> {
    let project_id = args.get(2).ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "usage: cargo run -- ask <project-id> <query>")
    })?;
    let query = args.get(3).ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "missing query")
    })?;
    let response = service.ask_project(project_id, query)?;
    let content = serde_json::to_string_pretty(&response).map_err(io::Error::other)?;
    println!("{content}");
    Ok(())
}

fn handle_plan(service: &ProjectService, args: &[String]) -> io::Result<()> {
    let project_id = args.get(2).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: cargo run -- plan <project-id>",
        )
    })?;
    let structure = service.plan_project_wiki(project_id)?;
    println!("planned wiki: {}", structure.title);
    println!("sections: {}", structure.sections.len());
    println!("pages: {}", structure.pages.len());
    Ok(())
}

fn handle_generate(service: &ProjectService, args: &[String]) -> io::Result<()> {
    let project_id = args.get(2).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: cargo run -- generate <project-id>",
        )
    })?;
    let structure = service.generate_project_wiki(project_id)?;
    println!("generated wiki: {}", structure.title);
    println!("pages written: {}", structure.pages.len());
    Ok(())
}

fn handle_export(service: &ProjectService, args: &[String]) -> io::Result<()> {
    let project_id = args.get(2).ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "usage: cargo run -- export <project-id>")
    })?;
    let path = service.export_project_wiki_markdown(project_id)?;
    println!("{}", path.display());
    Ok(())
}

fn handle_slides(service: &ProjectService, args: &[String]) -> io::Result<()> {
    let project_id = args.get(2).ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "usage: cargo run -- slides <project-id>")
    })?;
    let path = service.generate_slides(project_id)?;
    println!("{}", path.display());
    Ok(())
}

fn handle_workshop(service: &ProjectService, args: &[String]) -> io::Result<()> {
    let project_id = args.get(2).ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "usage: cargo run -- workshop <project-id>")
    })?;
    let path = service.generate_workshop(project_id)?;
    println!("{}", path.display());
    Ok(())
}

fn print_usage(service: &ProjectService) {
    println!("deepwiki-mcp foundation");
    println!("global root: {}", service.global_root().display());
    println!();
    println!("commands:");
    println!("  cargo run -- init <storage-mode> <repo-type> <source>");
    println!("  cargo run -- list");
    println!("  cargo run -- show <project-id>");
    println!("  cargo run -- config <project-id> [provider|none] [model|none] [embedding-provider|none] [embedding-model|none]");
    println!("  cargo run -- fetch <project-id>");
    println!("  cargo run -- scan <project-id>");
    println!("  cargo run -- build-index <project-id>");
    println!("  cargo run -- ask <project-id> <query>");
    println!("  cargo run -- plan <project-id>");
    println!("  cargo run -- generate <project-id>");
    println!("  cargo run -- export <project-id>");
    println!("  cargo run -- slides <project-id>");
    println!("  cargo run -- workshop <project-id>");
    println!("  cargo run -- serve");
}
