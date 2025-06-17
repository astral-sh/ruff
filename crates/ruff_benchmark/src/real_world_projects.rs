//! Infrastructure for benchmarking real-world Python projects.
//!
//! This module provides functionality to:
//! 1. Clone external repositories to temporary directories
//! 2. Install dependencies using uv with date constraints
//! 3. Load project files into `MemoryFileSystem` for benchmarking

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use ruff_db::system::{MemoryFileSystem, SystemPath, SystemPathBuf};
use ruff_python_ast::PythonVersion;

/// Configuration for a real-world project to benchmark
#[derive(Debug, Clone)]
pub struct RealWorldProject<'a> {
    pub name: &'a str,

    /// Git repository URL
    pub location: &'a str,
    /// Specific commit hash to checkout
    pub commit: &'a str,
    /// List of paths within the project to check
    pub paths: &'a [&'a SystemPath],
    /// Dependencies to install via uv
    pub dependencies: &'a [&'a str],
    /// Date constraint for dependencies (ISO 8601 format)
    pub max_dep_date: &'a str,
    /// Python version to use
    pub python_version: PythonVersion,
}

impl<'a> RealWorldProject<'a> {
    /// Setup a real-world project for benchmarking
    pub fn setup(self) -> Result<SetupProject<'a>> {
        // Create project directory in cargo target
        let project_root = get_project_cache_dir(self.name)?;

        // Clone the repository if it doesn't exist, or update if it does
        if project_root.exists() {
            update_repository(&project_root, self.commit)?;
        } else {
            clone_repository(self.location, &project_root, self.commit)?;
        }

        let checkout = Checkout {
            path: project_root,
            project: self,
        };

        // Install dependencies if specified
        if !checkout.project().dependencies.is_empty() {
            install_dependencies(&checkout)?;
        }

        // Load files into memory filesystem
        let memory_fs = load_into_memory_fs(&checkout.path)?;

        Ok(SetupProject {
            path: checkout.path,
            memory_fs,
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

/// A setup real-world project ready for benchmarking
pub struct SetupProject<'a> {
    /// Path to the cloned project
    pub path: PathBuf,
    /// Memory filesystem containing the project files
    pub memory_fs: MemoryFileSystem,
    /// Project configuration
    pub config: RealWorldProject<'a>,
}

impl<'a> SetupProject<'a> {
    /// Get the memory filesystem for benchmarking
    pub fn memory_fs(&self) -> &MemoryFileSystem {
        &self.memory_fs
    }

    /// Get the project configuration
    pub fn config(&self) -> &RealWorldProject<'a> {
        &self.config
    }

    /// Get the benchmark paths as `SystemPathBuf`
    pub fn check_paths(&self) -> &'a [&SystemPath] {
        self.config.paths
    }

    /// Get the virtual environment path
    pub fn venv_path(&self) -> PathBuf {
        self.path.join(".venv")
    }
}

/// Get the cache directory for a project in the cargo target directory
fn get_project_cache_dir(project_name: &str) -> Result<std::path::PathBuf> {
    let target_dir = cargo_target_directory()
        .cloned()
        .unwrap_or_else(|| PathBuf::from("target"));
    let cache_dir = target_dir.join("benchmark_cache").join(project_name);

    if let Some(parent) = cache_dir.parent() {
        std::fs::create_dir_all(parent).context("Failed to create cache directory")?;
    }

    Ok(cache_dir)
}

/// Update an existing repository
fn update_repository(project_root: &Path, commit: &str) -> Result<()> {
    // Fetch latest changes
    let output = Command::new("git")
        .args(["fetch", "origin"])
        .current_dir(project_root)
        .output()
        .context("Failed to execute git fetch command")?;

    if !output.status.success() {
        anyhow::bail!(
            "Git fetch failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // Checkout specific commit
    let target = commit;
    let output = Command::new("git")
        .args(["reset", "--hard", target])
        .current_dir(project_root)
        .output()
        .context("Failed to execute git reset command")?;

    if !output.status.success() {
        anyhow::bail!(
            "Git checkout of commit {} failed: {}",
            target,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(())
}

/// Clone a git repository to the specified directory
fn clone_repository(repo_url: &str, target_dir: &Path, commit: &str) -> Result<()> {
    // Create parent directory if it doesn't exist
    if let Some(parent) = target_dir.parent() {
        std::fs::create_dir_all(parent).context("Failed to create parent directory for clone")?;
    }

    // Clone the repository
    let output = Command::new("git")
        .args(["clone", repo_url, target_dir.to_str().unwrap()])
        .output()
        .context("Failed to execute git clone command")?;

    if !output.status.success() {
        anyhow::bail!(
            "Git clone failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // Checkout specific commit
    let output = Command::new("git")
        .args(["checkout", commit])
        .current_dir(target_dir)
        .output()
        .context("Failed to execute git checkout command")?;

    if !output.status.success() {
        anyhow::bail!(
            "Git checkout of commit {} failed: {}",
            commit,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(())
}

/// Install dependencies using uv with date constraints
fn install_dependencies(checkout: &Checkout) -> Result<()> {
    // Check if uv is available
    let uv_check = Command::new("uv")
        .arg("--version")
        .output()
        .context("Failed to execute uv version check")?;

    if !uv_check.status.success() {
        anyhow::bail!("uv is not installed or not found in PATH");
    }

    // Create an isolated virtual environment to avoid picking up ruff's pyproject.toml
    let venv_path = checkout.venv_path();
    let python_version_str = checkout.project().python_version.to_string();

    // Only create venv if it doesn't exist
    if !venv_path.exists() {
        let output = Command::new("uv")
            .args(["venv", "--python", &python_version_str])
            .arg(&venv_path)
            .output()
            .context("Failed to execute uv venv command")?;

        if !output.status.success() {
            anyhow::bail!(
                "Failed to create virtual environment: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }

    // Create a requirements file with dependencies
    let requirements_content = checkout.project().dependencies.join("\n");
    let requirements_path = checkout.path.join("benchmark_requirements.txt");
    std::fs::write(&requirements_path, requirements_content)
        .context("Failed to write requirements file")?;

    // Install dependencies with date constraint in the isolated environment
    let mut cmd = Command::new("uv");
    cmd.args([
        "pip",
        "install",
        "--python",
        venv_path.to_str().unwrap(),
        "--requirement",
        requirements_path.to_str().unwrap(),
    ]);

    // Add date constraint if specified

    cmd.args(["--exclude-newer", checkout.project().max_dep_date]);

    let output = cmd
        .output()
        .context("Failed to execute uv pip install command")?;

    if !output.status.success() {
        anyhow::bail!(
            "Dependency installation failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(())
}

/// Load project files into a `MemoryFileSystem`
fn load_into_memory_fs(path: &Path) -> Result<MemoryFileSystem> {
    let fs = MemoryFileSystem::new();

    load_directory_recursive(&fs, path, &SystemPathBuf::from("/"))?;

    Ok(fs)
}

/// Recursively load a directory into the memory filesystem
fn load_directory_recursive(
    fs: &MemoryFileSystem,
    source_path: &Path,
    dest_path: &SystemPath,
) -> Result<()> {
    if source_path.is_file() {
        if source_path.file_name().and_then(OsStr::to_str) == Some("pyvenv.cfg") {
            // Skip pyenv.cfg files because the Python path will be invalid.
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
                    // Skip non UTF-8 files
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
            let source_child = source_path.join(&file_name);
            let dest_child = dest_path.join(file_name.to_string_lossy().as_ref());

            // Skip hidden files and common non-Python directories
            let file_name_str = file_name.to_string_lossy();
            if file_name != ".venv" && file_name_str.starts_with('.')
                || file_name_str == "__pycache__"
                || file_name_str == "node_modules"
                || file_name_str == ".git"
            {
                continue;
            }

            load_directory_recursive(fs, &source_child, &dest_child)?;
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
