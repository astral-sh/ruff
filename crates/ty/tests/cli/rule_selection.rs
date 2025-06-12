use insta_cmd::assert_cmd_snapshot;

use crate::CliTest;

/// The rule severity can be changed in the configuration file
#[test]
fn configuration_rule_severity() -> anyhow::Result<()> {
    let case = CliTest::with_file(
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
    let case = CliTest::with_file(
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
    let case = CliTest::with_file(
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
    let case = CliTest::with_files([
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
    let case = CliTest::with_file("test.py", "print(10)")?;

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
