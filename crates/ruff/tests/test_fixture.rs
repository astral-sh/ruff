//! Test fixture utilities for ruff CLI tests

use anyhow::{Context as _, Result};
use insta::internals::SettingsBindDropGuard;
use insta_cmd::get_cargo_bin;
use regex::escape;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use tempfile::TempDir;

const BIN_NAME: &str = "ruff";

/// Creates a regex filter for replacing temporary directory paths in snapshots
pub(crate) fn tempdir_filter(path: impl AsRef<Path>) -> String {
    format!(r"{}\\?/?", escape(path.as_ref().to_str().unwrap()))
}

/// A test fixture for running ruff CLI tests with temporary directories and files.
///
/// This fixture provides:
/// - Temporary directory management
/// - File creation utilities
/// - Proper snapshot filtering for cross-platform compatibility
/// - Pre-configured ruff command creation
///
/// # Example
///
/// ```rust,no_run
/// use crate::common::RuffTestFixture;
///
/// let fixture = RuffTestFixture::with_file("ruff.toml", "select = ['E']")?;
/// let output = fixture.command().args(["check", "."]).output()?;
/// ```
pub(crate) struct RuffTestFixture {
    _temp_dir: TempDir,
    _settings_scope: SettingsBindDropGuard,
    project_dir: PathBuf,
}

impl RuffTestFixture {
    /// Creates a new test fixture with an empty temporary directory.
    ///
    /// This sets up:
    /// - A temporary directory that's automatically cleaned up
    /// - Insta snapshot filters for cross-platform path compatibility
    /// - Environment isolation for consistent test behavior
    pub(crate) fn new() -> Result<Self> {
        let temp_dir = TempDir::new()?;

        // Canonicalize the tempdir path because macOS uses symlinks for tempdirs
        // and that doesn't play well with our snapshot filtering.
        let project_dir = temp_dir
            .path()
            .canonicalize()
            .context("Failed to canonicalize project path")?;

        let mut settings = insta::Settings::clone_current();
        settings.add_filter(&tempdir_filter(&project_dir), "[TMP]/");
        settings.add_filter(r#"\\(\w\w|\s|\.|")"#, "/$1");
        settings.add_filter(
            r#"The system cannot find the file specified."#,
            "No such file or directory",
        );

        let settings_scope = settings.bind_to_scope();

        Ok(Self {
            project_dir,
            _temp_dir: temp_dir,
            _settings_scope: settings_scope,
        })
    }

    /// Creates a test fixture with multiple files.
    ///
    /// # Arguments
    ///
    /// * `files` - An iterator of (path, content) pairs to create
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// let fixture = RuffTestFixture::with_files([
    ///     ("ruff.toml", "select = ['E']"),
    ///     ("src/main.py", "print('hello')"),
    /// ])?;
    /// ```
    pub(crate) fn with_files<'a>(
        files: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> Result<Self> {
        let fixture = Self::new()?;
        fixture.write_files(files)?;
        Ok(fixture)
    }

    /// Creates a test fixture with a single file.
    ///
    /// # Arguments
    ///
    /// * `path` - The relative path for the file
    /// * `content` - The content to write to the file
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// let fixture = RuffTestFixture::with_file("ruff.toml", "select = ['E']")?;
    /// ```
    pub(crate) fn with_file(path: impl AsRef<Path>, content: &str) -> Result<Self> {
        let fixture = Self::new()?;
        fixture.write_file(path, content)?;
        Ok(fixture)
    }

    /// Writes multiple files to the test directory.
    ///
    /// Parent directories are created automatically if they don't exist.
    ///
    /// # Arguments
    ///
    /// * `files` - An iterator of (path, content) pairs to create
    pub(crate) fn write_files<'a>(
        &self,
        files: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> Result<()> {
        for (path, content) in files {
            self.write_file(path, content)?;
        }

        Ok(())
    }

    /// Ensures that the parent directory of a path exists.
    fn ensure_parent_directory(path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory `{}`", parent.display()))?;
        }
        Ok(())
    }

    /// Writes a file to the test directory.
    ///
    /// Parent directories are created automatically if they don't exist.
    /// Leading newlines are trimmed from the content for cleaner test code.
    ///
    /// # Arguments
    ///
    /// * `path` - The relative path for the file
    /// * `content` - The content to write to the file
    pub(crate) fn write_file(&self, path: impl AsRef<Path>, content: &str) -> Result<()> {
        let path = path.as_ref();
        let file_path = self.project_dir.join(path);

        Self::ensure_parent_directory(&file_path)?;

        let content = content.trim_start_matches('\n');
        fs::write(&file_path, content)
            .with_context(|| format!("Failed to write file `{}`", file_path.display()))?;

        Ok(())
    }

    /// Creates a symbolic link in the test directory (Unix only).
    ///
    /// # Arguments
    ///
    /// * `original` - The path to the original file/directory (relative to test root)
    /// * `link` - The path for the symbolic link (relative to test root)
    #[cfg(unix)]
    pub(crate) fn write_symlink(
        &self,
        original: impl AsRef<Path>,
        link: impl AsRef<Path>,
    ) -> Result<()> {
        let link_path = self.project_dir.join(link.as_ref());
        let original_path = self.project_dir.join(original.as_ref());

        Self::ensure_parent_directory(&link_path)?;

        std::os::unix::fs::symlink(original_path, &link_path)
            .with_context(|| format!("Failed to write symlink `{}`", link_path.display()))?;

        Ok(())
    }

    /// Returns the path to the test directory root.
    pub(crate) fn root(&self) -> &Path {
        &self.project_dir
    }

    /// Creates a pre-configured ruff command for testing.
    ///
    /// The command is set up with:
    /// - The correct ruff binary path
    /// - Working directory set to the test directory
    /// - Clean environment variables for consistent behavior
    ///
    /// You can chain additional arguments and options as needed.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// let output = fixture
    ///     .command()
    ///     .args(["check", "--select", "E"])
    ///     .arg(".")
    ///     .output()?;
    /// ```
    pub(crate) fn command(&self) -> Command {
        let mut command = Command::new(get_cargo_bin(BIN_NAME));
        command.current_dir(&self.project_dir);

        // Unset all environment variables because they can affect test behavior.
        command.env_clear();

        command
    }
}
