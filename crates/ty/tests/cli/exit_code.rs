use insta_cmd::assert_cmd_snapshot;

use crate::CliTest;

#[test]
fn only_warnings() -> anyhow::Result<()> {
    let case = CliTest::with_file("test.py", r"print(x)  # [unresolved-reference]")?;

    assert_cmd_snapshot!(case.command().arg("--warn").arg("unresolved-reference"), @r###"
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
    "###);

    Ok(())
}

#[test]
fn only_info() -> anyhow::Result<()> {
    let case = CliTest::with_file(
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
    info[revealed-type]: Revealed type
     --> test.py:3:13
      |
    2 | from typing_extensions import reveal_type
    3 | reveal_type(1)
      |             ^ `Literal[1]`
      |

    Found 1 diagnostic

    ----- stderr -----
    "###);

    Ok(())
}

#[test]
fn only_info_and_error_on_warning_is_true() -> anyhow::Result<()> {
    let case = CliTest::with_file(
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
    info[revealed-type]: Revealed type
     --> test.py:3:13
      |
    2 | from typing_extensions import reveal_type
    3 | reveal_type(1)
      |             ^ `Literal[1]`
      |

    Found 1 diagnostic

    ----- stderr -----
    "###);

    Ok(())
}

#[test]
fn no_errors_but_error_on_warning_is_true() -> anyhow::Result<()> {
    let case = CliTest::with_file("test.py", r"print(x)  # [unresolved-reference]")?;

    assert_cmd_snapshot!(case.command().arg("--error-on-warning").arg("--warn").arg("unresolved-reference"), @r###"
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
    "###);

    Ok(())
}

#[test]
fn no_errors_but_error_on_warning_is_enabled_in_configuration() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        ("test.py", r"print(x)  # [unresolved-reference]"),
        (
            "ty.toml",
            r#"
            [terminal]
            error-on-warning = true
        "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command().arg("--warn").arg("unresolved-reference"), @r###"
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
    "###);

    Ok(())
}

#[test]
fn both_warnings_and_errors() -> anyhow::Result<()> {
    let case = CliTest::with_file(
        "test.py",
        r#"
        print(x)     # [unresolved-reference]
        print(4[1])  # [not-subscriptable]
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
    3 | print(4[1])  # [not-subscriptable]
      |
    info: rule `unresolved-reference` was selected on the command line

    error[not-subscriptable]: Cannot subscript object of type `Literal[4]` with no `__getitem__` method
     --> test.py:3:7
      |
    2 | print(x)     # [unresolved-reference]
    3 | print(4[1])  # [not-subscriptable]
      |       ^^^^
      |
    info: rule `not-subscriptable` is enabled by default

    Found 2 diagnostics

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn both_warnings_and_errors_and_error_on_warning_is_true() -> anyhow::Result<()> {
    let case = CliTest::with_file(
        "test.py",
        r###"
        print(x)     # [unresolved-reference]
        print(4[1])  # [not-subscriptable]
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
    3 | print(4[1])  # [not-subscriptable]
      |
    info: rule `unresolved-reference` was selected on the command line

    error[not-subscriptable]: Cannot subscript object of type `Literal[4]` with no `__getitem__` method
     --> test.py:3:7
      |
    2 | print(x)     # [unresolved-reference]
    3 | print(4[1])  # [not-subscriptable]
      |       ^^^^
      |
    info: rule `not-subscriptable` is enabled by default

    Found 2 diagnostics

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn exit_zero_is_true() -> anyhow::Result<()> {
    let case = CliTest::with_file(
        "test.py",
        r#"
        print(x)     # [unresolved-reference]
        print(4[1])  # [not-subscriptable]
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
    3 | print(4[1])  # [not-subscriptable]
      |
    info: rule `unresolved-reference` was selected on the command line

    error[not-subscriptable]: Cannot subscript object of type `Literal[4]` with no `__getitem__` method
     --> test.py:3:7
      |
    2 | print(x)     # [unresolved-reference]
    3 | print(4[1])  # [not-subscriptable]
      |       ^^^^
      |
    info: rule `not-subscriptable` is enabled by default

    Found 2 diagnostics

    ----- stderr -----
    ");

    Ok(())
}
