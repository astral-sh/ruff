use insta_cmd::assert_cmd_snapshot;

use crate::CliTest;

#[test]
fn only_warnings() -> anyhow::Result<()> {
    let case = CliTest::with_file("test.py", r"print(x)  # [unresolved-reference]")?;

    assert_cmd_snapshot!(case.command().arg("--warn").arg("unresolved-reference"), @"
    success: false
    exit_code: 1
    ----- stdout -----
    warning[unresolved-reference]: Name `x` used when not defined
     --> test.py:1:7
      |
    1 | print(x)  # [unresolved-reference]
      |       ^
      |

    Found 1 diagnostic

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn only_warnings_and_exit_zero_on_warning() -> anyhow::Result<()> {
    let case = CliTest::with_file("test.py", r"print(x)  # [unresolved-reference]")?;

    let output = case
        .command()
        .arg("--exit-zero-on-warning")
        .arg("--warn")
        .arg("unresolved-reference")
        .output()?;

    assert!(
        output.status.success(),
        "`--exit-zero-on-warning` failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    Ok(())
}

#[test]
fn error_on_warning_conflicts_with_exit_zero_on_warning() -> anyhow::Result<()> {
    let case = CliTest::with_file("test.py", "")?
        .with_filter(r"Usage: ty(?:\.exe)? check", "Usage: ty check");

    assert_cmd_snapshot!(case.command().arg("--error-on-warning").arg("--exit-zero-on-warning"), @"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: the argument '--error-on-warning' cannot be used with '--exit-zero-on-warning'

    Usage: ty check --error-on-warning [PATH]...

    For more information, try '--help'.
    ");

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

    assert_cmd_snapshot!(case.command(), @"
    success: true
    exit_code: 0
    ----- stdout -----
    info[revealed-type]: Revealed type
     --> test.py:3:13
      |
    3 | reveal_type(1)
      |             ^ `Literal[1]`
      |

    Found 1 diagnostic

    ----- stderr -----
    ");

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

    assert_cmd_snapshot!(case.command().arg("--error-on-warning").arg("test.py"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    info[revealed-type]: Revealed type
     --> test.py:3:13
      |
    3 | reveal_type(1)
      |             ^ `Literal[1]`
      |

    Found 1 diagnostic

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn only_warnings_and_error_on_warning_overrides_configuration() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        ("test.py", r"print(x)  # [unresolved-reference]"),
        (
            "ty.toml",
            r#"
            [terminal]
            error-on-warning = false
        "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command().arg("--error-on-warning").arg("--warn").arg("unresolved-reference"), @"
    success: false
    exit_code: 1
    ----- stdout -----
    warning[unresolved-reference]: Name `x` used when not defined
     --> test.py:1:7
      |
    1 | print(x)  # [unresolved-reference]
      |       ^
      |

    Found 1 diagnostic

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn only_warnings_and_error_on_warning_is_disabled_in_configuration() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        ("test.py", r"print(x)  # [unresolved-reference]"),
        (
            "ty.toml",
            r#"
            [terminal]
            error-on-warning = false
        "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command().arg("--warn").arg("unresolved-reference"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    warning[unresolved-reference]: Name `x` used when not defined
     --> test.py:1:7
      |
    1 | print(x)  # [unresolved-reference]
      |       ^
      |

    Found 1 diagnostic

    ----- stderr -----
    ");

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

    assert_cmd_snapshot!(case.command().arg("--warn").arg("unresolved-reference"), @"
    success: false
    exit_code: 1
    ----- stdout -----
    warning[unresolved-reference]: Name `x` used when not defined
     --> test.py:2:7
      |
    2 | print(x)     # [unresolved-reference]
      |       ^
      |

    error[not-subscriptable]: Cannot subscript object of type `Literal[4]` with no `__getitem__` method
     --> test.py:3:7
      |
    3 | print(4[1])  # [not-subscriptable]
      |       ^^^^
      |

    Found 2 diagnostics

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn both_warnings_and_errors_and_exit_zero_on_warning() -> anyhow::Result<()> {
    let case = CliTest::with_file(
        "test.py",
        r###"
        print(x)     # [unresolved-reference]
        print(4[1])  # [not-subscriptable]
        "###,
    )?;

    assert_cmd_snapshot!(case.command().arg("--warn").arg("unresolved-reference").arg("--exit-zero-on-warning"), @"
    success: false
    exit_code: 1
    ----- stdout -----
    warning[unresolved-reference]: Name `x` used when not defined
     --> test.py:2:7
      |
    2 | print(x)     # [unresolved-reference]
      |       ^
      |

    error[not-subscriptable]: Cannot subscript object of type `Literal[4]` with no `__getitem__` method
     --> test.py:3:7
      |
    3 | print(4[1])  # [not-subscriptable]
      |       ^^^^
      |

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

    assert_cmd_snapshot!(case.command().arg("--exit-zero").arg("--warn").arg("unresolved-reference"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    warning[unresolved-reference]: Name `x` used when not defined
     --> test.py:2:7
      |
    2 | print(x)     # [unresolved-reference]
      |       ^
      |

    error[not-subscriptable]: Cannot subscript object of type `Literal[4]` with no `__getitem__` method
     --> test.py:3:7
      |
    3 | print(4[1])  # [not-subscriptable]
      |       ^^^^
      |

    Found 2 diagnostics

    ----- stderr -----
    ");

    Ok(())
}
