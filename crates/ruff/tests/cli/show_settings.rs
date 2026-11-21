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
        (
            "ruff.toml",
            r#"
                target-version = 'py39'
                lint.preview = true
                format.preview = true
            "#,
        ),
        (
            "script.py",
            r#"
                # /// script
                # requires-python = '>=3.12'
                # ///
            "#,
        ),
    ])?;
    let mut command = test.command();
    command.args(["check", "--show-settings", "script.py"]);

    let output = command.output()?;
    assert!(output.status.success());
    let settings = str::from_utf8(&output.stdout)?;
    assert!(settings.contains("linter.unresolved_target_version = 3.9"));
    assert!(settings.contains("formatter.unresolved_target_version = 3.9"));

    let output = command.arg("--preview").output()?;
    assert!(output.status.success());
    let settings = str::from_utf8(&output.stdout)?;
    assert!(settings.contains("linter.unresolved_target_version = 3.12"));
    assert!(settings.contains("formatter.unresolved_target_version = 3.12"));
    Ok(())
}

#[test]
fn inline_script_target_version() -> anyhow::Result<()> {
    let test = CliTest::with_file(
        "ruff.toml",
        r#"
            preview = true
            target-version = 'py39'
        "#,
    )?;
    let assert_version = |script: &str, expected: &str| -> anyhow::Result<()> {
        test.write_file("script.py", script)?;
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
            "{script}"
        );
        Ok(())
    };

    assert_version(
        r#"
            # /// script
            # [tool.ruff]
            # ///
        "#,
        "3.11",
    )?;
    assert_version(
        r#"
            # /// script
            # requires-python = '>=3.12'
            # [tool.ruff]
            # ///
        "#,
        "3.12",
    )?;
    assert_version(
        r#"
            # /// script
            # requires-python = '>=3.12'
            # tool.ruff.target-version = 'py310'
            # ///
        "#,
        "3.10",
    )?;
    assert_version(
        r#"
            # /// script
            # requires-python = '>=3.12'
            # tool.ruff.extend = 'ruff.toml'
            # ///
        "#,
        "3.12",
    )?;
    Ok(())
}
