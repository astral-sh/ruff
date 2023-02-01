use std::fs::{create_dir_all, read_to_string, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::Result;
use colored::Colorize;

const CARGO_PKG_NAME: &str = env!("CARGO_PKG_NAME");
const CARGO_PKG_VERSION: &str = env!("CARGO_PKG_VERSION");

fn cache_dir() -> &'static str {
    "./.ruff_cache"
}

fn file_path() -> PathBuf {
    Path::new(cache_dir()).join(".update-informer")
}

/// Get the "latest" version for which the user has been informed.
fn get_latest() -> Result<Option<String>> {
    let path = file_path();
    if path.exists() {
        Ok(Some(read_to_string(path)?.trim().to_string()))
    } else {
        Ok(None)
    }
}

/// Set the "latest" version for which the user has been informed.
fn set_latest(version: &str) -> Result<()> {
    create_dir_all(cache_dir())?;
    let path = file_path();
    let mut file = File::create(path)?;
    file.write_all(version.trim().as_bytes())?;
    Ok(())
}

/// Update the user if a newer version is available.
pub fn check_for_updates() -> Result<()> {
    use update_informer::{registry, Check};

    let informer = update_informer::new(registry::PyPI, CARGO_PKG_NAME, CARGO_PKG_VERSION);

    if let Some(new_version) = informer
        .check_version()
        .ok()
        .flatten()
        .map(|version| version.to_string())
    {
        // If we've already notified the user about this version, return early.
        if let Some(latest_version) = get_latest()? {
            if latest_version == new_version {
                return Ok(());
            }
        }
        set_latest(&new_version)?;

        let msg = format!(
            "A new version of {pkg_name} is available: v{pkg_version} -> {new_version}",
            pkg_name = CARGO_PKG_NAME.italic().cyan(),
            pkg_version = CARGO_PKG_VERSION,
            new_version = new_version.green()
        );

        let cmd = format!(
            "Run to update: {cmd} {pkg_name}",
            cmd = "pip3 install --upgrade".green(),
            pkg_name = CARGO_PKG_NAME.green()
        );

        println!("\n{msg}\n{cmd}");
    }

    Ok(())
}
