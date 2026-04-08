use std::fs;
use std::io;
use std::path::Path;

use crate::domain::WikiStructure;
use crate::wiki::render_wiki_markdown;

pub fn export_wiki_markdown(output_path: &Path, structure: &WikiStructure) -> io::Result<()> {
    fs::write(output_path, render_wiki_markdown(structure))
}
