use std::fs;

use insta::assert_snapshot;
use insta_cmd::assert_cmd_snapshot;

use crate::CliTest;

#[test]
fn add_ignore() -> anyhow::Result<()> {
    let case = CliTest::with_file(
        "different_violations.py",
        r#"
            import sys

            x = 1 + a

            if sys.does_not_exist:
                ...

            def test(a, b): ...

            test(x = 10, b = 12)
            "#,
    )?;

    assert_cmd_snapshot!(case.command().arg("--add-ignore"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!
    Added 4 ignore comments

    ----- stderr -----
    ");

    // There should be no diagnostics when running ty again
    assert_cmd_snapshot!(case.command(), @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn add_ignore_unfixable() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        ("has_syntax_error.py", r"print(x  # [unresolved-reference]"),
        (
            "different_violations.py",
            r#"
            import sys

            x = 1 + a

            reveal_type(x)

            if sys.does_not_exist:
                ...
            "#,
        ),
        (
            "repeated_violations.py",
            r#"
            x = (
                1 +
                a * b
            )

            y = y  # ty: ignore[unresolved-reference]
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command().arg("--add-ignore").env("RUST_BACKTRACE", "1"), @"
    success: false
    exit_code: 1
    ----- stdout -----
    info[revealed-type]: Revealed type
     --> different_violations.py:6:13
      |
    4 | x = 1 + a  # ty:ignore[unresolved-reference]
    5 |
    6 | reveal_type(x)  # ty:ignore[undefined-reveal]
      |             ^ `Unknown`
    7 |
    8 | if sys.does_not_exist:  # ty:ignore[unresolved-attribute]
      |

    error[unresolved-reference]: Name `x` used when not defined
     --> has_syntax_error.py:1:7
      |
    1 | print(x  # [unresolved-reference]
      |       ^
      |

    error[invalid-syntax]: unexpected EOF while parsing
     --> has_syntax_error.py:1:34
      |
    1 | print(x  # [unresolved-reference]
      |                                  ^
      |

    Found 3 diagnostics
    Added 5 ignore comments

    ----- stderr -----
    WARN Skipping file `<temp_dir>/has_syntax_error.py` with syntax errors
    ");

    Ok(())
}

#[test]
fn fix() -> anyhow::Result<()> {
    let case = CliTest::with_file(
        "unused_ignore.py",
        r#"
            x = 1  # ty: ignore[unresolved-reference]
            "#,
    )?;

    assert_cmd_snapshot!(
        case.command().arg("--fix").arg("--warn").arg("unused-ignore-comment"),
        @r"
    success: true
    exit_code: 0
    ----- stdout -----
    Found 1 diagnostic (1 fixed, 0 remaining).

    ----- stderr -----
    "
    );

    assert_snapshot!(
        fs::read_to_string(case.root().join("unused_ignore.py"))?,
        @r"
    x = 1
    "
    );

    Ok(())
}

#[test]
fn fix_unfixable() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        ("has_syntax_error.py", "x = (\n"),
        (
            "unused_ignore.py",
            r#"
            x = 1  # ty: ignore[unresolved-reference]
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(
        case.command().arg("--fix").arg("--warn").arg("unused-ignore-comment"),
        @r"
    success: false
    exit_code: 1
    ----- stdout -----
    error[invalid-syntax]: unexpected EOF while parsing
     --> has_syntax_error.py:2:1
      |
    1 | x = (
      |      ^
      |

    Found 2 diagnostics (1 fixed, 1 remaining).

    ----- stderr -----
    WARN Skipping file `<temp_dir>/has_syntax_error.py` with syntax errors
    "
    );

    assert_snapshot!(
        fs::read_to_string(case.root().join("unused_ignore.py"))?,
        @r"
    x = 1
    "
    );

    Ok(())
}

#[test]
fn fix_clean_file() -> anyhow::Result<()> {
    let case = CliTest::with_file(
        "clean.py",
        r#"
            x = 1
            "#,
    )?;

    assert_cmd_snapshot!(case.command().arg("--fix"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    ");

    Ok(())
}
