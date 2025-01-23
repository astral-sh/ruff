use anyhow::Context;
use insta::Settings;
use insta_cmd::{assert_cmd_snapshot, get_cargo_bin};
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

/// Specifying an option on the CLI should take precedence over the same setting in the
/// project's configuration.
#[test]
fn config_override() -> anyhow::Result<()> {
    let case = TestCase::with_files([
        (
            "pyproject.toml",
            r#"
            [tool.knot.environment]
            python-version = "3.11"
            "#,
        ),
        (
            "test.py",
            r#"
            import sys

            # Access `sys.last_exc` that was only added in Python 3.12
            print(sys.last_exc)
            "#,
        ),
    ])?;

    case.insta_settings().bind(|| {
        assert_cmd_snapshot!(case.command(), @r"
            success: false
            exit_code: 1
            ----- stdout -----
            error[lint:unresolved-attribute] <temp_dir>/test.py:5:7 Type `<module 'sys'>` has no attribute `last_exc`

            ----- stderr -----
        ");

        assert_cmd_snapshot!(case.command().arg("--python-version").arg("3.12"), @r"
            success: true
            exit_code: 0
            ----- stdout -----

            ----- stderr -----
        ");
    });

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
    let case = TestCase::with_files([
        (
            "pyproject.toml",
            r#"
            [tool.knot.environment]
            python-version = "3.11"
            "#,
        ),
        (
            "libs/utils.py",
            r#"
            def add(a: int, b: int) -> int:
                a + b
            "#,
        ),
        (
            "child/test.py",
            r#"
            from utils import add

            stat = add(10, 15)
            "#,
        ),
    ])?;

    case.insta_settings().bind(|| {
        // Make sure that the CLI fails when the `libs` directory is not in the search path.
        assert_cmd_snapshot!(case.command().current_dir(case.project_dir().join("child")), @r#"
            success: false
            exit_code: 1
            ----- stdout -----
            error[lint:unresolved-import] <temp_dir>/child/test.py:2:1 Cannot resolve import `utils`

            ----- stderr -----
        "#);

        assert_cmd_snapshot!(case.command().current_dir(case.project_dir().join("child")).arg("--extra-search-path").arg("../libs"), @r"
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
    let case = TestCase::with_files([
        (
            "pyproject.toml",
            r#"
            [tool.knot.environment]
            python-version = "3.11"
            extra-paths = ["libs"]
            "#,
        ),
        (
            "libs/utils.py",
            r#"
            def add(a: int, b: int) -> int:
                a + b
            "#,
        ),
        (
            "child/test.py",
            r#"
            from utils import add

            stat = add(10, 15)
            "#,
        ),
    ])?;

    case.insta_settings().bind(|| {
        assert_cmd_snapshot!(case.command().current_dir(case.project_dir().join("child")), @r"
            success: true
            exit_code: 0
            ----- stdout -----

            ----- stderr -----
        ");
    });

    Ok(())
}

/// The rule severity can be changed in the configuration file
#[test]
fn rule_severity() -> anyhow::Result<()> {
    let case = TestCase::with_file(
        "test.py",
        r#"
            y = 4 / 0

            for a in range(0, y):
                x = a

            print(x)  # possibly-unresolved-reference
            "#,
    )?;

    case.insta_settings().bind(|| {
        // Assert that there's a possibly unresolved reference diagnostic
        // and that division-by-zero has a severity of error by default.
        assert_cmd_snapshot!(case.command(), @r"
            success: false
            exit_code: 1
            ----- stdout -----
            error[lint:division-by-zero] <temp_dir>/test.py:2:5 Cannot divide object of type `Literal[4]` by zero
            warning[lint:possibly-unresolved-reference] <temp_dir>/test.py:7:7 Name `x` used when possibly not defined

            ----- stderr -----
        ");

        case.write_file("pyproject.toml", r#"
            [tool.knot.rules]
            division-by-zero = "warn" # demote to warn
            possibly-unresolved-reference = "ignore"
        "#)?;

        assert_cmd_snapshot!(case.command(), @r"
            success: false
            exit_code: 1
            ----- stdout -----
            warning[lint:division-by-zero] <temp_dir>/test.py:2:5 Cannot divide object of type `Literal[4]` by zero

            ----- stderr -----
        ");

        Ok(())
    })
}

/// Red Knot warns about unknown rules
#[test]
fn unknown_rules() -> anyhow::Result<()> {
    let case = TestCase::with_files([
        (
            "pyproject.toml",
            r#"
            [tool.knot.rules]
            division-by-zer = "warn" # incorrect rule name
            "#,
        ),
        ("test.py", "print(10)"),
    ])?;

    case.insta_settings().bind(|| {
        assert_cmd_snapshot!(case.command(), @r"
            success: false
            exit_code: 1
            ----- stdout -----
            warning[unknown-rule] <temp_dir>/pyproject.toml:3:1 Unknown lint rule `division-by-zer`

            ----- stderr -----
        ");
    });

    Ok(())
}

struct TestCase {
    _temp_dir: TempDir,
    project_dir: PathBuf,
}

impl TestCase {
    fn new() -> anyhow::Result<Self> {
        let temp_dir = TempDir::new()?;

        // Canonicalize the tempdir path because macos uses symlinks for tempdirs
        // and that doesn't play well with our snapshot filtering.
        let project_dir = temp_dir
            .path()
            .canonicalize()
            .context("Failed to canonicalize project path")?;

        Ok(Self {
            project_dir,
            _temp_dir: temp_dir,
        })
    }

    fn with_files<'a>(files: impl IntoIterator<Item = (&'a str, &'a str)>) -> anyhow::Result<Self> {
        let case = Self::new()?;
        case.write_files(files)?;
        Ok(case)
    }

    fn with_file(path: impl AsRef<Path>, content: &str) -> anyhow::Result<Self> {
        let case = Self::new()?;
        case.write_file(path, content)?;
        Ok(case)
    }

    fn write_files<'a>(
        &self,
        files: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> anyhow::Result<()> {
        for (path, content) in files {
            self.write_file(path, content)?;
        }

        Ok(())
    }

    fn write_file(&self, path: impl AsRef<Path>, content: &str) -> anyhow::Result<()> {
        let path = path.as_ref();
        let path = self.project_dir.join(path);

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory `{}`", parent.display()))?;
        }
        std::fs::write(&path, &*ruff_python_trivia::textwrap::dedent(content))
            .with_context(|| format!("Failed to write file `{path}`", path = path.display()))?;

        Ok(())
    }

    fn project_dir(&self) -> &Path {
        &self.project_dir
    }

    // Returns the insta filters to escape paths in snapshots
    fn insta_settings(&self) -> Settings {
        let mut settings = insta::Settings::clone_current();
        settings.add_filter(&tempdir_filter(&self.project_dir), "<temp_dir>/");
        settings.add_filter(r#"\\(\w\w|\s|\.|")"#, "/$1");
        settings
    }

    fn command(&self) -> Command {
        let mut command = Command::new(get_cargo_bin("red_knot"));
        command.current_dir(&self.project_dir);
        command
    }
}

fn tempdir_filter(path: &Path) -> String {
    format!(r"{}\\?/?", regex::escape(path.to_str().unwrap()))
}
