use crate::domain::WikiStructure;

pub fn generate_slides_markdown(structure: &WikiStructure) -> String {
    let mut markdown = String::new();
    markdown.push_str(&format!("# {} Slides\n\n", structure.title));
    markdown.push_str("## Slide 1: Project Overview\n\n");
    markdown.push_str(&format!("{}\n\n", structure.description));

    for (index, section) in structure.sections.iter().enumerate() {
        markdown.push_str(&format!("## Slide {}: {}\n\n", index + 2, section.title));
        for page_id in &section.pages {
            if let Some(page) = structure.pages.iter().find(|page| &page.id == page_id) {
                markdown.push_str(&format!("- {}: {}\n", page.title, page.description));
            }
        }
        markdown.push('\n');
    }

    markdown
}

pub fn generate_workshop_markdown(structure: &WikiStructure) -> String {
    let mut markdown = String::new();
    markdown.push_str(&format!("# {} Workshop\n\n", structure.title));
    markdown.push_str("## Introduction\n\n");
    markdown.push_str(&format!("{}\n\n", structure.description));

    for (index, page) in structure.pages.iter().enumerate() {
        markdown.push_str(&format!("## Exercise {}: {}\n\n", index + 1, page.title));
        markdown.push_str(&format!("{}\n\n", page.description));
        markdown.push_str("### Relevant Files\n\n");
        for file in &page.file_paths {
            markdown.push_str(&format!("- `{file}`\n"));
        }
        markdown.push_str("\n### Challenge\n\n");
        markdown.push_str("Trace how this area is implemented and extend the documentation with examples.\n\n");
    }

    markdown
}
