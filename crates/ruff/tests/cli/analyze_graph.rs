use std::process::Command;

use insta_cmd::assert_cmd_snapshot;
use serde_json::json;

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

#[test]
fn package_dependencies_and_dependents() -> anyhow::Result<()> {
    let test = AnalyzeTest::with_files([
        ("app/__init__.py", ""),
        ("app/one.py", "import lib.a"),
        ("app/two.py", "import lib.b"),
        ("lib/__init__.py", ""),
        ("lib/a.py", ""),
        ("lib/b.py", ""),
    ])?;

    assert_eq!(test.package_graph(&[])?, json!({"app": ["lib"], "lib": []}));
    assert_eq!(
        test.package_graph(&["--direction", "dependents"])?,
        json!({"app": [], "lib": ["app"]})
    );
    Ok(())
}

#[test]
fn package_dependencies_for_selected_files() -> anyhow::Result<()> {
    let test = AnalyzeTest::with_files([
        ("app/__init__.py", ""),
        ("app/one.py", "import lib.a"),
        ("app/two.py", "import app.one\nimport lib.sub.b"),
        ("lib/__init__.py", ""),
        ("lib/a.py", ""),
        ("lib/sub/__init__.py", ""),
        ("lib/sub/b.py", ""),
    ])?;

    assert_eq!(
        test.package_graph(&["app"])?,
        json!({"app": ["lib", std::path::Path::new("lib").join("sub")]})
    );
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

    fn package_graph(&self, args: &[&str]) -> anyhow::Result<serde_json::Value> {
        let output = self.command().arg("--package").args(args).output()?;
        anyhow::ensure!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(serde_json::from_slice(&output.stdout)?)
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
