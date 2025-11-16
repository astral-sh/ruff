use insta_cmd::assert_cmd_snapshot;

use crate::CliTest;

#[test]
fn type_checking_imports() -> anyhow::Result<()> {
    let test = CliTest::with_files([
        ("ruff/__init__.py", ""),
        (
            "ruff/a.py",
            r#"
            from typing import TYPE_CHECKING

            import ruff.b

            if TYPE_CHECKING:
                import ruff.c
        "#,
        ),
        (
            "ruff/b.py",
            r#"
            if TYPE_CHECKING:
                from ruff import c
            "#,
        ),
        ("ruff/c.py", ""),
    ])?;

    assert_cmd_snapshot!(test.analyze_graph_command(), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "ruff\/__init__.py": [],
      "ruff\\a.py": [
        "ruff\\b.py",
        "ruff\\c.py"
      ],
      "ruff\\b.py": [
        "ruff\\c.py"
      ],
      "ruff\\c.py": []
    }

    ----- stderr -----
    "#);

    assert_cmd_snapshot!(
        test.analyze_graph_command()
            .arg("--no-type-checking-imports"),
        @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "ruff\/__init__.py": [],
      "ruff\\a.py": [
        "ruff\\b.py"
      ],
      "ruff\\b.py": [],
      "ruff\\c.py": []
    }

    ----- stderr -----
    "#
    );

    Ok(())
}

#[test]
fn type_checking_imports_from_config() -> anyhow::Result<()> {
    let test = CliTest::with_files([
        ("ruff/__init__.py", ""),
        (
            "ruff/a.py",
            r#"
            from typing import TYPE_CHECKING

            import ruff.b

            if TYPE_CHECKING:
                import ruff.c
        "#,
        ),
        (
            "ruff/b.py",
            r#"
            if TYPE_CHECKING:
                from ruff import c
            "#,
        ),
        ("ruff/c.py", ""),
        (
            "ruff.toml",
            r#"
            [analyze]
            type-checking-imports = false
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(test.analyze_graph_command(), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "ruff\/__init__.py": [],
      "ruff\\a.py": [
        "ruff\\b.py"
      ],
      "ruff\\b.py": [],
      "ruff\\c.py": []
    }

    ----- stderr -----
    "#);

    test.write_file(
        "ruff.toml",
        r#"
        [analyze]
        type-checking-imports = true
        "#,
    )?;

    assert_cmd_snapshot!(test.analyze_graph_command(), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "ruff\/__init__.py": [],
      "ruff\\a.py": [
        "ruff\\b.py",
        "ruff\\c.py"
      ],
      "ruff\\b.py": [
        "ruff\\c.py"
      ],
      "ruff\\c.py": []
    }

    ----- stderr -----
    "#
    );

    Ok(())
}

impl CliTest {
    fn analyze_graph_command(&self) -> std::process::Command {
        let mut command = self.command();
        command.arg("analyze").arg("graph").arg("--preview");
        command
    }
}
