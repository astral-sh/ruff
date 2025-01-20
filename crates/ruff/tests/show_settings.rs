use anyhow::Context;
use insta_cmd::{assert_cmd_snapshot, get_cargo_bin};
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

const BIN_NAME: &str = "ruff";

#[test]
fn display_default_settings() -> anyhow::Result<()> {
    let tempdir = TempDir::new().context("Failed to create temp directory.")?;

    // Tempdir path's on macos are symlinks, which doesn't play nicely with
    // our snapshot filtering.
    let project_dir = tempdir
        .path()
        .canonicalize()
        .context("Failed to canonical tempdir path.")?;

    std::fs::write(
        project_dir.join("pyproject.toml"),
        r#"
[project]
name = "ruff"
version = "0.9.2"
requires-python = ">=3.7"

[tool.ruff]
line-length = 100

[tool.ruff.lint]
ignore = [
  # Conflicts with the formatter
  "COM812", "ISC001"
]

"#,
    )?;

    std::fs::write(project_dir.join("test.py"), r#"print("Hello")"#)
        .context("Failed to write test.py.")?;

    insta::with_settings!({filters => vec![
        (&*tempdir_filter(&project_dir), "<temp_dir>/"),
        (r#"\\(\w\w|\s|\.|")"#, "/$1"),
    ]}, {
        assert_cmd_snapshot!(Command::new(get_cargo_bin(BIN_NAME))
            .args(["check", "--show-settings", "test.py"])
            .current_dir(project_dir));
    });

    Ok(())
}

fn tempdir_filter(project_dir: &Path) -> String {
    format!(r#"{}\\?/?"#, regex::escape(project_dir.to_str().unwrap()))
}
