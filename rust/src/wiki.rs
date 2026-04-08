use std::collections::BTreeMap;

use crate::domain::{RepoScan, WikiPage, WikiSection, WikiStructure};

pub fn plan_wiki(scan: &RepoScan, comprehensive: bool) -> WikiStructure {
    let title = repository_name(scan)
        .map(|name| format!("{name} Wiki"))
        .unwrap_or_else(|| "Project Wiki".to_string());
    let description = build_description(scan);

    let mut sections = Vec::new();
    let mut root_sections = Vec::new();
    let mut pages = Vec::new();

    let categorized = categorize_files(&scan.file_tree);
    let mut page_counter = 1usize;
    let mut section_counter = 1usize;

    for (category, file_paths) in categorized {
        if file_paths.is_empty() {
            continue;
        }

        let section_id = format!("section-{section_counter}");
        section_counter += 1;
        let page_id = format!("page-{page_counter}");
        page_counter += 1;

        let title_case = category_title(&category);
        pages.push(WikiPage {
            id: page_id.clone(),
            title: title_case.clone(),
            description: format!("Overview of the project's {} implementation.", title_case.to_lowercase()),
            importance: importance_for_category(&category).to_string(),
            file_paths: trim_files(file_paths, comprehensive),
            related_pages: Vec::new(),
            parent_section: Some(section_id.clone()),
            content: None,
        });

        sections.push(WikiSection {
            id: section_id.clone(),
            title: title_case,
            pages: vec![page_id],
            subsections: Vec::new(),
        });
        root_sections.push(section_id);
    }

    if pages.is_empty() {
        pages.push(WikiPage {
            id: "page-1".to_string(),
            title: "Repository Overview".to_string(),
            description: "High-level documentation page for the repository.".to_string(),
            importance: "high".to_string(),
            file_paths: trim_files(scan.file_tree.clone(), comprehensive),
            related_pages: Vec::new(),
            parent_section: None,
            content: None,
        });
    }

    let related_page_ids: Vec<String> = pages.iter().map(|page| page.id.clone()).collect();
    for page in &mut pages {
        page.related_pages = related_page_ids
            .iter()
            .filter(|id| **id != page.id)
            .take(3)
            .cloned()
            .collect();
    }

    WikiStructure {
        id: "wiki".to_string(),
        title,
        description,
        sections,
        root_sections,
        pages,
    }
}

fn categorize_files(files: &[String]) -> BTreeMap<String, Vec<String>> {
    let mut categories: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for file in files {
        let category = if file.starts_with("tests/") || file.starts_with("test/") {
            "testing"
        } else if file.starts_with("src/app/") || file.starts_with("src/components/") || file.ends_with(".tsx") || file.ends_with(".jsx") {
            "frontend"
        } else if file.starts_with("api/") || file.ends_with(".py") {
            "backend"
        } else if file.contains("config") || file.ends_with(".json") || file.ends_with(".toml") || file.ends_with(".yaml") || file.ends_with(".yml") {
            "configuration"
        } else if file.contains("docker") || file.contains("compose") || file.contains("deploy") {
            "deployment"
        } else if file.ends_with(".md") {
            "documentation"
        } else {
            "architecture"
        };

        categories
            .entry(category.to_string())
            .or_default()
            .push(file.clone());
    }

    categories
}

fn trim_files(files: Vec<String>, comprehensive: bool) -> Vec<String> {
    let limit = if comprehensive { 12 } else { 6 };
    files.into_iter().take(limit).collect()
}

fn category_title(category: &str) -> String {
    match category {
        "frontend" => "Frontend Components".to_string(),
        "backend" => "Backend Systems".to_string(),
        "testing" => "Testing Strategy".to_string(),
        "configuration" => "Configuration".to_string(),
        "deployment" => "Deployment and Infrastructure".to_string(),
        "documentation" => "Project Documentation".to_string(),
        _ => "System Architecture".to_string(),
    }
}

fn importance_for_category(category: &str) -> &'static str {
    match category {
        "architecture" | "backend" | "frontend" => "high",
        "configuration" | "deployment" => "medium",
        _ => "low",
    }
}

fn repository_name(scan: &RepoScan) -> Option<String> {
    scan.root_path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
}

fn build_description(scan: &RepoScan) -> String {
    if let Some(readme) = &scan.readme_content {
        let summary = readme
            .lines()
            .map(str::trim)
            .find(|line| {
                !line.is_empty()
                    && !line.starts_with('#')
                    && !line.starts_with("---")
                    && !line.starts_with("![")
                    && !line.starts_with("[![")
            })
            .unwrap_or("Repository documentation generated from source structure.");
        return summary.to_string();
    }

    "Repository documentation generated from source structure.".to_string()
}

pub fn render_page_markdown(page: &WikiPage, scan: &RepoScan) -> String {
    let mut markdown = String::new();
    markdown.push_str("<details>\n<summary>Relevant source files</summary>\n\n");
    for file in &page.file_paths {
        markdown.push_str(&format!("- `{file}`\n"));
    }
    markdown.push_str("</details>\n\n");
    markdown.push_str(&format!("# {}\n\n", page.title));
    markdown.push_str(&format!("{}\n\n", page.description));

    markdown.push_str("## Summary\n\n");
    markdown.push_str(&format!(
        "This page was generated from the repository scan for `{}`. It currently serves as a structured Rust-owned wiki stub and lists the source files that should drive future LLM-backed generation.\n\n",
        scan.root_path.display()
    ));

    if let Some(branch) = &scan.default_branch {
        markdown.push_str("## Repository Metadata\n\n");
        markdown.push_str(&format!("- Default branch: `{branch}`\n"));
        markdown.push_str(&format!("- Scanned files: `{}`\n\n", scan.file_tree.len()));
    }

    markdown.push_str("## Source Inventory\n\n");
    for file in &page.file_paths {
        markdown.push_str(&format!("- `{file}`\n"));
    }

    markdown
}

pub fn render_wiki_markdown(structure: &WikiStructure) -> String {
    let mut markdown = String::new();
    markdown.push_str(&format!("# {}\n\n", structure.title));
    markdown.push_str(&format!("{}\n\n", structure.description));

    for section in &structure.sections {
        markdown.push_str(&format!("## {}\n\n", section.title));
        for page_id in &section.pages {
            if let Some(page) = structure.pages.iter().find(|page| &page.id == page_id) {
                markdown.push_str(&format!("- [{}](wiki/pages/{}.md)\n", page.title, page.id));
            }
        }
        markdown.push('\n');
    }

    markdown
}
