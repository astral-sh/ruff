#![allow(
    dead_code,
    reason = "Test utility methods used by different test modules"
)]

use anyhow::Context;
use insta::internals::SettingsBindDropGuard;
use insta_cmd::get_cargo_bin;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

pub(crate) struct CliTest {
    _temp_dir: TempDir,
    _settings_scope: SettingsBindDropGuard,
    project_dir: PathBuf,
}

impl CliTest {
    pub(crate) fn new() -> anyhow::Result<Self> {
        let temp_dir = TempDir::new()?;

        // Canonicalize the tempdir path because macos uses symlinks for tempdirs
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
        settings.add_filter(&tempdir_filter(&project_dir), "<temp_dir>/");
        settings.add_filter(r#"\\(\w\w|\s|\.|")"#, "/$1");

        let settings_scope = settings.bind_to_scope();

        Ok(Self {
            project_dir,
            _temp_dir: temp_dir,
            _settings_scope: settings_scope,
        })
    }

    pub(crate) fn with_files<'a>(
        files: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> anyhow::Result<Self> {
        let case = Self::new()?;
        case.write_files(files)?;
        Ok(case)
    }

    pub(crate) fn with_file(path: impl AsRef<Path>, content: &str) -> anyhow::Result<Self> {
        let case = Self::new()?;
        case.write_file(path, content)?;
        Ok(case)
    }

    pub(crate) fn write_files<'a>(
        &self,
        files: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> anyhow::Result<()> {
        for (path, content) in files {
            self.write_file(path, content)?;
        }

        Ok(())
    }

    pub(crate) fn write_file(&self, path: impl AsRef<Path>, content: &str) -> anyhow::Result<()> {
        let path = path.as_ref();
        let path = self.project_dir.join(path);

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory `{}`", parent.display()))?;
        }
        std::fs::write(&path, &*ruff_python_trivia::textwrap::dedent(content))
            .with_context(|| format!("Failed to write file `{path}`", path = path.display()))?;

        Ok(())
    }

    pub(crate) fn root(&self) -> &Path {
        &self.project_dir
    }

    pub(crate) fn command(&self) -> Command {
        let mut command = Command::new(get_cargo_bin("ty"));
        command.current_dir(&self.project_dir).arg("check");

        // Unset environment variables that can affect test behavior
        command.env_remove("VIRTUAL_ENV");
        command.env_remove("CONDA_PREFIX");

        command
    }
}

fn tempdir_filter(path: &Path) -> String {
    format!(r"{}\\?/?", regex::escape(path.to_str().unwrap()))
}
