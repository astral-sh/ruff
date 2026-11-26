//! Pinned Git checkouts for whole-project benchmarks.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};

/// Obtain a pinned checkout without installing any project dependencies.
pub fn checkout_project(name: &str, repository: &str, commit: &str) -> Result<PathBuf> {
    let path = get_project_cache_dir(name)?;
    std::fs::create_dir_all(&path)?;
    if !path.join(".git").is_dir() {
        run_git(&path, &["init", "--quiet"])?;
    }

    // A cached revision can be checked out offline.
    if !Command::new("git")
        .args(["cat-file", "-e", commit])
        .current_dir(&path)
        .output()?
        .status
        .success()
    {
        run_git(
            &path,
            &["fetch", "--depth=1", "--no-tags", repository, commit],
        )?;
    }
    run_git(&path, &["checkout", "--quiet", "--detach", commit])?;
    Ok(path)
}

fn run_git(path: &Path, args: &[&str]) -> Result<()> {
    let output = Command::new("git")
        .args(args)
        .current_dir(path)
        .output()
        .with_context(|| format!("Failed to run git in {}", path.display()))?;
    anyhow::ensure!(
        output.status.success(),
        "git {} failed in {}: {}",
        args.join(" "),
        path.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

/// Get the cache directory for a project in the cargo target directory
pub fn get_project_cache_dir(project_name: &str) -> Result<PathBuf> {
    let target_dir = cargo_target_directory()
        .cloned()
        .unwrap_or_else(|| PathBuf::from("target"));
    let target_dir =
        std::path::absolute(target_dir).context("Failed to construct an absolute path")?;
    let cache_dir = target_dir.join("benchmark_cache").join(project_name);

    if let Some(parent) = cache_dir.parent() {
        std::fs::create_dir_all(parent).context("Failed to create cache directory")?;
    }

    Ok(cache_dir)
}

static CARGO_TARGET_DIR: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();

fn cargo_target_directory() -> Option<&'static PathBuf> {
    CARGO_TARGET_DIR
        .get_or_init(|| {
            #[derive(serde::Deserialize)]
            struct Metadata {
                target_directory: PathBuf,
            }

            std::env::var_os("CARGO_TARGET_DIR")
                .map(PathBuf::from)
                .or_else(|| {
                    let output = Command::new(std::env::var_os("CARGO")?)
                        .args(["metadata", "--no-deps", "--format-version", "1"])
                        .output()
                        .ok()?;
                    let metadata: Metadata = serde_json::from_slice(&output.stdout).ok()?;
                    Some(metadata.target_directory)
                })
        })
        .as_ref()
}
