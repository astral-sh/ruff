use std::process::Command;

use insta_cmd::assert_cmd_snapshot;

use crate::CliTest;

#[test]
fn type_checking_imports() -> anyhow::Result<()> {
    let test = AnalyzeTest::with_files([
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

    assert_cmd_snapshot!(test.command(), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "ruff/__init__.py": [],
      "ruff/a.py": [
        "ruff/b.py",
        "ruff/c.py"
      ],
      "ruff/b.py": [
        "ruff/c.py"
      ],
      "ruff/c.py": []
    }

    ----- stderr -----
    "#);

    assert_cmd_snapshot!(
        test.command()
            .arg("--no-type-checking-imports"),
        @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "ruff/__init__.py": [],
      "ruff/a.py": [
        "ruff/b.py"
      ],
      "ruff/b.py": [],
      "ruff/c.py": []
    }

    ----- stderr -----
    "#
    );

    Ok(())
}

#[test]
fn type_checking_imports_from_config() -> anyhow::Result<()> {
    let test = AnalyzeTest::with_files([
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

    assert_cmd_snapshot!(test.command(), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "ruff/__init__.py": [],
      "ruff/a.py": [
        "ruff/b.py"
      ],
      "ruff/b.py": [],
      "ruff/c.py": []
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

    assert_cmd_snapshot!(test.command(), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "ruff/__init__.py": [],
      "ruff/a.py": [
        "ruff/b.py",
        "ruff/c.py"
      ],
      "ruff/b.py": [
        "ruff/c.py"
      ],
      "ruff/c.py": []
    }

    ----- stderr -----
    "#
    );

    Ok(())
}

/// Each file resolves imports against the `src` of its own configuration.
#[test]
fn nested_config_src() -> anyhow::Result<()> {
    let test = AnalyzeTest::with_files([
        ("app/ruff.toml", r#"src = ["src"]"#),
        ("app/src/app/__init__.py", ""),
        ("app/src/app/main.py", "import utils"),
        ("app/src/utils/__init__.py", ""),
        ("lib/ruff.toml", r#"src = ["src"]"#),
        ("lib/src/lib/__init__.py", "import utils"),
        ("lib/src/utils/__init__.py", ""),
    ])?;

    assert_cmd_snapshot!(test.command(), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "app/src/app/__init__.py": [],
      "app/src/app/main.py": [
        "app/src/utils/__init__.py"
      ],
      "app/src/utils/__init__.py": [],
      "lib/src/lib/__init__.py": [
        "lib/src/utils/__init__.py"
      ],
      "lib/src/utils/__init__.py": []
    }

    ----- stderr -----
    "#);

    Ok(())
}

/// A relative import resolves from a directory without an `__init__.py` inside a package.
#[test]
fn relative_import_in_namespace_directory() -> anyhow::Result<()> {
    let test = AnalyzeTest::with_files([
        ("ruff/__init__.py", ""),
        ("ruff/a.py", ""),
        ("ruff/sub/b.py", "from .. import a"),
    ])?;

    assert_cmd_snapshot!(test.command(), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "ruff/__init__.py": [],
      "ruff/a.py": [],
      "ruff/sub/b.py": [
        "ruff/a.py"
      ]
    }

    ----- stderr -----
    "#);

    Ok(())
}

/// A file resolves a name from the package roots that contain it before falling back to other
/// package roots.
#[test]
fn ancestor_package_root_first() -> anyhow::Result<()> {
    let test = AnalyzeTest::with_files([
        ("app/main.py", "import utils"),
        ("app/utils/__init__.py", ""),
        ("lib/helpers/__init__.py", "import utils\nimport shared"),
        ("lib/utils/__init__.py", ""),
        ("other/shared/__init__.py", ""),
    ])?;

    assert_cmd_snapshot!(test.command(), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    {
      "app/main.py": [
        "app/utils/__init__.py"
      ],
      "app/utils/__init__.py": [],
      "lib/helpers/__init__.py": [
        "lib/utils/__init__.py",
        "other/shared/__init__.py"
      ],
      "lib/utils/__init__.py": [],
      "other/shared/__init__.py": []
    }

    ----- stderr -----
    "#);

    Ok(())
}

struct AnalyzeTest {
    cli_test: CliTest,
}

impl AnalyzeTest {
    pub(crate) fn new() -> anyhow::Result<Self> {
        Ok(Self {
            cli_test: CliTest::with_settings(|_, mut settings| {
                settings.add_filter(r#"\\\\"#, "/");
                settings
            })?,
        })
    }

    fn with_files<'a>(files: impl IntoIterator<Item = (&'a str, &'a str)>) -> anyhow::Result<Self> {
        let case = Self::new()?;
        case.write_files(files)?;
        Ok(case)
    }

    #[expect(unused)]
    fn with_file(path: impl AsRef<std::path::Path>, content: &str) -> anyhow::Result<Self> {
        let fixture = Self::new()?;
        fixture.write_file(path, content)?;
        Ok(fixture)
    }

    fn command(&self) -> Command {
        let mut command = self.cli_test.command();
        command.arg("analyze").arg("graph").arg("--preview");
        command
    }
}

impl std::ops::Deref for AnalyzeTest {
    type Target = CliTest;

    fn deref(&self) -> &Self::Target {
        &self.cli_test
    }
}
