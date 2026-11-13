use std::fs;

use insta::assert_snapshot;
use insta_cmd::assert_cmd_snapshot;
use ruff_python_trivia::textwrap::dedent;

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
    WARN Skipping file `has_syntax_error.py` with syntax errors
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
fn fix_unused_ignore_after_line_continuation() -> anyhow::Result<()> {
    let case = CliTest::with_file(
        "example.py",
        r"
            x = 1 \
                # ty: ignore[invalid-assignment]
            +2
            # This backslash is part of the comment \
            # type: ignore
            y = 2
            ",
    )?;

    assert_cmd_snapshot!(case.command().arg("--fix"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Found 2 diagnostics (2 fixed, 0 remaining).

    ----- stderr -----
    ");

    assert_eq!(
        fs::read_to_string(case.root().join("example.py"))?,
        dedent(
            r"
            x = 1
            +2
            # This backslash is part of the comment \
            y = 2
            "
        )
    );

    Ok(())
}

#[test]
fn fix_unfixable() -> anyhow::Result<()> {
    // Rename the TOML key and remove the ignore from `unused_ignore.py`. Only the file with a
    // syntax error should trigger a warning about skipped fixes. Shortening the key shifts the
    // Python version setting's byte offset, and the note should still point to that setting.
    let case = CliTest::with_files([
        (
            "has_syntax_error.py",
            "
            x = 1  # ty: ignore[unresolved-reference]
            x = (
            ",
        ),
        (
            "pyproject.toml",
            r#"
            [tool.ty]
            rules = { "unsound-return-statement" = 'ignore' }  # Keep this comment.
            environment.python-version = "3.12"
            "#,
        ),
        ("version_error.py", "PythonFinalizationError"),
        (
            "unused_ignore.py",
            r#"
            x = 1  # ty: ignore[unresolved-reference]
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(
        case.command().arg("--fix").arg("--warn").arg("unused-ignore-comment"),
        @r#"
    success: false
    exit_code: 1
    ----- stdout -----
    warning[unused-ignore-comment]: Unused `ty: ignore` directive
     --> has_syntax_error.py:2:8
      |
    2 | x = 1  # ty: ignore[unresolved-reference]
      |        ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    help: Remove the unused suppression comment
      |
    1 |
      - x = 1  # ty: ignore[unresolved-reference]
    2 + x = 1
    3 | x = (
      |

    error[invalid-syntax]: unexpected EOF while parsing
     --> has_syntax_error.py:4:1
      |
    4 |
      | ^

    error[unresolved-reference]: Name `PythonFinalizationError` used when not defined
     --> version_error.py:1:1
      |
    1 | PythonFinalizationError
      | ^^^^^^^^^^^^^^^^^^^^^^^
    info: `PythonFinalizationError` was added as a builtin in Python 3.13
    info: Python 3.12 was assumed when resolving types
     --> pyproject.toml:4:30
      |
    4 | environment.python-version = "3.12"
      |                              ^^^^^^ Python version configuration

    Found 5 diagnostics (2 fixed, 3 remaining).

    ----- stderr -----
    WARN Skipping file `has_syntax_error.py` with syntax errors
    "#
    );

    assert_snapshot!(fs::read_to_string(case.root().join("pyproject.toml"))?, @r#"

    [tool.ty]
    rules = { "unsound-return" = 'ignore' }  # Keep this comment.
    environment.python-version = "3.12"
    "#);

    assert_snapshot!(
        fs::read_to_string(case.root().join("unused_ignore.py"))?,
        @r"
    x = 1
    "
    );

    Ok(())
}

#[test]
fn fix_rule_rename_in_user_config_also_used_as_project_config() -> anyhow::Result<()> {
    // Explicit selection and user configuration discovery load the same path. Fix each key once.
    let case = CliTest::with_file("test.py", "import missing")?;
    let config = case.user_config_directory().join("ty/ty.toml");
    case.write_file(&config, "[rules]\nunresolved-import = 'ignore'\n")?;

    assert_cmd_snapshot!(case.command().args(["--fix", "--config-file"]).arg(&config), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Found 1 diagnostic (1 fixed, 0 remaining).

    ----- stderr -----
    ");
    assert_snapshot!(fs::read_to_string(config)?, @"
    [rules]
    import-not-found = 'ignore'
    ");
    Ok(())
}

#[cfg(unix)]
#[test]
fn skip_rule_rename_when_project_config_links_to_user_config() -> anyhow::Result<()> {
    // The symlink gives the same file two paths. Keep the warnings and skip the edit.
    let case = CliTest::with_file("test.py", "import missing")?;
    let user_config = case.user_config_directory();
    let case = case.with_filter(
        &regex::escape(&user_config.to_string_lossy()),
        "<user_config>",
    );
    let config = user_config.join("ty/ty.toml");
    let original = "[rules]\nunresolved-import = 'ignore'\n";
    case.write_file(&config, original)?;
    case.write_symlink(&config, "ty.toml")?;

    assert_cmd_snapshot!(case.command().arg("--fix"), @"
    success: false
    exit_code: 1
    ----- stdout -----
    warning[renamed-rule]: Rule `unresolved-import` has been renamed to `import-not-found`
     --> <user_config>/ty/ty.toml:2:1
      |
    2 | unresolved-import = 'ignore'
      | ^^^^^^^^^^^^^^^^^

    warning[renamed-rule]: Rule `unresolved-import` has been renamed to `import-not-found`
     --> ty.toml:2:1
      |
    2 | unresolved-import = 'ignore'
      | ^^^^^^^^^^^^^^^^^

    Found 2 diagnostics (0 fixed, 2 remaining).

    ----- stderr -----
    ");
    assert_eq!(fs::read_to_string(config)?, original);
    Ok(())
}

#[test]
fn unsafe_fix() -> anyhow::Result<()> {
    let case = CliTest::with_file("unsafe_fix.py", "raise NotImplemented")?;

    assert_cmd_snapshot!(case.command(), @"
    success: false
    exit_code: 1
    ----- stdout -----
    error[invalid-raise]: Cannot raise `NotImplemented`
     --> unsafe_fix.py:1:7
      |
    1 | raise NotImplemented
      |       ^^^^^^^^^^^^^^ Did you mean `NotImplementedError`?
    info: Can only raise an instance or subclass of `BaseException`
    help: Use `NotImplementedError` instead
      |
      - raise NotImplemented
    1 + raise NotImplementedError
      |
    note: This is an unsafe fix and may change runtime behavior

    Found 1 diagnostic

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn display_only_fix() -> anyhow::Result<()> {
    let case = CliTest::with_file("display_only_fix.py", "if (x for x in range(3)): pass")?;

    assert_cmd_snapshot!(case.command(), @"
    success: false
    exit_code: 1
    ----- stdout -----
    warning[redundant-condition]: A generator is always truthy
     --> display_only_fix.py:1:4
      |
    1 | if (x for x in range(3)): pass
      |    ^^^^^^^^^^^^^^^^^^^^^ Inferred type `GeneratorType[int, None, None]` is always truthy
    help: Did you mean to use `any()`?
      |
      - if (x for x in range(3)): pass
    1 + if any((x for x in range(3))): pass
      |
    note: This suggestion may be incorrect or produce invalid syntax. It requires manual review and cannot be applied automatically

    Found 1 diagnostic

    ----- stderr -----
    ");

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

/// `--fix` respects the baseline so users can fix new diagnostics without also modifying code
/// for existing ones. Users can apply all available fixes by running without a baseline.
#[test]
fn fix_skips_baselined_diagnostics() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "pyproject.toml",
            r"
            [tool.ty]
            baseline = 'baseline.json'
            ",
        ),
        (
            "unused_ignore.py",
            r"
            x = 1  # ty: ignore[unresolved-reference]
            ",
        ),
    ])?;
    assert_cmd_snapshot!(case.command().args(["--warn", "unused-ignore-comment", "--update-baseline"]), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Updated baseline `<temp_dir>/baseline.json` with 1 diagnostic.

    ----- stderr -----
    ");

    case.write_file(
        "unused_ignore.py",
        r"
        x = 1  # ty: ignore[unresolved-reference]
        y = 2  # ty: ignore[unresolved-reference]
        ",
    )?;
    assert_cmd_snapshot!(case.command().args(["--warn", "unused-ignore-comment", "--fix"]), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Found 1 diagnostic (1 fixed, 0 remaining).

    ----- stderr -----
    ");
    assert_snapshot!(fs::read_to_string(case.root().join("unused_ignore.py"))?, @"
    x = 1  # ty: ignore[unresolved-reference]
    y = 2
    ");
    Ok(())
}

#[test]
fn add_ignore_skips_baselined_diagnostics() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "pyproject.toml",
            r"
            [tool.ty]
            baseline = 'baseline.json'
            ",
        ),
        (
            "errors.py",
            r"
            x: int = 'wrong'
            ",
        ),
    ])?;
    assert_cmd_snapshot!(case.command().arg("--update-baseline"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    Updated baseline `<temp_dir>/baseline.json` with 1 diagnostic.

    ----- stderr -----
    ");

    case.write_file(
        "errors.py",
        r"
        x: int = 'wrong'
        print(missing)
        ",
    )?;
    assert_cmd_snapshot!(case.command().arg("--add-ignore"), @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!
    Added 1 ignore comment

    ----- stderr -----
    ");
    assert_snapshot!(fs::read_to_string(case.root().join("errors.py"))?, @r#"
    x: int = 'wrong'
    print(missing)  # ty: ignore[unresolved-reference]
    "#);
    Ok(())
}
