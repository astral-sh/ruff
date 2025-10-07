//! Test fixture utilities for ruff CLI tests
//!
//! The core concept is borrowed from ty/tests/cli/main.rs and can be extended
//! with more functionality from there in the future if needed.

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
    // Path is already normalized by dunce::simplified() in new(), so no need to call it again
    let path_str = path.as_ref().to_str().unwrap();

    // Escape the path for regex
    let escaped_path = escape(path_str);
    // Replace literal backslashes in the escaped pattern with a character class that matches both
    let cross_platform_pattern = escaped_path.replace(r"\\", r"[\\/]");

    // Always add optional UNC prefix to match both \\?\C:\... and C:\... patterns
    // This ensures compatibility regardless of whether input or output contains UNC prefix
    format!(r"(?:[\\/][\\/]\?[\\/])?{cross_platform_pattern}[\\/]?")
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
        // Simplify with dunce because otherwise we get UNC paths on Windows.
        let project_dir = dunce::simplified(
            &temp_dir
                .path()
                .canonicalize()
                .context("Failed to canonicalize project path")?,
        )
        .to_path_buf();

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
    /// Content is dedented to remove common leading whitespace for cleaner test code.
    ///
    /// # Arguments
    ///
    /// * `path` - The relative path for the file
    /// * `content` - The content to write to the file
    pub(crate) fn write_file(&self, path: impl AsRef<Path>, content: &str) -> Result<()> {
        let path = path.as_ref();
        let file_path = self.project_dir.join(path);

        Self::ensure_parent_directory(&file_path)?;

        let content = ruff_python_trivia::textwrap::dedent(content);
        fs::write(&file_path, content.as_ref())
            .with_context(|| format!("Failed to write file `{}`", file_path.display()))?;

        Ok(())
    }

    /// Returns the path to the test directory root.
    pub(crate) fn root(&self) -> &Path {
        &self.project_dir
    }

    /// Returns a regex filter pattern for replacing temporary directory paths in snapshots.
    ///
    /// This provides the tempdir filter pattern that can be used in
    /// `insta::with_settings!` filters array for tests that need custom filtering
    /// beyond what the global fixture filters provide.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// insta::with_settings!({
    ///     filters => vec![
    ///         (fixture.tempdir_filter().as_str(), "[TMP]/"),
    ///         (r"some_other_pattern", "replacement")
    ///     ]
    /// }, {
    ///     assert_cmd_snapshot!(fixture.command().args(["check", "."]));
    /// });
    /// ```
    pub(crate) fn tempdir_filter(&self) -> String {
        tempdir_filter(self.root())
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_tempdir_filter() {
        let path = "/tmp/test/path";
        let filter = tempdir_filter(path);
        assert!(filter.contains("test"));
        assert!(filter.contains("path"));
    }

    #[test]
    fn test_fixture_new() -> Result<()> {
        let fixture = RuffTestFixture::new()?;
        assert!(fixture.root().exists());
        Ok(())
    }

    #[test]
    fn test_fixture_with_file() -> Result<()> {
        let fixture = RuffTestFixture::with_file("test.py", "print('hello')")?;
        let file_path = fixture.root().join("test.py");
        assert!(file_path.exists());
        let content = fs::read_to_string(file_path)?;
        assert_eq!(content, "print('hello')");
        Ok(())
    }

    #[test]
    fn test_write_file() -> Result<()> {
        let fixture = RuffTestFixture::new()?;
        fixture.write_file("nested/dir/file.py", "test content")?;
        let file_path = fixture.root().join("nested/dir/file.py");
        assert!(file_path.exists());
        let content = fs::read_to_string(file_path)?;
        assert_eq!(content, "test content");
        Ok(())
    }

    #[test]
    fn test_tempdir_filter_method() -> Result<()> {
        let fixture = RuffTestFixture::new()?;
        let filter = fixture.tempdir_filter();
        assert!(!filter.is_empty());
        Ok(())
    }

    #[test]
    fn test_command_creation() -> Result<()> {
        let fixture = RuffTestFixture::new()?;
        let command = fixture.command();
        assert_eq!(command.get_current_dir(), Some(fixture.root()));
        Ok(())
    }

    #[test]
    fn test_dedent_functionality() -> Result<()> {
        let fixture = RuffTestFixture::new()?;
        let indented_content = "
            def hello():
                print('hello')
                return True
        ";
        fixture.write_file("test_dedent.py", indented_content)?;
        let file_path = fixture.root().join("test_dedent.py");
        let content = fs::read_to_string(file_path)?;
        // Verify that the indentation was removed
        assert!(content.trim().starts_with("def hello():"));
        assert!(content.contains("    print('hello')")); // Still has relative indentation
        assert!(content.contains("    return True"));
        Ok(())
    }
}
