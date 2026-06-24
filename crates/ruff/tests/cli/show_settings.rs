use insta_cmd::assert_cmd_snapshot;

use crate::CliTest;

#[test]
fn display_default_settings() -> anyhow::Result<()> {
    let test = CliTest::with_files([
        (
            "pyproject.toml",
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
        ),
        ("test.py", r#"print("Hello")"#),
    ])?;

    assert_cmd_snapshot!(test.command().args(["check", "--show-settings", "test.py"]));

    Ok(())
}

#[test]
fn display_settings_from_nested_directory() -> anyhow::Result<()> {
    let test = CliTest::with_files([
        (
            "pyproject.toml",
            r#"
            [tool.ruff]
            line-length = 100

            [tool.ruff.lint]
            select = ["E", "F"]
        "#,
        ),
        (
            "subdir/pyproject.toml",
            r#"
            [tool.ruff]
            line-length = 120

            [tool.ruff.lint]
            select = ["E", "F", "I"]
        "#,
        ),
        ("subdir/test.py", r#"import os"#),
    ])?;

    assert_cmd_snapshot!(
        test.command()
            .args(["check", "--show-settings", "subdir/test.py"])
    );

    Ok(())
}
