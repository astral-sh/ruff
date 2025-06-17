//! Infrastructure for benchmarking real-world Python projects.
//!
//! This module provides functionality to:
//! 1. Clone external repositories to temporary directories
//! 2. Install dependencies using uv with date constraints
//! 3. Load project files into MemoryFileSystem for benchmarking

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

use ruff_db::system::{MemoryFileSystem, SystemPath, SystemPathBuf};
use ruff_python_ast::PythonVersion;

/// Configuration for a real-world project to benchmark
#[derive(Debug, Clone)]
pub struct RealWorldProject {
    /// Git repository URL
    pub location: String,
    /// Specific commit hash to checkout (optional, uses latest if None)
    pub commit: Option<String>,
    /// List of paths within the project to check
    pub paths: Vec<String>,
    /// Dependencies to install via uv
    pub deps: Vec<String>,
    /// Date constraint for dependencies (ISO 8601 format)
    pub max_dep_date: String,
    /// Python version to use
    pub python_version: PythonVersion,
    /// Estimated benchmark costs for reference
    pub cost: HashMap<String, u32>,
}

impl RealWorldProject {
    /// Create a new real-world project configuration
    pub fn new(location: impl Into<String>) -> Self {
        Self {
            location: location.into(),
            commit: None,
            paths: Vec::new(),
            deps: Vec::new(),
            max_dep_date: "2025-06-17".to_string(), // Default to today's date
            python_version: PythonVersion::PY311,   // Default Python version
            cost: HashMap::new(),
        }
    }

    /// Set the specific commit to checkout
    pub fn with_commit(mut self, commit: impl Into<String>) -> Self {
        self.commit = Some(commit.into());
        self
    }

    /// Add paths to benchmark within the project
    pub fn with_paths(mut self, paths: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.paths.extend(paths.into_iter().map(|p| p.into()));
        self
    }

    /// Add dependencies to install
    pub fn with_deps(mut self, deps: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.deps.extend(deps.into_iter().map(|d| d.into()));
        self
    }

    /// Set the maximum date for dependencies
    pub fn with_max_dep_date(mut self, date: impl Into<String>) -> Self {
        self.max_dep_date = date.into();
        self
    }

    /// Add cost estimates
    pub fn with_cost(mut self, tool: impl Into<String>, cost: u32) -> Self {
        self.cost.insert(tool.into(), cost);
        self
    }

    /// Set the Python version to use
    pub fn with_python_version(mut self, version: PythonVersion) -> Self {
        self.python_version = version;
        self
    }
}

/// A setup real-world project ready for benchmarking
pub struct SetupProject {
    /// Memory filesystem containing the project files
    pub memory_fs: MemoryFileSystem,
    /// Project configuration
    pub config: RealWorldProject,
}

impl SetupProject {
    /// Get the memory filesystem for benchmarking
    pub fn memory_fs(&self) -> &MemoryFileSystem {
        &self.memory_fs
    }

    /// Get the project configuration
    pub fn config(&self) -> &RealWorldProject {
        &self.config
    }

    /// Get the benchmark paths as SystemPathBuf
    pub fn benchmark_paths(&self) -> Vec<SystemPathBuf> {
        self.config
            .paths
            .iter()
            .map(|path| SystemPathBuf::from("src").join(path))
            .collect()
    }
}

/// Setup a real-world project for benchmarking
pub fn setup_real_world_project(
    project: RealWorldProject,
) -> std::result::Result<SetupProject, SetupError> {
    // Create temporary directory
    let temp_dir = tempfile::TempDir::new().map_err(SetupError::TempDir)?;
    let project_root = temp_dir.path().to_path_buf();

    // Clone the repository
    clone_repository(&project.location, &project_root, project.commit.as_deref())?;

    // Install dependencies if specified
    if !project.deps.is_empty() {
        install_dependencies(
            &project_root,
            &project.deps,
            &project.max_dep_date,
            project.python_version,
        )?;
    }

    // Load files into memory filesystem
    let memory_fs = load_into_memory_fs(&project_root, &project.paths)?;

    Ok(SetupProject {
        memory_fs,
        config: project,
    })
}

/// Clone a git repository to the specified directory
fn clone_repository(
    repo_url: &str,
    target_dir: &Path,
    commit: Option<&str>,
) -> std::result::Result<(), SetupError> {
    // Clone the repository
    let output = Command::new("git")
        .args(["clone", "--depth", "1", repo_url, "."])
        .current_dir(target_dir)
        .output()
        .map_err(SetupError::GitCommand)?;

    if !output.status.success() {
        return Err(SetupError::GitClone {
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        });
    }

    // Checkout specific commit if provided
    if let Some(commit_hash) = commit {
        // First, unshallow the repository to get all commits
        let output = Command::new("git")
            .args(["fetch", "--unshallow"])
            .current_dir(target_dir)
            .output()
            .map_err(SetupError::GitCommand)?;

        if !output.status.success() {
            return Err(SetupError::GitFetch {
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            });
        }

        // Checkout the specific commit
        let output = Command::new("git")
            .args(["checkout", commit_hash])
            .current_dir(target_dir)
            .output()
            .map_err(SetupError::GitCommand)?;

        if !output.status.success() {
            return Err(SetupError::GitCheckout {
                commit: commit_hash.to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            });
        }
    }

    Ok(())
}

/// Install dependencies using uv with date constraints
fn install_dependencies(
    project_root: &Path,
    deps: &[String],
    max_date: &str,
    python_version: PythonVersion,
) -> std::result::Result<(), SetupError> {
    // Check if uv is available
    let uv_check = Command::new("uv")
        .arg("--version")
        .output()
        .map_err(SetupError::UvCommand)?;

    if !uv_check.status.success() {
        return Err(SetupError::UvNotFound);
    }

    // Create an isolated virtual environment to avoid picking up ruff's pyproject.toml
    let venv_path = project_root.join(".benchmark_venv");
    let python_version_str = python_version.to_string();
    let output = Command::new("uv")
        .args(["venv", "--python", &python_version_str])
        .arg(&venv_path)
        .current_dir(project_root)
        .output()
        .map_err(SetupError::UvCommand)?;

    if !output.status.success() {
        return Err(SetupError::VenvCreation {
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        });
    }

    // Create a requirements file with dependencies
    let requirements_content = deps.join("\n");
    let requirements_path = project_root.join("benchmark_requirements.txt");
    std::fs::write(&requirements_path, requirements_content)
        .map_err(SetupError::WriteRequirements)?;

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
    if !max_date.is_empty() {
        cmd.args(["--exclude-newer", max_date]);
    }

    let output = cmd
        .current_dir(project_root)
        .output()
        .map_err(SetupError::UvCommand)?;

    if !output.status.success() {
        return Err(SetupError::DependencyInstall {
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        });
    }

    Ok(())
}

/// Load project files into a MemoryFileSystem
fn load_into_memory_fs(
    project_root: &Path,
    benchmark_paths: &[String],
) -> std::result::Result<MemoryFileSystem, SetupError> {
    let fs = MemoryFileSystem::new();

    // Walk through each benchmark path and load Python files
    for path in benchmark_paths {
        let full_path = project_root.join(path);
        if !full_path.exists() {
            return Err(SetupError::PathNotFound(path.clone()));
        }

        load_directory_recursive(&fs, &full_path, &SystemPathBuf::from("src").join(path))?;
    }

    Ok(fs)
}

/// Recursively load a directory into the memory filesystem
fn load_directory_recursive(
    fs: &MemoryFileSystem,
    source_path: &Path,
    dest_path: &SystemPath,
) -> std::result::Result<(), SetupError> {
    if source_path.is_file() {
        // Load single file
        if let Some(ext) = source_path.extension() {
            if ext == "py" || ext == "pyi" {
                let content =
                    std::fs::read_to_string(source_path).map_err(|e| SetupError::ReadFile {
                        path: source_path.to_path_buf(),
                        error: e,
                    })?;
                if let Err(e) = fs.write_file_all(dest_path.to_path_buf(), content) {
                    return Err(SetupError::WriteMemoryFs(format!("{:?}", e)));
                }
            }
        }
    } else if source_path.is_dir() {
        // Create directory in memory fs
        if let Err(e) = fs.create_directory_all(dest_path.to_path_buf()) {
            return Err(SetupError::WriteMemoryFs(format!("{:?}", e)));
        }

        // Read directory contents
        let entries = std::fs::read_dir(source_path).map_err(|e| SetupError::ReadDir {
            path: source_path.to_path_buf(),
            error: e,
        })?;

        for entry in entries {
            let entry = entry.map_err(|e| SetupError::ReadDir {
                path: source_path.to_path_buf(),
                error: e,
            })?;

            let file_name = entry.file_name();
            let source_child = source_path.join(&file_name);
            let dest_child = dest_path.join(file_name.to_string_lossy().as_ref());

            // Skip hidden files and common non-Python directories
            let file_name_str = file_name.to_string_lossy();
            if file_name_str.starts_with('.')
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

/// Errors that can occur during project setup
#[derive(Debug)]
pub enum SetupError {
    TempDir(std::io::Error),
    GitCommand(std::io::Error),
    GitClone {
        stderr: String,
    },
    GitFetch {
        stderr: String,
    },
    GitCheckout {
        commit: String,
        stderr: String,
    },
    UvCommand(std::io::Error),
    UvNotFound,
    VenvCreation {
        stderr: String,
    },
    WriteRequirements(std::io::Error),
    DependencyInstall {
        stderr: String,
    },
    PathNotFound(String),
    ReadFile {
        path: std::path::PathBuf,
        error: std::io::Error,
    },
    ReadDir {
        path: std::path::PathBuf,
        error: std::io::Error,
    },
    WriteMemoryFs(String),
}

impl std::fmt::Display for SetupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SetupError::TempDir(e) => write!(f, "Failed to create temporary directory: {}", e),
            SetupError::GitCommand(e) => write!(f, "Failed to execute git command: {}", e),
            SetupError::GitClone { stderr } => write!(f, "Git clone failed: {}", stderr),
            SetupError::GitFetch { stderr } => write!(f, "Git fetch failed: {}", stderr),
            SetupError::GitCheckout { commit, stderr } => {
                write!(f, "Git checkout of commit {} failed: {}", commit, stderr)
            }
            SetupError::UvCommand(e) => write!(f, "Failed to execute uv command: {}", e),
            SetupError::UvNotFound => write!(f, "uv is not installed or not found in PATH"),
            SetupError::VenvCreation { stderr } => {
                write!(f, "Failed to create virtual environment: {}", stderr)
            }
            SetupError::WriteRequirements(e) => {
                write!(f, "Failed to write requirements file: {}", e)
            }
            SetupError::DependencyInstall { stderr } => {
                write!(f, "Dependency installation failed: {}", stderr)
            }
            SetupError::PathNotFound(path) => write!(f, "Benchmark path not found: {}", path),
            SetupError::ReadFile { path, error } => {
                write!(f, "Failed to read file {}: {}", path.display(), error)
            }
            SetupError::ReadDir { path, error } => {
                write!(f, "Failed to read directory {}: {}", path.display(), error)
            }
            SetupError::WriteMemoryFs(e) => {
                write!(f, "Failed to write to memory filesystem: {}", e)
            }
        }
    }
}

impl std::error::Error for SetupError {}

/// Pre-defined real-world projects for benchmarking
pub mod projects {
    use super::*;

    /// The colour-science/colour project
    pub fn colour_science() -> RealWorldProject {
        RealWorldProject::new("https://github.com/colour-science/colour")
            .with_paths(["colour"])
            .with_deps([
                "matplotlib",
                "numpy",
                "pandas-stubs",
                "pytest",
                "scipy-stubs",
            ])
            .with_max_dep_date("2025-06-17")
            .with_python_version(PythonVersion::PY311)
            .with_cost("mypy", 800)
            .with_cost("pyright", 180)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_real_world_project_builder() {
        let project = RealWorldProject::new("https://github.com/example/repo")
            .with_commit("abc123")
            .with_paths(["src", "tests"])
            .with_deps(["requests", "pytest"])
            .with_max_dep_date("2024-01-01")
            .with_cost("mypy", 100);

        assert_eq!(project.location, "https://github.com/example/repo");
        assert_eq!(project.commit, Some("abc123".to_string()));
        assert_eq!(project.paths, vec!["src", "tests"]);
        assert_eq!(project.deps, vec!["requests", "pytest"]);
        assert_eq!(project.max_dep_date, "2024-01-01");
        assert_eq!(project.cost.get("mypy"), Some(&100));
    }
}
