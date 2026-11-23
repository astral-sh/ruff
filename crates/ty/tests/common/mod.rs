//! Shared fixtures for ty CLI integration tests.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::Context as _;
use insta::Settings;
use insta::internals::SettingsBindDropGuard;
use insta_cmd::get_cargo_bin;
use tempfile::TempDir;

pub struct CliTest {
    _temp_dir: TempDir,
    settings: Settings,
    settings_scope: Option<SettingsBindDropGuard>,
    pub(crate) project_dir: PathBuf,
    pub(crate) ty_binary_path: PathBuf,
}

impl CliTest {
    pub fn new() -> anyhow::Result<Self> {
        let temp_dir = TempDir::new()?;

        // Canonicalize the tempdir path because macos uses symlinks for tempdirs
        // and that doesn't play well with our snapshot filtering.
        // Simplify with dunce because otherwise we get UNC paths on Windows.
        let temp_dir_path = dunce::simplified(
            &temp_dir
                .path()
                .canonicalize()
                .context("Failed to canonicalize temporary directory path")?,
        )
        .to_path_buf();
        let project_dir = temp_dir_path.join("project");
        std::fs::create_dir_all(&project_dir)
            .with_context(|| format!("Failed to create directory `{}`", project_dir.display()))?;

        let mut settings = insta::Settings::clone_current();
        settings.add_filter(&tempdir_filter(&project_dir), "<temp_dir>/");
        settings.add_filter(r"\bty\.exe\b", "ty");
        settings.add_filter(r#"\\(\w\w|\s|\.|")"#, "/$1");
        // 0.003s
        settings.add_filter(r"\d.\d\d\ds", "0.000s");
        settings.add_filter(
            "INFO Checking file `[^`]+` took more than 100ms \\([^)]+\\)\n",
            "",
        );
        settings.add_filter("INFO Defaulting to python-platform `[^`]+`\n", "");
        settings.add_filter("INFO Python version: [^,]+, platform: [a-z0-9_]+\n", "");
        settings.add_filter(
            r#"The system cannot find the file specified."#,
            "No such file or directory",
        );

        let settings_scope = settings.bind_to_scope();

        Ok(Self {
            project_dir,
            _temp_dir: temp_dir,
            settings,
            settings_scope: Some(settings_scope),
            ty_binary_path: get_cargo_bin("ty"),
        })
    }

    pub fn with_files<'a>(
        files: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> anyhow::Result<Self> {
        let case = Self::new()?;
        case.write_files(files)?;
        Ok(case)
    }

    pub fn with_file(path: impl AsRef<Path>, content: &str) -> anyhow::Result<Self> {
        let case = Self::new()?;
        case.write_file(path, content)?;
        Ok(case)
    }

    pub fn write_files<'a>(
        &self,
        files: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> anyhow::Result<()> {
        for (path, content) in files {
            self.write_file(path, content)?;
        }

        Ok(())
    }

    /// Add a filter to the settings and rebind them.
    #[must_use]
    pub fn with_filter(mut self, pattern: &str, replacement: &str) -> Self {
        self.settings.add_filter(pattern, replacement);
        // Drop the old scope before binding a new one, otherwise the old scope is dropped _after_
        // binding and assigning the new one, restoring the settings to their state before the old
        // scope was bound.
        drop(self.settings_scope.take());
        self.settings_scope = Some(self.settings.bind_to_scope());
        self
    }

    pub(crate) fn ensure_parent_directory(path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory `{}`", parent.display()))?;
        }
        Ok(())
    }

    pub fn write_file(&self, path: impl AsRef<Path>, content: &str) -> anyhow::Result<()> {
        let path = path.as_ref();
        let path = self.project_dir.join(path);

        Self::ensure_parent_directory(&path)?;

        std::fs::write(&path, &*ruff_python_trivia::textwrap::dedent(content))
            .with_context(|| format!("Failed to write file `{path}`", path = path.display()))?;

        Ok(())
    }

    #[cfg(unix)]
    pub fn write_symlink(
        &self,
        original: impl AsRef<Path>,
        link: impl AsRef<Path>,
    ) -> anyhow::Result<()> {
        let link = link.as_ref();
        let link = self.project_dir.join(link);

        let original = original.as_ref();
        let original = self.project_dir.join(original);

        Self::ensure_parent_directory(&link)?;

        std::os::unix::fs::symlink(original, &link)
            .with_context(|| format!("Failed to write symlink `{link}`", link = link.display()))?;

        Ok(())
    }

    pub fn root(&self) -> &Path {
        &self.project_dir
    }

    pub fn command(&self) -> Command {
        self.command_with_subcommand("check")
    }

    pub fn command_with_subcommand(&self, subcommand: &str) -> Command {
        let mut command = Command::new(&self.ty_binary_path);
        command.current_dir(&self.project_dir).arg(subcommand);

        // Unset all environment variables because they can affect test behavior.
        command.env_clear();
        // Point user config discovery at a test-local directory to avoid picking up host config.
        command.env(
            user_config_directory_env_var(),
            self.user_config_directory(),
        );

        command
    }

    pub fn user_config_directory(&self) -> PathBuf {
        self.project_dir
            .parent()
            .expect("project directory always has a parent")
            .join("home/.config")
    }
}

fn tempdir_filter(path: &Path) -> String {
    format!(r"{}\\?/?", regex::escape(path.to_str().unwrap()))
}

pub(crate) fn user_config_directory_env_var() -> &'static str {
    if cfg!(windows) {
        "APPDATA"
    } else {
        "XDG_CONFIG_HOME"
    }
}
