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
fn add_ignore_keeps_nested_blanket_suppression_used() -> anyhow::Result<()> {
    let case = CliTest::with_file(
        "nested.py",
        r#"
            def f(value: int) -> int:
                return value

            seen_code = True
            # ty: ignore[]
            values = [
                # ty: ignore[blanket-ignore-comment]
                # ty: ignore
                f("bad"),
                # ty: ignore
                missing,
            ]
            "#,
    )?;

    assert_cmd_snapshot!(
        case.command()
            .arg("--add-ignore")
            .arg("--warn")
            .arg("blanket-ignore-comment"),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!
    Added 1 ignore comment

    ----- stderr -----
    "
    );

    assert_snapshot!(fs::read_to_string(case.root().join("nested.py"))?, @r#"

    def f(value: int) -> int:
        return value

    seen_code = True
    # ty: ignore[blanket-ignore-comment]
    values = [
        # ty: ignore[blanket-ignore-comment]
        # ty: ignore
        f("bad"),
        # ty: ignore
        missing,
    ]
    "#);

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
    6 | reveal_type(x)  # ty: ignore[undefined-reveal]
      |             ^ `Unknown`

    error[unresolved-reference]: Name `x` used when not defined
     --> has_syntax_error.py:1:7
      |
    1 | print(x  # [unresolved-reference]
      |       ^

    error[invalid-syntax]: unexpected EOF while parsing
     --> has_syntax_error.py:1:34
      |
    1 | print(x  # [unresolved-reference]
      |                                  ^

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
            values = [
                # ty: ignore[]
                1,
            ]
            "#,
    )?;

    assert_cmd_snapshot!(
        case.command().arg("--fix").arg("--warn").arg("unused-ignore-comment"),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    Found 2 diagnostics (2 fixed, 0 remaining).

    ----- stderr -----
    "
    );

    assert_snapshot!(
        fs::read_to_string(case.root().join("unused_ignore.py"))?,
        @"

    x = 1
    values = [
        1,
    ]
    "
    );

    Ok(())
}

#[test]
fn show_unsafe_fixes() -> anyhow::Result<()> {
    let case = CliTest::with_file(
        "fixes.py",
        r#"
            from typing import TypedDict

            class Person(TypedDict):
                name: str  # ty: ignore[invalid-assignment]

            def greet(person: Person):
                print(person["Name"])
            "#,
    )?;

    assert_cmd_snapshot!(case.command().args(["--warn", "unused-ignore-comment"]), @r#"
    success: false
    exit_code: 1
    ----- stdout -----
    warning[unused-ignore-comment]: Unused `ty: ignore` directive
     --> fixes.py:5:16
      |
    5 |     name: str  # ty: ignore[invalid-assignment]
      |                ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    help: Remove the unused suppression comment
      |
    4 | class Person(TypedDict):
      -     name: str  # ty: ignore[invalid-assignment]
    5 +     name: str
    6 |
      |

    error[invalid-key]: Unknown key "Name" for TypedDict `Person`
     --> fixes.py:8:18
      |
    8 |     print(person["Name"])
      |           ------ ^^^^^^ Did you mean "name"?
      |           |
      |           TypedDict `Person`
    help: Replace with "name"
      |
    7 | def greet(person: Person):
      -     print(person["Name"])
    8 +     print(person["name"])
      |
    note: This is an unsafe fix and may change runtime behavior
    note: This fix cannot be applied automatically on the command line

    Found 2 diagnostics

    ----- stderr -----
    "#);

    assert_cmd_snapshot!(
        case.command().args(["--warn", "unused-ignore-comment", "--output-format", "concise"]),
        @r#"
    success: false
    exit_code: 1
    ----- stdout -----
    fixes.py:5:16: warning[unused-ignore-comment] Unused `ty: ignore` directive
    fixes.py:8:18: error[invalid-key] Unknown key "Name" for TypedDict `Person` - did you mean "name"?
    Found 2 diagnostics

    ----- stderr -----
    "#
    );

    assert_cmd_snapshot!(case.command().args(["--warn", "unused-ignore-comment", "--fix"]), @r#"
    success: false
    exit_code: 1
    ----- stdout -----
    error[invalid-key]: Unknown key "Name" for TypedDict `Person`
     --> fixes.py:8:18
      |
    8 |     print(person["Name"])
      |           ------ ^^^^^^ Did you mean "name"?
      |           |
      |           TypedDict `Person`
    help: Replace with "name"
      |
    7 | def greet(person: Person):
      -     print(person["Name"])
    8 +     print(person["name"])
      |
    note: This is an unsafe fix and may change runtime behavior
    note: This fix cannot be applied automatically on the command line

    Found 2 diagnostics (1 fixed, 1 remaining).

    ----- stderr -----
    "#);

    assert_snapshot!(fs::read_to_string(case.root().join("fixes.py"))?, @r#"

    from typing import TypedDict

    class Person(TypedDict):
        name: str

    def greet(person: Person):
        print(person["Name"])
    "#);

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
        @"
    success: false
    exit_code: 1
    ----- stdout -----
    error[invalid-syntax]: unexpected EOF while parsing
     --> has_syntax_error.py:2:1
      |
    2 |
      | ^

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
