#![allow(clippy::print_stderr)]

//! Infrastructure for benchmarking real-world Python projects.
//!
//! The module uses a setup similar to mypy primer's, which should make it easy
//! to add new benchmarks for projects in [mypy primer's project's list](https://github.com/hauntsaninja/mypy_primer/blob/ebaa9fd27b51a278873b63676fd25490cec6823b/mypy_primer/projects.py#L74).
//!
//! The basic steps for a project are:
//! 1. Clone or update the project into a directory inside `./target`. The commits are pinnted to prevent flaky benchmark results due to new commits.
//! 2. For projects with dependencies, run uv to create a virtual environment and install the dependencies.
//! 3. (optionally) Copy the entire project structure into a memory file system to reduce the IO noise in benchmarks.
//! 4. (not in this module) Create a `ProjectDatabase` and run the benchmark.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use anyhow::{Context, Result};
use ruff_db::system::{MemoryFileSystem, SystemPath, SystemPathBuf};
use ruff_python_ast::PythonVersion;

/// Configuration for a real-world project to benchmark
#[derive(Debug, Clone)]
pub struct RealWorldProject<'a> {
    // The name of the project.
    pub name: &'a str,
    /// The project's GIT repository. Must be publicly accessible.
    pub repository: &'a str,
    /// Specific commit hash to checkout
    pub commit: &'a str,
    /// List of paths within the project to check (`ty check <paths>`)
    pub paths: &'a [&'a str],
    /// Dependencies to install via uv
    pub dependencies: &'a [&'a str],
    /// Limit candidate packages to those that were uploaded prior to a given point in time (ISO 8601 format).
    /// Maps to uv's `exclude-newer`.
    pub max_dep_date: &'a str,
    /// Python version to use
    pub python_version: PythonVersion,
}

impl<'a> RealWorldProject<'a> {
    /// Setup a real-world project for benchmarking
    pub fn setup(self) -> Result<InstalledProject<'a>> {
        let start = Instant::now();
        tracing::debug!("Setting up project {}", self.name);

        // Create project directory in cargo target
        let project_root = get_project_cache_dir(self.name)?;

        // Clone the repository if it doesn't exist, or update if it does
        if project_root.exists() {
            tracing::debug!("Updating repository for project '{}'...", self.name);
            let start = std::time::Instant::now();
            update_repository(&project_root, self.commit)?;
            tracing::debug!(
                "Repository update completed in {:.2}s",
                start.elapsed().as_secs_f64()
            );
        } else {
            tracing::debug!("Cloning repository for project '{}'...", self.name);
            let start = std::time::Instant::now();
            clone_repository(self.repository, &project_root, self.commit)?;
            tracing::debug!(
                "Repository clone completed in {:.2}s",
                start.elapsed().as_secs_f64()
            );
        }

        let checkout = Checkout {
            path: project_root,
            project: self,
        };

        // Install dependencies if specified
        tracing::debug!(
            "Installing {} dependencies for project '{}'...",
            checkout.project().dependencies.len(),
            checkout.project().name
        );
        let start_install = std::time::Instant::now();
        install_dependencies(&checkout)?;
        tracing::debug!(
            "Dependency installation completed in {:.2}s",
            start_install.elapsed().as_secs_f64()
        );

        tracing::debug!("Project setup took: {:.2}s", start.elapsed().as_secs_f64());

        Ok(InstalledProject {
            path: checkout.path,
            config: checkout.project,
        })
    }
}

struct Checkout<'a> {
    project: RealWorldProject<'a>,
    path: PathBuf,
}

impl<'a> Checkout<'a> {
    /// Get the virtual environment path
    fn venv_path(&self) -> PathBuf {
        self.path.join(".venv")
    }

    fn project(&self) -> &RealWorldProject<'a> {
        &self.project
    }
}

/// Checked out project with its dependencies installed.
pub struct InstalledProject<'a> {
    /// Path to the cloned project
    pub path: PathBuf,
    /// Project configuration
    pub config: RealWorldProject<'a>,
}

impl<'a> InstalledProject<'a> {
    /// Get the project configuration
    pub fn config(&self) -> &RealWorldProject<'a> {
        &self.config
    }

    /// Get the benchmark paths
    pub fn check_paths(&self) -> &[&str] {
        self.config.paths
    }

    /// Get the virtual environment path
    pub fn venv_path(&self) -> PathBuf {
        self.path.join(".venv")
    }

    /// Copies the entire project to a memory file system.
    pub fn copy_to_memory_fs(&self) -> anyhow::Result<MemoryFileSystem> {
        let fs = MemoryFileSystem::new();

        copy_directory_recursive(&fs, &self.path, &SystemPathBuf::from("/"))?;

        Ok(fs)
    }
}

/// Get the cache directory for a project in the cargo target directory
fn get_project_cache_dir(project_name: &str) -> Result<std::path::PathBuf> {
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

/// Update an existing repository
fn update_repository(project_root: &Path, commit: &str) -> Result<()> {
    let output = Command::new("git")
        .args(["fetch", "origin", commit])
        .current_dir(project_root)
        .output()
        .context("Failed to execute git fetch command")?;

    if !output.status.success() {
        anyhow::bail!(
            "Git fetch of commit {} failed: {}",
            commit,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // Checkout specific commit
    let output = Command::new("git")
        .args(["checkout", commit])
        .current_dir(project_root)
        .output()
        .context("Failed to execute git checkout command")?;

    anyhow::ensure!(
        output.status.success(),
        "Git checkout of commit {} failed: {}",
        commit,
        String::from_utf8_lossy(&output.stderr)
    );

    Ok(())
}

/// Clone a git repository to the specified directory
fn clone_repository(repo_url: &str, target_dir: &Path, commit: &str) -> Result<()> {
    // Create parent directory if it doesn't exist
    if let Some(parent) = target_dir.parent() {
        std::fs::create_dir_all(parent).context("Failed to create parent directory for clone")?;
    }

    // Clone with minimal depth and fetch only the specific commit
    let output = Command::new("git")
        .args([
            "clone",
            "--filter=blob:none", // Don't download large files initially
            "--no-checkout",      // Don't checkout files yet
            repo_url,
            target_dir.to_str().unwrap(),
        ])
        .output()
        .context("Failed to execute git clone command")?;

    anyhow::ensure!(
        output.status.success(),
        "Git clone failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Fetch the specific commit
    let output = Command::new("git")
        .args(["fetch", "origin", commit])
        .current_dir(target_dir)
        .output()
        .context("Failed to execute git fetch command")?;

    anyhow::ensure!(
        output.status.success(),
        "Git fetch of commit {} failed: {}",
        commit,
        String::from_utf8_lossy(&output.stderr)
    );

    // Checkout the specific commit
    let output = Command::new("git")
        .args(["checkout", commit])
        .current_dir(target_dir)
        .output()
        .context("Failed to execute git checkout command")?;

    anyhow::ensure!(
        output.status.success(),
        "Git checkout of commit {} failed: {}",
        commit,
        String::from_utf8_lossy(&output.stderr)
    );

    Ok(())
}

/// Install dependencies using uv with date constraints
fn install_dependencies(checkout: &Checkout) -> Result<()> {
    // Check if uv is available
    let uv_check = Command::new("uv")
        .arg("--version")
        .output()
        .context("Failed to execute uv version check.")?;

    if !uv_check.status.success() {
        anyhow::bail!(
            "uv is not installed or not found in PATH. If you need to install it, follow the instructions at https://docs.astral.sh/uv/getting-started/installation/"
        );
    }

    let venv_path = checkout.venv_path();
    let python_version_str = checkout.project().python_version.to_string();

    let output = Command::new("uv")
        .args(["venv", "--python", &python_version_str, "--allow-existing"])
        .arg(&venv_path)
        .output()
        .context("Failed to execute uv venv command")?;

    anyhow::ensure!(
        output.status.success(),
        "Failed to create virtual environment: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    if checkout.project().dependencies.is_empty() {
        tracing::debug!(
            "No dependencies to install for project '{}'",
            checkout.project().name
        );
        return Ok(());
    }

    // Install dependencies with date constraint in the isolated environment
    let mut cmd = Command::new("uv");
    cmd.args([
        "pip",
        "install",
        "--python",
        venv_path.to_str().unwrap(),
        "--exclude-newer",
        checkout.project().max_dep_date,
    ])
    .args(checkout.project().dependencies);

    let output = cmd
        .output()
        .context("Failed to execute uv pip install command")?;

    anyhow::ensure!(
        output.status.success(),
        "Dependency installation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    Ok(())
}

/// Recursively load a directory into the memory filesystem
fn copy_directory_recursive(
    fs: &MemoryFileSystem,
    source_path: &Path,
    dest_path: &SystemPath,
) -> Result<()> {
    if source_path.is_file() {
        if source_path.file_name().and_then(OsStr::to_str) == Some("pyvenv.cfg") {
            // Skip pyvenv.cfg files because the Python path will be invalid.
            return Ok(());
        }

        match std::fs::read_to_string(source_path) {
            Ok(content) => {
                fs.write_file_all(dest_path.to_path_buf(), content)
                    .with_context(|| {
                        format!("Failed to write file to memory filesystem: {dest_path}")
                    })?;
            }
            Err(error) => {
                if error.kind() == std::io::ErrorKind::InvalidData {
                    // Skip binary files.
                    return Ok(());
                }
                return Err(error)
                    .with_context(|| format!("Failed to read file: {}", source_path.display()));
            }
        }
    } else if source_path.is_dir() {
        // Create directory in memory fs
        fs.create_directory_all(dest_path.to_path_buf())
            .with_context(|| {
                format!("Failed to create directory in memory filesystem: {dest_path}")
            })?;

        // Read directory contents
        let entries = std::fs::read_dir(source_path)
            .with_context(|| format!("Failed to read directory: {}", source_path.display()))?;

        for entry in entries {
            let entry = entry.with_context(|| {
                format!("Failed to read directory entry: {}", source_path.display())
            })?;

            let file_name = entry.file_name();
            let file_name = file_name.to_str().context("Expected UTF8 path")?;
            let source_child = source_path.join(file_name);
            let dest_child = dest_path.join(file_name);

            // Skip hidden files and common non-Python directories
            if file_name != ".venv" && (file_name.starts_with('.') || matches!(file_name, ".git")) {
                continue;
            }

            copy_directory_recursive(fs, &source_child, &dest_child)?;
        }
    }

    Ok(())
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
                        .args(["metadata", "--format-version", "1"])
                        .output()
                        .ok()?;
                    let metadata: Metadata = serde_json::from_slice(&output.stdout).ok()?;
                    Some(metadata.target_directory)
                })
        })
        .as_ref()
}
