use anyhow::Context;
use insta_cmd::{assert_cmd_snapshot, get_cargo_bin};
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

/// Specifying an option on the CLI should take precedence over the same setting in the
/// project's configuration.
#[test]
fn config_override() -> anyhow::Result<()> {
    let tempdir = TempDir::new()?;

    std::fs::write(
        tempdir.path().join("pyproject.toml"),
        r#"
[tool.knot.environment]
python-version = "3.11"
"#,
    )
    .context("Failed to write settings")?;

    std::fs::write(
        tempdir.path().join("test.py"),
        r#"
import sys

# Access `sys.last_exc` that was only added in Python 3.12
print(sys.last_exc)
"#,
    )
    .context("Failed to write test.py")?;

    insta::with_settings!({filters => vec![(&*tempdir_filter(tempdir.path()), "<temp_dir>/")]}, {
        assert_cmd_snapshot!(knot().arg("--project").arg(tempdir.path()), @r"
        success: false
        exit_code: 1
        ----- stdout -----
        error[lint:unresolved-attribute] <temp_dir>/test.py:5:7 Type `<module 'sys'>` has no attribute `last_exc`

        ----- stderr -----
        ");
    });

    assert_cmd_snapshot!(knot().arg("--project").arg(tempdir.path()).arg("--python-version").arg("3.12"), @r"
    success: true
    exit_code: 0
    ----- stdout -----

    ----- stderr -----
    ");

    Ok(())
}

/// Paths specified on the CLI are relative to the current working directory and not the project root.
///
/// We test this by adding an extra search path from the CLI to the libs directory when
/// running the CLI from the child directory (using relative paths).
///
/// Project layout:
/// ```
///  - libs
///    |- utils.py
///  - child
///    | - test.py
/// - pyproject.toml
/// ```
///
/// And the command is run in the `child` directory.
#[test]
fn cli_arguments_are_relative_to_the_current_directory() -> anyhow::Result<()> {
    let tempdir = TempDir::new()?;

    let project_dir = tempdir.path().canonicalize()?;

    let libs = project_dir.join("libs");
    std::fs::create_dir_all(&libs).context("Failed to create `libs` directory")?;

    let child = project_dir.join("child");
    std::fs::create_dir(&child).context("Failed to create `child` directory")?;

    std::fs::write(
        tempdir.path().join("pyproject.toml"),
        r#"
[tool.knot.environment]
python-version = "3.11"
"#,
    )
    .context("Failed to write `pyproject.toml`")?;

    std::fs::write(
        libs.join("utils.py"),
        r#"
def add(a: int, b: int) -> int:
    a + b
"#,
    )
    .context("Failed to write `utils.py`")?;

    std::fs::write(
        child.join("test.py"),
        r#"
from utils import add

stat = add(10, 15)
"#,
    )
    .context("Failed to write `child/test.py`")?;

    let project_filter = tempdir_filter(&project_dir);
    let filters = vec![
        (&*project_filter, "<temp_dir>/"),
        (r#"\\(\w\w|\s|\.|")"#, "/$1"),
    ];

    // Make sure that the CLI fails when the `libs` directory is not in the search path.
    insta::with_settings!({filters => filters}, {
        assert_cmd_snapshot!(knot().current_dir(&child), @r#"
        success: false
        exit_code: 1
        ----- stdout -----
        error[lint:unresolved-import] <temp_dir>/child/test.py:2:1 Cannot resolve import `utils`

        ----- stderr -----
        "#);
    });

    insta::with_settings!({filters => vec![(&*tempdir_filter(&project_dir), "<temp_dir>/")]}, {
        assert_cmd_snapshot!(knot().current_dir(child).arg("--extra-search-path").arg("../libs"), @r"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        ");
    });

    Ok(())
}

/// Paths specified in a configuration file are relative to the project root.
///
/// We test this by adding `libs` (as a relative path) to the extra search path in the configuration and run
/// the CLI from a subdirectory.
///
/// Project layout:
/// ```
///  - libs
///    |- utils.py
///  - child
///    | - test.py
/// - pyproject.toml
/// ```
#[test]
fn paths_in_configuration_files_are_relative_to_the_project_root() -> anyhow::Result<()> {
    let tempdir = TempDir::new()?;

    let project_dir = tempdir.path();

    let libs = project_dir.join("libs");
    std::fs::create_dir_all(&libs).context("Failed to create `libs` directory")?;

    let child = project_dir.join("child");
    std::fs::create_dir(&child).context("Failed to create `child` directory")?;

    std::fs::write(
        tempdir.path().join("pyproject.toml"),
        r#"
[tool.knot.environment]
python-version = "3.11"
extra-paths = ["libs"]
"#,
    )
    .context("Failed to write `pyproject.toml`")?;

    std::fs::write(
        libs.join("utils.py"),
        r#"
def add(a: int, b: int) -> int:
    a + b
"#,
    )
    .context("Failed to write `utils.py`")?;

    std::fs::write(
        child.join("test.py"),
        r#"
from utils import add

stat = add(10, 15)
"#,
    )
    .context("Failed to write `child/test.py`")?;

    insta::with_settings!({filters => vec![(&*tempdir_filter(tempdir.path()), "<temp_dir>/")]}, {
        assert_cmd_snapshot!(knot().current_dir(child), @r"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
        ");
    });

    Ok(())
}

fn knot() -> Command {
    Command::new(get_cargo_bin("red_knot"))
}

fn tempdir_filter(path: &Path) -> String {
    format!(r"{}\\?/?", regex::escape(path.to_str().unwrap()))
}
