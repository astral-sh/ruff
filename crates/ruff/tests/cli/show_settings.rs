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
fn script_requires_python_show_settings() -> anyhow::Result<()> {
    let test = CliTest::with_files([
        ("ruff.toml", "target-version = 'py39'"),
        (
            "script.py",
            "# /// script\n# requires-python = '>=3.12'\n# ///\n",
        ),
    ])?;
    let output = test
        .command()
        .args(["check", "--show-settings", "script.py"])
        .output()?;
    assert!(output.status.success());
    let settings = String::from_utf8(output.stdout)?;
    assert!(settings.contains("linter.unresolved_target_version = 3.12"));
    assert!(settings.contains("formatter.unresolved_target_version = 3.12"));
    Ok(())
}

#[test]
fn inline_script_target_version() -> anyhow::Result<()> {
    let test = CliTest::with_file("ruff.toml", "target-version = 'py39'")?;
    for (metadata, expected) in [
        ("dependencies = []", "3.11"),
        ("[tool.ruff]", "3.11"),
        ("requires-python = '>=3.12'\n# [tool.ruff]", "3.12"),
        (
            "requires-python = '>=3.12'\n# tool.ruff.target-version = 'py310'",
            "3.10",
        ),
        (
            "requires-python = '>=3.12'\n# tool.ruff.extend = 'ruff.toml'",
            "3.12",
        ),
    ] {
        test.write_file("script.py", &format!("# /// script\n# {metadata}\n# ///\n"))?;
        let output = test
            .command()
            .args(["check", "--show-settings", "script.py"])
            .output()?;
        assert!(output.status.success());
        let settings = String::from_utf8(output.stdout)?;
        assert!(settings.contains(&format!(
            "Settings path: \"{}\"",
            test.root().join("script.py").display()
        )));
        assert!(
            settings.contains(&format!("formatter.unresolved_target_version = {expected}")),
            "{metadata}"
        );
    }
    Ok(())
}
