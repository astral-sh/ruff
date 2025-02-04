use anyhow::Context;
use insta::internals::SettingsBindDropGuard;
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

    assert_cmd_snapshot!(case.command(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    error: lint:unresolved-attribute
     --> <temp_dir>/test.py:5:7
      |
    4 | # Access `sys.last_exc` that was only added in Python 3.12
    5 | print(sys.last_exc)
      |       ^^^^^^^^^^^^ Type `<module 'sys'>` has no attribute `last_exc`
      |


    ----- stderr -----
    "###);

    assert_cmd_snapshot!(case.command().arg("--python-version").arg("3.12"), @r"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
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

    // Make sure that the CLI fails when the `libs` directory is not in the search path.
    assert_cmd_snapshot!(case.command().current_dir(case.project_dir().join("child")), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    error: lint:unresolved-import
     --> <temp_dir>/child/test.py:2:1
      |
    2 | from utils import add
      | ^^^^^^^^^^^^^^^^^^^^^ Cannot resolve import `utils`
    3 |
    4 | stat = add(10, 15)
      |


    ----- stderr -----
    "###);

    assert_cmd_snapshot!(case.command().current_dir(case.project_dir().join("child")).arg("--extra-search-path").arg("../libs"), @r"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
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

    assert_cmd_snapshot!(case.command().current_dir(case.project_dir().join("child")), @r"
        success: true
        exit_code: 0
        ----- stdout -----

        ----- stderr -----
    ");

    Ok(())
}

/// The rule severity can be changed in the configuration file
#[test]
fn configuration_rule_severity() -> anyhow::Result<()> {
    let case = TestCase::with_file(
        "test.py",
        r#"
            y = 4 / 0

            for a in range(0, y):
                x = a

            print(x)  # possibly-unresolved-reference
            "#,
    )?;

    // Assert that there's a possibly unresolved reference diagnostic
    // and that division-by-zero has a severity of error by default.
    assert_cmd_snapshot!(case.command(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    error: lint:division-by-zero
     --> <temp_dir>/test.py:2:5
      |
    2 | y = 4 / 0
      |     ^^^^^ Cannot divide object of type `Literal[4]` by zero
    3 |
    4 | for a in range(0, y):
      |

    warning: lint:possibly-unresolved-reference
     --> <temp_dir>/test.py:7:7
      |
    5 |     x = a
    6 |
    7 | print(x)  # possibly-unresolved-reference
      |       - Name `x` used when possibly not defined
      |


    ----- stderr -----
    "###);

    case.write_file(
        "pyproject.toml",
        r#"
        [tool.knot.rules]
        division-by-zero = "warn" # demote to warn
        possibly-unresolved-reference = "ignore"
    "#,
    )?;

    assert_cmd_snapshot!(case.command(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    warning: lint:division-by-zero
     --> <temp_dir>/test.py:2:5
      |
    2 | y = 4 / 0
      |     ----- Cannot divide object of type `Literal[4]` by zero
    3 |
    4 | for a in range(0, y):
      |


    ----- stderr -----
    "###);

    Ok(())
}

/// The rule severity can be changed using `--ignore`, `--warn`, and `--error`
#[test]
fn cli_rule_severity() -> anyhow::Result<()> {
    let case = TestCase::with_file(
        "test.py",
        r#"
        import does_not_exit

        y = 4 / 0

        for a in range(0, y):
            x = a

        print(x)  # possibly-unresolved-reference
        "#,
    )?;

    // Assert that there's a possibly unresolved reference diagnostic
    // and that division-by-zero has a severity of error by default.
    assert_cmd_snapshot!(case.command(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    error: lint:unresolved-import
     --> <temp_dir>/test.py:2:8
      |
    2 | import does_not_exit
      |        ^^^^^^^^^^^^^ Cannot resolve import `does_not_exit`
    3 |
    4 | y = 4 / 0
      |

    error: lint:division-by-zero
     --> <temp_dir>/test.py:4:5
      |
    2 | import does_not_exit
    3 |
    4 | y = 4 / 0
      |     ^^^^^ Cannot divide object of type `Literal[4]` by zero
    5 |
    6 | for a in range(0, y):
      |

    warning: lint:possibly-unresolved-reference
     --> <temp_dir>/test.py:9:7
      |
    7 |     x = a
    8 |
    9 | print(x)  # possibly-unresolved-reference
      |       - Name `x` used when possibly not defined
      |


    ----- stderr -----
    "###);

    assert_cmd_snapshot!(
        case
            .command()
            .arg("--ignore")
            .arg("possibly-unresolved-reference")
            .arg("--warn")
            .arg("division-by-zero")
            .arg("--warn")
            .arg("unresolved-import"),
        @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    warning: lint:unresolved-import
     --> <temp_dir>/test.py:2:8
      |
    2 | import does_not_exit
      |        ------------- Cannot resolve import `does_not_exit`
    3 |
    4 | y = 4 / 0
      |

    warning: lint:division-by-zero
     --> <temp_dir>/test.py:4:5
      |
    2 | import does_not_exit
    3 |
    4 | y = 4 / 0
      |     ----- Cannot divide object of type `Literal[4]` by zero
    5 |
    6 | for a in range(0, y):
      |


    ----- stderr -----
    "###
    );

    Ok(())
}

/// The rule severity can be changed using `--ignore`, `--warn`, and `--error` and
/// values specified last override previous severities.
#[test]
fn cli_rule_severity_precedence() -> anyhow::Result<()> {
    let case = TestCase::with_file(
        "test.py",
        r#"
        y = 4 / 0

        for a in range(0, y):
            x = a

        print(x)  # possibly-unresolved-reference
        "#,
    )?;

    // Assert that there's a possibly unresolved reference diagnostic
    // and that division-by-zero has a severity of error by default.
    assert_cmd_snapshot!(case.command(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    error: lint:division-by-zero
     --> <temp_dir>/test.py:2:5
      |
    2 | y = 4 / 0
      |     ^^^^^ Cannot divide object of type `Literal[4]` by zero
    3 |
    4 | for a in range(0, y):
      |

    warning: lint:possibly-unresolved-reference
     --> <temp_dir>/test.py:7:7
      |
    5 |     x = a
    6 |
    7 | print(x)  # possibly-unresolved-reference
      |       - Name `x` used when possibly not defined
      |


    ----- stderr -----
    "###);

    assert_cmd_snapshot!(
        case
            .command()
            .arg("--error")
            .arg("possibly-unresolved-reference")
            .arg("--warn")
            .arg("division-by-zero")
            // Override the error severity with warning
            .arg("--ignore")
            .arg("possibly-unresolved-reference"),
        @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    warning: lint:division-by-zero
     --> <temp_dir>/test.py:2:5
      |
    2 | y = 4 / 0
      |     ----- Cannot divide object of type `Literal[4]` by zero
    3 |
    4 | for a in range(0, y):
      |


    ----- stderr -----
    "###
    );

    Ok(())
}

/// Red Knot warns about unknown rules specified in a configuration file
#[test]
fn configuration_unknown_rules() -> anyhow::Result<()> {
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

    assert_cmd_snapshot!(case.command(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    warning: unknown-rule
     --> <temp_dir>/pyproject.toml:3:1
      |
    2 | [tool.knot.rules]
    3 | division-by-zer = "warn" # incorrect rule name
      | --------------- Unknown lint rule `division-by-zer`
      |


    ----- stderr -----
    "###);

    Ok(())
}

/// Red Knot warns about unknown rules specified in a CLI argument
#[test]
fn cli_unknown_rules() -> anyhow::Result<()> {
    let case = TestCase::with_file("test.py", "print(10)")?;

    assert_cmd_snapshot!(case.command().arg("--ignore").arg("division-by-zer"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    warning: unknown-rule: Unknown lint rule `division-by-zer`


    ----- stderr -----
    "###);

    Ok(())
}

#[test]
fn exit_code_only_warnings() -> anyhow::Result<()> {
    let case = TestCase::with_file("test.py", r"print(x)  # [unresolved-reference]")?;

    assert_cmd_snapshot!(case.command(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    warning: lint:unresolved-reference
     --> <temp_dir>/test.py:1:7
      |
    1 | print(x)  # [unresolved-reference]
      |       - Name `x` used when not defined
      |


    ----- stderr -----
    "###);

    Ok(())
}

#[test]
fn exit_code_only_info() -> anyhow::Result<()> {
    let case = TestCase::with_file(
        "test.py",
        r#"
        from typing_extensions import reveal_type
        reveal_type(1)
        "#,
    )?;

    assert_cmd_snapshot!(case.command(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    info: revealed-type
     --> <temp_dir>/test.py:3:1
      |
    2 | from typing_extensions import reveal_type
    3 | reveal_type(1)
      | -------------- info: Revealed type is `Literal[1]`
      |


    ----- stderr -----
    "###);

    Ok(())
}

#[test]
fn exit_code_only_info_and_error_on_warning_is_true() -> anyhow::Result<()> {
    let case = TestCase::with_file(
        "test.py",
        r#"
        from typing_extensions import reveal_type
        reveal_type(1)
        "#,
    )?;

    assert_cmd_snapshot!(case.command().arg("--error-on-warning"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    info: revealed-type
     --> <temp_dir>/test.py:3:1
      |
    2 | from typing_extensions import reveal_type
    3 | reveal_type(1)
      | -------------- info: Revealed type is `Literal[1]`
      |


    ----- stderr -----
    "###);

    Ok(())
}

#[test]
fn exit_code_no_errors_but_error_on_warning_is_true() -> anyhow::Result<()> {
    let case = TestCase::with_file("test.py", r"print(x)  # [unresolved-reference]")?;

    assert_cmd_snapshot!(case.command().arg("--error-on-warning"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    warning: lint:unresolved-reference
     --> <temp_dir>/test.py:1:7
      |
    1 | print(x)  # [unresolved-reference]
      |       - Name `x` used when not defined
      |


    ----- stderr -----
    "###);

    Ok(())
}

#[test]
fn exit_code_both_warnings_and_errors() -> anyhow::Result<()> {
    let case = TestCase::with_file(
        "test.py",
        r#"
        print(x)     # [unresolved-reference]
        print(4[1])  # [non-subscriptable]
        "#,
    )?;

    assert_cmd_snapshot!(case.command(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    warning: lint:unresolved-reference
     --> <temp_dir>/test.py:2:7
      |
    2 | print(x)     # [unresolved-reference]
      |       - Name `x` used when not defined
    3 | print(4[1])  # [non-subscriptable]
      |

    error: lint:non-subscriptable
     --> <temp_dir>/test.py:3:7
      |
    2 | print(x)     # [unresolved-reference]
    3 | print(4[1])  # [non-subscriptable]
      |       ^ Cannot subscript object of type `Literal[4]` with no `__getitem__` method
      |


    ----- stderr -----
    "###);

    Ok(())
}

#[test]
fn exit_code_both_warnings_and_errors_and_error_on_warning_is_true() -> anyhow::Result<()> {
    let case = TestCase::with_file(
        "test.py",
        r###"
        print(x)     # [unresolved-reference]
        print(4[1])  # [non-subscriptable]
        "###,
    )?;

    assert_cmd_snapshot!(case.command().arg("--error-on-warning"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    warning: lint:unresolved-reference
     --> <temp_dir>/test.py:2:7
      |
    2 | print(x)     # [unresolved-reference]
      |       - Name `x` used when not defined
    3 | print(4[1])  # [non-subscriptable]
      |

    error: lint:non-subscriptable
     --> <temp_dir>/test.py:3:7
      |
    2 | print(x)     # [unresolved-reference]
    3 | print(4[1])  # [non-subscriptable]
      |       ^ Cannot subscript object of type `Literal[4]` with no `__getitem__` method
      |


    ----- stderr -----
    "###);

    Ok(())
}

#[test]
fn exit_code_exit_zero_is_true() -> anyhow::Result<()> {
    let case = TestCase::with_file(
        "test.py",
        r#"
        print(x)     # [unresolved-reference]
        print(4[1])  # [non-subscriptable]
        "#,
    )?;

    assert_cmd_snapshot!(case.command().arg("--exit-zero"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    warning: lint:unresolved-reference
     --> <temp_dir>/test.py:2:7
      |
    2 | print(x)     # [unresolved-reference]
      |       - Name `x` used when not defined
    3 | print(4[1])  # [non-subscriptable]
      |

    error: lint:non-subscriptable
     --> <temp_dir>/test.py:3:7
      |
    2 | print(x)     # [unresolved-reference]
    3 | print(4[1])  # [non-subscriptable]
      |       ^ Cannot subscript object of type `Literal[4]` with no `__getitem__` method
      |


    ----- stderr -----
    "###);

    Ok(())
}

struct TestCase {
    _temp_dir: TempDir,
    _settings_scope: SettingsBindDropGuard,
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

    fn command(&self) -> Command {
        let mut command = Command::new(get_cargo_bin("red_knot"));
        command.current_dir(&self.project_dir).arg("check");
        command
    }
}

fn tempdir_filter(path: &Path) -> String {
    format!(r"{}\\?/?", regex::escape(path.to_str().unwrap()))
}
