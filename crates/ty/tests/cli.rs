use insta_cmd::assert_cmd_snapshot;
use std::fmt::Write;

mod common;
use common::TestCase;

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
    assert_cmd_snapshot!(case.command().arg("--config").arg("bad-option=true"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: TOML parse error at line 1, column 1
      |
    1 | bad-option=true
      | ^^^^^^^^^^
    unknown field `bad-option`, expected one of `environment`, `src`, `rules`, `terminal`


    Usage: ty <COMMAND>

    For more information, try '--help'.
    "###);

    Ok(())
}

#[test]
fn config_file_override() -> anyhow::Result<()> {
    // Set `error-on-warning` to true in the configuration file
    // Explicitly set `--warn unresolved-reference` to ensure the rule warns instead of errors
    let case = TestCase::with_files(vec![
        ("test.py", r"print(x)  # [unresolved-reference]"),
        (
            "ty-override.toml",
            r#"
            [terminal]
            error-on-warning = true
            "#,
        ),
    ])?;

    // Ensure flag works via CLI arg
    assert_cmd_snapshot!(case.command().arg("--warn").arg("unresolved-reference").arg("--config-file").arg("ty-override.toml"), @r"
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

    // Ensure the flag works via an environment variable
    assert_cmd_snapshot!(case.command().arg("--warn").arg("unresolved-reference").env("TY_CONFIG_FILE", "ty-override.toml"), @r"
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
