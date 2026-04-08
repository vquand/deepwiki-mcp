use std::io;
use std::path::Path;
use std::process::Command;

pub fn git_clone(url: &str, target_dir: &Path) -> io::Result<()> {
    let status = Command::new("git")
        .arg("clone")
        .arg("--depth=1")
        .arg("--single-branch")
        .arg(url)
        .arg(target_dir)
        .status()?;

    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("git clone failed for {url}")))
    }
}

pub fn git_pull(repo_dir: &Path) -> io::Result<()> {
    let status = Command::new("git")
        .arg("-C")
        .arg(repo_dir)
        .arg("pull")
        .arg("--ff-only")
        .status()?;

    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "git pull failed for {}",
            repo_dir.display()
        )))
    }
}
