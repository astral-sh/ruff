mod config_option;
mod exit_code;
mod file_selection;
mod python_environment;
mod rule_selection;

use anyhow::Context as _;
use insta::internals::SettingsBindDropGuard;
use insta_cmd::{assert_cmd_snapshot, get_cargo_bin};
use std::{
    fmt::Write,
    path::{Path, PathBuf},
    process::Command,
};
use tempfile::TempDir;

#[test]
fn test_run_in_sub_directory() -> anyhow::Result<()> {
    let case = CliTest::with_files([("test.py", "~"), ("subdir/nothing", "")])?;
    assert_cmd_snapshot!(case.command().current_dir(case.root().join("subdir")).arg(".."), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    error[invalid-syntax]
     --> <temp_dir>/test.py:1:2
      |
    1 | ~
      |  ^ Expected an expression
      |

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");
    Ok(())
}

#[test]
fn test_include_hidden_files_by_default() -> anyhow::Result<()> {
    let case = CliTest::with_files([(".test.py", "~")])?;
    assert_cmd_snapshot!(case.command(), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    error[invalid-syntax]
     --> .test.py:1:2
      |
    1 | ~
      |  ^ Expected an expression
      |

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");
    Ok(())
}

#[test]
fn test_respect_ignore_files() -> anyhow::Result<()> {
    // First test that the default option works correctly (the file is skipped)
    let case = CliTest::with_files([(".ignore", "test.py"), ("test.py", "~")])?;
    assert_cmd_snapshot!(case.command(), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    WARN No python files found under the given path(s)
    ");

    // Test that we can set to false via CLI
    assert_cmd_snapshot!(case.command().arg("--no-respect-ignore-files"), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    error[invalid-syntax]
     --> test.py:1:2
      |
    1 | ~
      |  ^ Expected an expression
      |

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

    // Test that we can set to false via config file
    case.write_file("ty.toml", "src.respect-ignore-files = false")?;
    assert_cmd_snapshot!(case.command(), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    error[invalid-syntax]
     --> test.py:1:2
      |
    1 | ~
      |  ^ Expected an expression
      |

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

    // Ensure CLI takes precedence
    case.write_file("ty.toml", "src.respect-ignore-files = true")?;
    assert_cmd_snapshot!(case.command().arg("--no-respect-ignore-files"), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    error[invalid-syntax]
     --> test.py:1:2
      |
    1 | ~
      |  ^ Expected an expression
      |

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
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
    let case = CliTest::with_files([
        (
            "pyproject.toml",
            r#"
            [tool.ty.environment]
            python-version = "3.11"
            "#,
        ),
        (
            "libs/utils.py",
            r#"
            def add(a: int, b: int) -> int:
                return a + b
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

    // Make sure that the CLI fails when the `libs` directory is not in the search path.
    assert_cmd_snapshot!(case.command().current_dir(case.root().join("child")), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-import]: Cannot resolve imported module `utils`
     --> test.py:2:6
      |
    2 | from utils import add
      |      ^^^^^
    3 |
    4 | stat = add(10, 15)
      |
    info: make sure your Python environment is properly configured: https://github.com/astral-sh/ty/blob/main/docs/README.md#python-environment
    info: rule `unresolved-import` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

    assert_cmd_snapshot!(case.command().current_dir(case.root().join("child")).arg("--extra-search-path").arg("../libs"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

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
    let case = CliTest::with_files([
        (
            "pyproject.toml",
            r#"
            [tool.ty.environment]
            python-version = "3.11"
            extra-paths = ["libs"]
            "#,
        ),
        (
            "libs/utils.py",
            r#"
            def add(a: int, b: int) -> int:
                return a + b
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

    assert_cmd_snapshot!(case.command().current_dir(case.root().join("child")), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

    Ok(())
}

#[test]
fn user_configuration() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "project/ty.toml",
            r#"
            [rules]
            division-by-zero = "warn"
            "#,
        ),
        (
            "project/main.py",
            r#"
            y = 4 / 0

            for a in range(0, int(y)):
                x = a

            prin(x)
            "#,
        ),
    ])?;

    let config_directory = case.root().join("home/.config");
    let config_env_var = if cfg!(windows) {
        "APPDATA"
    } else {
        "XDG_CONFIG_HOME"
    };

    assert_cmd_snapshot!(
        case.command().current_dir(case.root().join("project")).env(config_env_var, config_directory.as_os_str()),
        @r"
    success: false
    exit_code: 1
    ----- stdout -----
    warning[division-by-zero]: Cannot divide object of type `Literal[4]` by zero
     --> main.py:2:5
      |
    2 | y = 4 / 0
      |     ^^^^^
    3 |
    4 | for a in range(0, int(y)):
      |
    info: rule `division-by-zero` was selected in the configuration file

    error[unresolved-reference]: Name `prin` used when not defined
     --> main.py:7:1
      |
    5 |     x = a
    6 |
    7 | prin(x)
      | ^^^^
      |
    info: rule `unresolved-reference` is enabled by default

    Found 2 diagnostics

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    "
    );

    // The user-level configuration sets the severity for `unresolved-reference` to warn.
    // Changing the level for `division-by-zero` has no effect, because the project-level configuration
    // has higher precedence.
    case.write_file(
        config_directory.join("ty/ty.toml"),
        r#"
        [rules]
        division-by-zero = "error"
        unresolved-reference = "warn"
        "#,
    )?;

    assert_cmd_snapshot!(
        case.command().current_dir(case.root().join("project")).env(config_env_var, config_directory.as_os_str()),
        @r"
    success: true
    exit_code: 0
    ----- stdout -----
    warning[division-by-zero]: Cannot divide object of type `Literal[4]` by zero
     --> main.py:2:5
      |
    2 | y = 4 / 0
      |     ^^^^^
    3 |
    4 | for a in range(0, int(y)):
      |
    info: rule `division-by-zero` was selected in the configuration file

    warning[unresolved-reference]: Name `prin` used when not defined
     --> main.py:7:1
      |
    5 |     x = a
    6 |
    7 | prin(x)
      | ^^^^
      |
    info: rule `unresolved-reference` was selected in the configuration file

    Found 2 diagnostics

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    "
    );

    Ok(())
}

#[test]
fn check_specific_paths() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "project/main.py",
            r#"
            y = 4 / 0  # error: division-by-zero
            "#,
        ),
        (
            "project/tests/test_main.py",
            r#"
            import does_not_exist  # error: unresolved-import
            "#,
        ),
        (
            "project/other.py",
            r#"
            from main2 import z  # error: unresolved-import

            print(z)
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(
        case.command(),
        @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-import]: Cannot resolve imported module `main2`
     --> project/other.py:2:6
      |
    2 | from main2 import z  # error: unresolved-import
      |      ^^^^^
    3 |
    4 | print(z)
      |
    info: make sure your Python environment is properly configured: https://github.com/astral-sh/ty/blob/main/docs/README.md#python-environment
    info: rule `unresolved-import` is enabled by default

    error[unresolved-import]: Cannot resolve imported module `does_not_exist`
     --> project/tests/test_main.py:2:8
      |
    2 | import does_not_exist  # error: unresolved-import
      |        ^^^^^^^^^^^^^^
      |
    info: make sure your Python environment is properly configured: https://github.com/astral-sh/ty/blob/main/docs/README.md#python-environment
    info: rule `unresolved-import` is enabled by default

    Found 2 diagnostics

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    "###
    );

    // Now check only the `tests` and `other.py` files.
    // We should no longer see any diagnostics related to `main.py`.
    assert_cmd_snapshot!(
        case.command().arg("project/tests").arg("project/other.py"),
        @r"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-import]: Cannot resolve imported module `main2`
     --> project/other.py:2:6
      |
    2 | from main2 import z  # error: unresolved-import
      |      ^^^^^
    3 |
    4 | print(z)
      |
    info: make sure your Python environment is properly configured: https://github.com/astral-sh/ty/blob/main/docs/README.md#python-environment
    info: rule `unresolved-import` is enabled by default

    error[unresolved-import]: Cannot resolve imported module `does_not_exist`
     --> project/tests/test_main.py:2:8
      |
    2 | import does_not_exist  # error: unresolved-import
      |        ^^^^^^^^^^^^^^
      |
    info: make sure your Python environment is properly configured: https://github.com/astral-sh/ty/blob/main/docs/README.md#python-environment
    info: rule `unresolved-import` is enabled by default

    Found 2 diagnostics

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    "
    );

    Ok(())
}

#[test]
fn check_non_existing_path() -> anyhow::Result<()> {
    let case = CliTest::with_files([])?;

    let mut settings = insta::Settings::clone_current();
    settings.add_filter(
        &regex::escape("The system cannot find the path specified. (os error 3)"),
        "No such file or directory (os error 2)",
    );
    let _s = settings.bind_to_scope();

    assert_cmd_snapshot!(
        case.command().arg("project/main.py").arg("project/tests"),
        @r"
    success: false
    exit_code: 1
    ----- stdout -----
    error[io]: `<temp_dir>/project/main.py`: No such file or directory (os error 2)

    error[io]: `<temp_dir>/project/tests`: No such file or directory (os error 2)

    Found 2 diagnostics

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    WARN No python files found under the given path(s)
    "
    );

    Ok(())
}

#[test]
fn concise_diagnostics() -> anyhow::Result<()> {
    let case = CliTest::with_file(
        "test.py",
        r#"
        print(x)     # [unresolved-reference]
        print(4[1])  # [non-subscriptable]
        "#,
    )?;

    assert_cmd_snapshot!(case.command().arg("--output-format=concise").arg("--warn").arg("unresolved-reference"), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    warning[unresolved-reference] test.py:2:7: Name `x` used when not defined
    error[non-subscriptable] test.py:3:7: Cannot subscript object of type `Literal[4]` with no `__getitem__` method
    Found 2 diagnostics

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

    Ok(())
}

/// This tests the diagnostic format for revealed type.
///
/// This test was introduced because changes were made to
/// how the revealed type diagnostic was constructed and
/// formatted in "verbose" mode. But it required extra
/// logic to ensure the concise version didn't regress on
/// information content. So this test was introduced to
/// capture that.
#[test]
fn concise_revealed_type() -> anyhow::Result<()> {
    let case = CliTest::with_file(
        "test.py",
        r#"
        from typing_extensions import reveal_type

        x = "hello"
        reveal_type(x)
        "#,
    )?;

    assert_cmd_snapshot!(case.command().arg("--output-format=concise"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    info[revealed-type] test.py:5:13: Revealed type: `Literal["hello"]`
    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    "#);

    Ok(())
}

#[test]
fn can_handle_large_binop_expressions() -> anyhow::Result<()> {
    let mut content = String::new();
    writeln!(
        &mut content,
        "
        from typing_extensions import reveal_type
        total = 1{plus_one_repeated}
        reveal_type(total)
        ",
        plus_one_repeated = " + 1".repeat(2000 - 1)
    )?;

    let case = CliTest::with_file("test.py", &ruff_python_trivia::textwrap::dedent(&content))?;

    assert_cmd_snapshot!(case.command(), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    info[revealed-type]: Revealed type
     --> test.py:4:13
      |
    2 | from typing_extensions import reveal_type
    3 | total = 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1...
    4 | reveal_type(total)
      |             ^^^^^ `Literal[2000]`
      |

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

    Ok(())
}

pub(crate) struct CliTest {
    _temp_dir: TempDir,
    _settings_scope: SettingsBindDropGuard,
    project_dir: PathBuf,
}

impl CliTest {
    pub(crate) fn new() -> anyhow::Result<Self> {
        let temp_dir = TempDir::new()?;

        // Canonicalize the tempdir path because macos uses symlinks for tempdirs
        // and that doesn't play well with our snapshot filtering.
        // Simplify with dunce because otherwise we get UNC paths on Windows.
        let project_dir = dunce::simplified(
            &temp_dir
                .path()
                .canonicalize()
                .context("Failed to canonicalize project path")?,
        )
        .to_path_buf();

        let mut settings = insta::Settings::clone_current();
        settings.add_filter(&tempdir_filter(&project_dir), "<temp_dir>/");
        settings.add_filter(r#"\\(\w\w|\s|\.|")"#, "/$1");

        let settings_scope = settings.bind_to_scope();

        Ok(Self {
            project_dir,
            _temp_dir: temp_dir,
            _settings_scope: settings_scope,
        })
    }

    pub(crate) fn with_files<'a>(
        files: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> anyhow::Result<Self> {
        let case = Self::new()?;
        case.write_files(files)?;
        Ok(case)
    }

    pub(crate) fn with_file(path: impl AsRef<Path>, content: &str) -> anyhow::Result<Self> {
        let case = Self::new()?;
        case.write_file(path, content)?;
        Ok(case)
    }

    pub(crate) fn write_files<'a>(
        &self,
        files: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> anyhow::Result<()> {
        for (path, content) in files {
            self.write_file(path, content)?;
        }

        Ok(())
    }

    fn ensure_parent_directory(path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory `{}`", parent.display()))?;
        }
        Ok(())
    }

    pub(crate) fn write_file(&self, path: impl AsRef<Path>, content: &str) -> anyhow::Result<()> {
        let path = path.as_ref();
        let path = self.project_dir.join(path);

        Self::ensure_parent_directory(&path)?;

        std::fs::write(&path, &*ruff_python_trivia::textwrap::dedent(content))
            .with_context(|| format!("Failed to write file `{path}`", path = path.display()))?;

        Ok(())
    }

    #[cfg(unix)]
    pub(crate) fn write_symlink(
        &self,
        original: impl AsRef<Path>,
        link: impl AsRef<Path>,
    ) -> anyhow::Result<()> {
        let link = link.as_ref();
        let link = self.project_dir.join(link);

        let original = original.as_ref();
        let original = self.project_dir.join(original);

        Self::ensure_parent_directory(&link)?;

        std::os::unix::fs::symlink(original, &link)
            .with_context(|| format!("Failed to write symlink `{link}`", link = link.display()))?;

        Ok(())
    }

    pub(crate) fn root(&self) -> &Path {
        &self.project_dir
    }

    pub(crate) fn command(&self) -> Command {
        let mut command = Command::new(get_cargo_bin("ty"));
        command.current_dir(&self.project_dir).arg("check");

        // Unset environment variables that can affect test behavior
        command.env_remove("VIRTUAL_ENV");
        command.env_remove("CONDA_PREFIX");

        command
    }
}

fn tempdir_filter(path: &Path) -> String {
    format!(r"{}\\?/?", regex::escape(path.to_str().unwrap()))
}
