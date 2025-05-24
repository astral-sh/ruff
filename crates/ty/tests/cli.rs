use anyhow::Context;
use insta::internals::SettingsBindDropGuard;
use insta_cmd::{assert_cmd_snapshot, get_cargo_bin};
use ruff_python_ast::PythonVersion;
use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

#[test]
fn test_run_in_sub_directory() -> anyhow::Result<()> {
    let case = TestCase::with_files([("test.py", "~"), ("subdir/nothing", "")])?;
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
    let case = TestCase::with_files([(".test.py", "~")])?;
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
    let case = TestCase::with_files([(".ignore", "test.py"), ("test.py", "~")])?;
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
    case.write_file("ty.toml", "respect-ignore-files = false")?;
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
    case.write_file("ty.toml", "respect-ignore-files = true")?;
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
/// Specifying an option on the CLI should take precedence over the same setting in the
/// project's configuration. Here, this is tested for the Python version.
#[test]
fn config_override_python_version() -> anyhow::Result<()> {
    let case = TestCase::with_files([
        (
            "pyproject.toml",
            r#"
            [tool.ty.environment]
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

    assert_cmd_snapshot!(case.command(), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-attribute]: Type `<module 'sys'>` has no attribute `last_exc`
     --> test.py:5:7
      |
    4 | # Access `sys.last_exc` that was only added in Python 3.12
    5 | print(sys.last_exc)
      |       ^^^^^^^^^^^^
      |
    info: rule `unresolved-attribute` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

    assert_cmd_snapshot!(case.command().arg("--python-version").arg("3.12"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

    Ok(())
}

/// Same as above, but for the Python platform.
#[test]
fn config_override_python_platform() -> anyhow::Result<()> {
    let case = TestCase::with_files([
        (
            "pyproject.toml",
            r#"
            [tool.ty.environment]
            python-platform = "linux"
            "#,
        ),
        (
            "test.py",
            r#"
            import sys
            from typing_extensions import reveal_type

            reveal_type(sys.platform)
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command(), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    info[revealed-type]: Revealed type
     --> test.py:5:13
      |
    3 | from typing_extensions import reveal_type
    4 |
    5 | reveal_type(sys.platform)
      |             ^^^^^^^^^^^^ `Literal["linux"]`
      |

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    "#);

    assert_cmd_snapshot!(case.command().arg("--python-platform").arg("all"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    info[revealed-type]: Revealed type
     --> test.py:5:13
      |
    3 | from typing_extensions import reveal_type
    4 |
    5 | reveal_type(sys.platform)
      |             ^^^^^^^^^^^^ `LiteralString`
      |

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

    Ok(())
}

#[test]
fn config_file_annotation_showing_where_python_version_set() -> anyhow::Result<()> {
    let case = TestCase::with_files([
        (
            "pyproject.toml",
            r#"
            [tool.ty.environment]
            python-version = "3.8"
            "#,
        ),
        (
            "test.py",
            r#"
            aiter
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command(), @r#"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-reference]: Name `aiter` used when not defined
     --> test.py:2:1
      |
    2 | aiter
      | ^^^^^
      |
    info: `aiter` was added as a builtin in Python 3.10
    info: Python 3.8 was assumed when resolving types
     --> pyproject.toml:3:18
      |
    2 | [tool.ty.environment]
    3 | python-version = "3.8"
      |                  ^^^^^ Python 3.8 assumed due to this configuration setting
      |
    info: rule `unresolved-reference` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    "#);

    assert_cmd_snapshot!(case.command().arg("--python-version=3.9"), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-reference]: Name `aiter` used when not defined
     --> test.py:2:1
      |
    2 | aiter
      | ^^^^^
      |
    info: `aiter` was added as a builtin in Python 3.10
    info: Python 3.9 was assumed when resolving types because it was specified on the command line
    info: rule `unresolved-reference` is enabled by default

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
    let case = TestCase::with_files([
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
    let case = TestCase::with_files([
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

/// The rule severity can be changed in the configuration file
#[test]
fn configuration_rule_severity() -> anyhow::Result<()> {
    let case = TestCase::with_file(
        "test.py",
        r#"
            y = 4 / 0

            for a in range(0, int(y)):
                x = a

            prin(x)  # unresolved-reference
            "#,
    )?;

    // Assert that there's an `unresolved-reference` diagnostic (error).
    assert_cmd_snapshot!(case.command(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-reference]: Name `prin` used when not defined
     --> test.py:7:1
      |
    5 |     x = a
    6 |
    7 | prin(x)  # unresolved-reference
      | ^^^^
      |
    info: rule `unresolved-reference` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    "###);

    case.write_file(
        "pyproject.toml",
        r#"
        [tool.ty.rules]
        division-by-zero = "warn" # promote to warn
        unresolved-reference = "ignore"
    "#,
    )?;

    assert_cmd_snapshot!(case.command(), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    warning[division-by-zero]: Cannot divide object of type `Literal[4]` by zero
     --> test.py:2:5
      |
    2 | y = 4 / 0
      |     ^^^^^
    3 |
    4 | for a in range(0, int(y)):
      |
    info: rule `division-by-zero` was selected in the configuration file

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

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

        for a in range(0, int(y)):
            x = a

        prin(x)  # unresolved-reference
        "#,
    )?;

    // Assert that there's an `unresolved-reference` diagnostic (error)
    // and an unresolved-import (error) diagnostic by default.
    assert_cmd_snapshot!(case.command(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-import]: Cannot resolve imported module `does_not_exit`
     --> test.py:2:8
      |
    2 | import does_not_exit
      |        ^^^^^^^^^^^^^
    3 |
    4 | y = 4 / 0
      |
    info: make sure your Python environment is properly configured: https://github.com/astral-sh/ty/blob/main/docs/README.md#python-environment
    info: rule `unresolved-import` is enabled by default

    error[unresolved-reference]: Name `prin` used when not defined
     --> test.py:9:1
      |
    7 |     x = a
    8 |
    9 | prin(x)  # unresolved-reference
      | ^^^^
      |
    info: rule `unresolved-reference` is enabled by default

    Found 2 diagnostics

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    "###);

    assert_cmd_snapshot!(
        case
            .command()
            .arg("--ignore")
            .arg("unresolved-reference")
            .arg("--warn")
            .arg("division-by-zero")
            .arg("--warn")
            .arg("unresolved-import"),
        @r"
    success: true
    exit_code: 0
    ----- stdout -----
    warning[unresolved-import]: Cannot resolve imported module `does_not_exit`
     --> test.py:2:8
      |
    2 | import does_not_exit
      |        ^^^^^^^^^^^^^
    3 |
    4 | y = 4 / 0
      |
    info: make sure your Python environment is properly configured: https://github.com/astral-sh/ty/blob/main/docs/README.md#python-environment
    info: rule `unresolved-import` was selected on the command line

    warning[division-by-zero]: Cannot divide object of type `Literal[4]` by zero
     --> test.py:4:5
      |
    2 | import does_not_exit
    3 |
    4 | y = 4 / 0
      |     ^^^^^
    5 |
    6 | for a in range(0, int(y)):
      |
    info: rule `division-by-zero` was selected on the command line

    Found 2 diagnostics

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    "
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

        for a in range(0, int(y)):
            x = a

        prin(x)  # unresolved-reference
        "#,
    )?;

    // Assert that there's a `unresolved-reference` diagnostic (error) by default.
    assert_cmd_snapshot!(case.command(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-reference]: Name `prin` used when not defined
     --> test.py:7:1
      |
    5 |     x = a
    6 |
    7 | prin(x)  # unresolved-reference
      | ^^^^
      |
    info: rule `unresolved-reference` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    "###);

    assert_cmd_snapshot!(
        case
            .command()
            .arg("--warn")
            .arg("unresolved-reference")
            .arg("--warn")
            .arg("division-by-zero")
            .arg("--ignore")
            .arg("unresolved-reference"),
        @r"
    success: true
    exit_code: 0
    ----- stdout -----
    warning[division-by-zero]: Cannot divide object of type `Literal[4]` by zero
     --> test.py:2:5
      |
    2 | y = 4 / 0
      |     ^^^^^
    3 |
    4 | for a in range(0, int(y)):
      |
    info: rule `division-by-zero` was selected on the command line

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    "
    );

    Ok(())
}

/// ty warns about unknown rules specified in a configuration file
#[test]
fn configuration_unknown_rules() -> anyhow::Result<()> {
    let case = TestCase::with_files([
        (
            "pyproject.toml",
            r#"
            [tool.ty.rules]
            division-by-zer = "warn" # incorrect rule name
            "#,
        ),
        ("test.py", "print(10)"),
    ])?;

    assert_cmd_snapshot!(case.command(), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    warning[unknown-rule]
     --> pyproject.toml:3:1
      |
    2 | [tool.ty.rules]
    3 | division-by-zer = "warn" # incorrect rule name
      | ^^^^^^^^^^^^^^^ Unknown lint rule `division-by-zer`
      |

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    "#);

    Ok(())
}

/// ty warns about unknown rules specified in a CLI argument
#[test]
fn cli_unknown_rules() -> anyhow::Result<()> {
    let case = TestCase::with_file("test.py", "print(10)")?;

    assert_cmd_snapshot!(case.command().arg("--ignore").arg("division-by-zer"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    warning[unknown-rule]: Unknown lint rule `division-by-zer`

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

    Ok(())
}

#[test]
fn exit_code_only_warnings() -> anyhow::Result<()> {
    let case = TestCase::with_file("test.py", r"print(x)  # [unresolved-reference]")?;

    assert_cmd_snapshot!(case.command().arg("--warn").arg("unresolved-reference"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    warning[unresolved-reference]: Name `x` used when not defined
     --> test.py:1:7
      |
    1 | print(x)  # [unresolved-reference]
      |       ^
      |
    info: rule `unresolved-reference` was selected on the command line

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

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

    assert_cmd_snapshot!(case.command(), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    info[revealed-type]: Revealed type
     --> test.py:3:13
      |
    2 | from typing_extensions import reveal_type
    3 | reveal_type(1)
      |             ^ `Literal[1]`
      |

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

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

    assert_cmd_snapshot!(case.command().arg("--error-on-warning"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    info[revealed-type]: Revealed type
     --> test.py:3:13
      |
    2 | from typing_extensions import reveal_type
    3 | reveal_type(1)
      |             ^ `Literal[1]`
      |

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

    Ok(())
}

#[test]
fn exit_code_no_errors_but_error_on_warning_is_true() -> anyhow::Result<()> {
    let case = TestCase::with_file("test.py", r"print(x)  # [unresolved-reference]")?;

    assert_cmd_snapshot!(case.command().arg("--error-on-warning").arg("--warn").arg("unresolved-reference"), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    warning[unresolved-reference]: Name `x` used when not defined
     --> test.py:1:7
      |
    1 | print(x)  # [unresolved-reference]
      |       ^
      |
    info: rule `unresolved-reference` was selected on the command line

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

    Ok(())
}

#[test]
fn exit_code_no_errors_but_error_on_warning_is_enabled_in_configuration() -> anyhow::Result<()> {
    let case = TestCase::with_files([
        ("test.py", r"print(x)  # [unresolved-reference]"),
        (
            "ty.toml",
            r#"
            [terminal]
            error-on-warning = true
        "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command().arg("--warn").arg("unresolved-reference"), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    warning[unresolved-reference]: Name `x` used when not defined
     --> test.py:1:7
      |
    1 | print(x)  # [unresolved-reference]
      |       ^
      |
    info: rule `unresolved-reference` was selected on the command line

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

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

    assert_cmd_snapshot!(case.command().arg("--warn").arg("unresolved-reference"), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    warning[unresolved-reference]: Name `x` used when not defined
     --> test.py:2:7
      |
    2 | print(x)     # [unresolved-reference]
      |       ^
    3 | print(4[1])  # [non-subscriptable]
      |
    info: rule `unresolved-reference` was selected on the command line

    error[non-subscriptable]: Cannot subscript object of type `Literal[4]` with no `__getitem__` method
     --> test.py:3:7
      |
    2 | print(x)     # [unresolved-reference]
    3 | print(4[1])  # [non-subscriptable]
      |       ^
      |
    info: rule `non-subscriptable` is enabled by default

    Found 2 diagnostics

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

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

    assert_cmd_snapshot!(case.command().arg("--warn").arg("unresolved-reference").arg("--error-on-warning"), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    warning[unresolved-reference]: Name `x` used when not defined
     --> test.py:2:7
      |
    2 | print(x)     # [unresolved-reference]
      |       ^
    3 | print(4[1])  # [non-subscriptable]
      |
    info: rule `unresolved-reference` was selected on the command line

    error[non-subscriptable]: Cannot subscript object of type `Literal[4]` with no `__getitem__` method
     --> test.py:3:7
      |
    2 | print(x)     # [unresolved-reference]
    3 | print(4[1])  # [non-subscriptable]
      |       ^
      |
    info: rule `non-subscriptable` is enabled by default

    Found 2 diagnostics

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

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

    assert_cmd_snapshot!(case.command().arg("--exit-zero").arg("--warn").arg("unresolved-reference"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    warning[unresolved-reference]: Name `x` used when not defined
     --> test.py:2:7
      |
    2 | print(x)     # [unresolved-reference]
      |       ^
    3 | print(4[1])  # [non-subscriptable]
      |
    info: rule `unresolved-reference` was selected on the command line

    error[non-subscriptable]: Cannot subscript object of type `Literal[4]` with no `__getitem__` method
     --> test.py:3:7
      |
    2 | print(x)     # [unresolved-reference]
    3 | print(4[1])  # [non-subscriptable]
      |       ^
      |
    info: rule `non-subscriptable` is enabled by default

    Found 2 diagnostics

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

    Ok(())
}

#[test]
fn user_configuration() -> anyhow::Result<()> {
    let case = TestCase::with_files([
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
    let case = TestCase::with_files([
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
    let case = TestCase::with_files([])?;

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
    let case = TestCase::with_file(
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
    let case = TestCase::with_file(
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

    let case = TestCase::with_file("test.py", &ruff_python_trivia::textwrap::dedent(&content))?;

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

#[test]
fn defaults_to_a_new_python_version() -> anyhow::Result<()> {
    let case = TestCase::with_files([
        (
            "ty.toml",
            &*format!(
                r#"
                [environment]
                python-version = "{}"
                python-platform = "linux"
                "#,
                PythonVersion::default()
            ),
        ),
        (
            "main.py",
            r#"
            import os

            os.grantpt(1) # only available on unix, Python 3.13 or newer
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command(), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-attribute]: Type `<module 'os'>` has no attribute `grantpt`
     --> main.py:4:1
      |
    2 | import os
    3 |
    4 | os.grantpt(1) # only available on unix, Python 3.13 or newer
      | ^^^^^^^^^^
      |
    info: rule `unresolved-attribute` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

    // Use default (which should be latest supported)
    let case = TestCase::with_files([
        (
            "ty.toml",
            r#"
            [environment]
            python-platform = "linux"
            "#,
        ),
        (
            "main.py",
            r#"
            import os

            os.grantpt(1) # only available on unix, Python 3.13 or newer
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command(), @r"
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
fn cli_config_args_toml_string_basic() -> anyhow::Result<()> {
    let case = TestCase::with_file("test.py", r"print(x)  # [unresolved-reference]")?;

    // Long flag
    assert_cmd_snapshot!(case.command().arg("--warn").arg("unresolved-reference").arg("--config").arg("terminal.error-on-warning=true"), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    warning[unresolved-reference]: Name `x` used when not defined
     --> test.py:1:7
      |
    1 | print(x)  # [unresolved-reference]
      |       ^
      |
    info: rule `unresolved-reference` was selected on the command line

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

    // Short flag
    assert_cmd_snapshot!(case.command().arg("-c").arg("terminal.error-on-warning=true"), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-reference]: Name `x` used when not defined
     --> test.py:1:7
      |
    1 | print(x)  # [unresolved-reference]
      |       ^
      |
    info: rule `unresolved-reference` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

    Ok(())
}

#[test]
fn cli_config_args_overrides_ty_toml() -> anyhow::Result<()> {
    let case = TestCase::with_files(vec![
        (
            "ty.toml",
            r#"
            [terminal]
            error-on-warning = true
            "#,
        ),
        ("test.py", r"print(x)  # [unresolved-reference]"),
    ])?;

    // Exit code of 1 due to the setting in `ty.toml`
    assert_cmd_snapshot!(case.command().arg("--warn").arg("unresolved-reference"), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    warning[unresolved-reference]: Name `x` used when not defined
     --> test.py:1:7
      |
    1 | print(x)  # [unresolved-reference]
      |       ^
      |
    info: rule `unresolved-reference` was selected on the command line

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

    // Exit code of 0 because the `ty.toml` setting is overwritten by `--config`
    assert_cmd_snapshot!(case.command().arg("--warn").arg("unresolved-reference").arg("--config").arg("terminal.error-on-warning=false"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    warning[unresolved-reference]: Name `x` used when not defined
     --> test.py:1:7
      |
    1 | print(x)  # [unresolved-reference]
      |       ^
      |
    info: rule `unresolved-reference` was selected on the command line

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

    Ok(())
}

#[test]
fn cli_config_args_later_overrides_earlier() -> anyhow::Result<()> {
    let case = TestCase::with_file("test.py", r"print(x)  # [unresolved-reference]")?;
    assert_cmd_snapshot!(case.command().arg("--warn").arg("unresolved-reference").arg("--config").arg("terminal.error-on-warning=true").arg("--config").arg("terminal.error-on-warning=false"), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    warning[unresolved-reference]: Name `x` used when not defined
     --> test.py:1:7
      |
    1 | print(x)  # [unresolved-reference]
      |       ^
      |
    info: rule `unresolved-reference` was selected on the command line

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

    Ok(())
}

#[test]
fn cli_config_args_invalid_option() -> anyhow::Result<()> {
    let case = TestCase::with_file("test.py", r"print(1)")?;
    assert_cmd_snapshot!(case.command().arg("--config").arg("bad-option=true"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: TOML parse error at line 1, column 1
      |
    1 | bad-option=true
      | ^^^^^^^^^^
    unknown field `bad-option`, expected one of `environment`, `src`, `rules`, `terminal`, `respect-ignore-files`


    Usage: ty <COMMAND>

    For more information, try '--help'.
    ");

    Ok(())
}

/// The `site-packages` directory is used by ty for external import.
/// Ty does the following checks to discover the `site-packages` directory in the order:
/// 1) If `VIRTUAL_ENV` environment variable is set
/// 2) If `CONDA_PREFIX` environment variable is set
/// 3) If a `.venv` directory exists at the project root
///
/// This test is aiming at validating the logic around `CONDA_PREFIX`.
///
/// A conda-like environment file structure is used
/// We test by first not setting the `CONDA_PREFIX` and expect a fail.
/// Then we test by setting `CONDA_PREFIX` to `conda-env` and expect a pass.
///
/// ├── project
/// │   └── test.py
/// └── conda-env
///     └── lib
///         └── python3.13
///             └── site-packages
///                 └── package1
///                     └── __init__.py
///
/// test.py imports package1
/// And the command is run in the `project` directory.
#[test]
fn check_conda_prefix_var_to_resolve_path() -> anyhow::Result<()> {
    let conda_package1_path = if cfg!(windows) {
        "conda-env/Lib/site-packages/package1/__init__.py"
    } else {
        "conda-env/lib/python3.13/site-packages/package1/__init__.py"
    };

    let case = TestCase::with_files([
        (
            "project/test.py",
            r#"
            import package1
            "#,
        ),
        (
            conda_package1_path,
            r#"
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command().current_dir(case.root().join("project")), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-import]: Cannot resolve imported module `package1`
     --> test.py:2:8
      |
    2 | import package1
      |        ^^^^^^^^
      |
    info: make sure your Python environment is properly configured: https://github.com/astral-sh/ty/blob/main/docs/README.md#python-environment
    info: rule `unresolved-import` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

    // do command : CONDA_PREFIX=<temp_dir>/conda_env
    assert_cmd_snapshot!(case.command().current_dir(case.root().join("project")).env("CONDA_PREFIX", case.root().join("conda-env")), @r"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");
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

    fn root(&self) -> &Path {
        &self.project_dir
    }

    fn command(&self) -> Command {
        let mut command = Command::new(get_cargo_bin("ty"));
        command.current_dir(&self.project_dir).arg("check");
        command
    }
}

fn tempdir_filter(path: &Path) -> String {
    format!(r"{}\\?/?", regex::escape(path.to_str().unwrap()))
}
