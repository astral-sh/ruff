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

#[test]
fn show_inline_script_settings() -> anyhow::Result<()> {
    let test = CliTest::with_settings(|_, mut settings| {
        settings.add_filter(
            r"(?ms)^# General Settings.*?^(linter\.line_length = \d+)$.*?(\s+----- stderr -----)",
            "$1$2",
        );
        settings
    })?;
    test.write_file(
        "script.py",
        r#"
            # /// script
            # [tool.ruff]
            # line-length = 100
            # ///
        "#,
    )?;

    assert_cmd_snapshot!(test.command().args(["check", "--preview", "--show-settings", "script.py"]), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    Resolved settings for: "[TMP]/script.py"
    Settings path: "[TMP]/script.py"

    linter.line_length = 100

    ----- stderr -----
    "#);

    Ok(())
}

#[test]
fn show_settings_invalid_script_metadata() -> anyhow::Result<()> {
    let test = CliTest::with_file(
        "script.py",
        r#"
            # /// script
            # requires-python = 42
            # ///
        "#,
    )?;

    assert_cmd_snapshot!(test.command().args(["check", "--preview", "--show-settings", "script.py"]), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    ruff failed
      Cause: invalid-script-metadata: invalid type: integer `42`, expected a string
     --> script.py:3:21
      |
    2 | # /// script
    3 | # requires-python = 42
      |                     ^^
    4 | # ///
      |
    ");

    Ok(())
}
