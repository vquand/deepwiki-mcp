use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::domain::RepoScan;

const README_CANDIDATES: &[&str] = &[
    "README.md",
    "README.txt",
    "README.rst",
    "readme.md",
    "readme.txt",
    "readme.rst",
];

const IGNORED_DIRS: &[&str] = &[
    ".git",
    ".deepwiki-mcp",
    "node_modules",
    "target",
    "__pycache__",
    ".next",
    "dist",
    "build",
];

pub fn scan_local_repository(repo_path: &Path) -> io::Result<RepoScan> {
    let repo_path = repo_path.canonicalize()?;
    let file_tree = collect_file_tree(&repo_path)?;
    let (readme_path, readme_content) = load_readme(&repo_path)?;
    let default_branch = detect_default_branch(&repo_path);

    Ok(RepoScan {
        scanned_at_epoch_ms: current_epoch_ms()?,
        root_path: repo_path,
        default_branch,
        readme_path,
        readme_content,
        file_tree,
    })
}

fn collect_file_tree(root: &Path) -> io::Result<Vec<String>> {
    let mut files = Vec::new();
    walk_dir(root, root, &mut files)?;
    files.sort();
    Ok(files)
}

fn walk_dir(root: &Path, dir: &Path, files: &mut Vec<String>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;

        if file_type.is_dir() {
            let dir_name = entry.file_name().to_string_lossy().to_string();
            if IGNORED_DIRS.iter().any(|ignored| *ignored == dir_name) {
                continue;
            }
            walk_dir(root, &path, files)?;
            continue;
        }

        if file_type.is_file() {
            let relative = path
                .strip_prefix(root)
                .unwrap_or(path.as_path())
                .to_string_lossy()
                .to_string();
            files.push(relative);
        }
    }
    Ok(())
}

fn load_readme(root: &Path) -> io::Result<(Option<PathBuf>, Option<String>)> {
    for candidate in README_CANDIDATES {
        let path = root.join(candidate);
        if path.exists() {
            let content = fs::read_to_string(&path)?;
            return Ok((Some(path), Some(content)));
        }
    }
    Ok((None, None))
}

fn detect_default_branch(root: &Path) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("rev-parse")
        .arg("--abbrev-ref")
        .arg("HEAD")
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if branch.is_empty() {
        None
    } else {
        Some(branch)
    }
}

fn current_epoch_ms() -> io::Result<u128> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .map_err(io::Error::other)
}
