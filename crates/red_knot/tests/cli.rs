use anyhow::Context;
use insta_cmd::{assert_cmd_snapshot, get_cargo_bin};
use std::process::Command;
use tempfile::TempDir;

/// Specifying an option on the CLI should take precedence over the same setting in the
/// project's configuration.
#[test]
fn test_config_override() -> anyhow::Result<()> {
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

    insta::with_settings!({filters => vec![(&*tempdir_filter(&tempdir), "<temp_dir>/")]}, {
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

fn knot() -> Command {
    Command::new(get_cargo_bin("red_knot"))
}

fn tempdir_filter(tempdir: &TempDir) -> String {
    format!(r"{}\\?/?", regex::escape(tempdir.path().to_str().unwrap()))
}
