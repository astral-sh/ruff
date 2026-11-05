use insta_cmd::assert_cmd_snapshot;

use crate::CliTest;

#[test]
fn strict_collection_inference() -> anyhow::Result<()> {
    let case = CliTest::with_file(
        "main.py",
        r#"
        from typing_extensions import reveal_type

        reveal_type([1, "a"])
        reveal_type({"a": 1, "b": "two"})
        reveal_type({1, "a"})
        "#,
    )?;

    assert_cmd_snapshot!(
        case.command().args(["--output-format", "concise"]),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    main.py:4:13: info[revealed-type] Revealed type: `list[int | str]`
    main.py:5:13: info[revealed-type] Revealed type: `dict[str, int | str]`
    main.py:6:13: info[revealed-type] Revealed type: `set[int | str]`
    Found 3 diagnostics

    ----- stderr -----
    "
    );

    case.write_file(
        "pyproject.toml",
        r#"
        [tool.ty.analysis]
        strict-list-inference = false
        strict-dictionary-inference = false
        strict-set-inference = false
        "#,
    )?;
    assert_cmd_snapshot!(
        case.command().args(["--output-format", "concise"]),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    main.py:4:13: info[revealed-type] Revealed type: `list[Unknown]`
    main.py:5:13: info[revealed-type] Revealed type: `dict[str, Unknown]`
    main.py:6:13: info[revealed-type] Revealed type: `set[Unknown]`
    Found 3 diagnostics

    ----- stderr -----
    "
    );

    assert_cmd_snapshot!(
        case.command().args([
            "--output-format",
            "concise",
            "--config",
            "analysis.strict-list-inference=true",
            "--config",
            "analysis.strict-dictionary-inference=true",
            "--config",
            "analysis.strict-set-inference=true",
        ]),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    main.py:4:13: info[revealed-type] Revealed type: `list[int | str]`
    main.py:5:13: info[revealed-type] Revealed type: `dict[str, int | str]`
    main.py:6:13: info[revealed-type] Revealed type: `set[int | str]`
    Found 3 diagnostics

    ----- stderr -----
    "
    );

    case.write_file(
        "ty.toml",
        r#"
        [analysis]
        strict-list-inference = false
        strict-dictionary-inference = false
        strict-set-inference = false

        [[overrides]]
        include = ["main.py"]
        [overrides.analysis]
        strict-list-inference = true
        strict-set-inference = true
        "#,
    )?;
    assert_cmd_snapshot!(
        case.command().args(["--output-format", "concise"]),
        @"
    success: true
    exit_code: 0
    ----- stdout -----
    main.py:4:13: info[revealed-type] Revealed type: `list[int | str]`
    main.py:5:13: info[revealed-type] Revealed type: `dict[str, Unknown]`
    main.py:6:13: info[revealed-type] Revealed type: `set[int | str]`
    Found 3 diagnostics

    ----- stderr -----
    WARN Ignoring the `tool.ty` section in `<temp_dir>/pyproject.toml` because `<temp_dir>/ty.toml` takes precedence.
    "
    );

    Ok(())
}

/// ty ignores `type: ignore` comments when setting `respect-type-ignore-comments=false`
#[test]
fn respect_type_ignore_comments_is_turned_off() -> anyhow::Result<()> {
    let case = CliTest::with_file(
        "test.py",
        r#"
            y = a + 5  # type: ignore
            "#,
    )?;

    // Assert that there's an `unresolved-reference` diagnostic (error).
    assert_cmd_snapshot!(case.command(), @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    ");

    assert_cmd_snapshot!(case.command().arg("--config").arg("analysis.respect-type-ignore-comments=false"), @"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-reference]: Name `a` used when not defined
     --> test.py:2:5
      |
    2 | y = a + 5  # type: ignore
      |     ^

    Found 1 diagnostic

    ----- stderr -----
    ");

    Ok(())
}

/// Basic override functionality: override analysis options for a specific file
#[test]
fn overrides_basic() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "pyproject.toml",
            r#"
            [tool.ty.analysis]
            respect-type-ignore-comments = true

            [[tool.ty.overrides]]
            include = ["tests/**"]

            [tool.ty.overrides.analysis]
            respect-type-ignore-comments = false
            "#,
        ),
        (
            "main.py",
            r#"
            print(x)  # type: ignore  # ignore respected (global)
            "#,
        ),
        (
            "tests/test_main.py",
            r#"
            print(x)  # type: ignore  # ignore not-respected (override)
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command(), @"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-reference]: Name `x` used when not defined
     --> tests/test_main.py:2:7
      |
    2 | print(x)  # type: ignore  # ignore not-respected (override)
      |       ^

    Found 1 diagnostic

    ----- stderr -----
    ");

    Ok(())
}

/// Multiple overrides: later overrides take precedence
#[test]
fn overrides_precedence() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "pyproject.toml",
            r#"
            [tool.ty.analysis]
            respect-type-ignore-comments = true

            # First override: all test files
            [[tool.ty.overrides]]
            include = ["tests/**"]
            [tool.ty.overrides.analysis]
            respect-type-ignore-comments = false

            # Second override: specific test file (takes precedence)
            [[tool.ty.overrides]]
            include = ["tests/important.py"]
            [tool.ty.overrides.analysis]
            respect-type-ignore-comments = true
            "#,
        ),
        (
            "tests/test_main.py",
            r#"
            print(y)  # type: ignore (should be an error, because type ignores are disabled)
            "#,
        ),
        (
            "tests/important.py",
            r#"
            print(y)  # type: ignore (no error, because type ignores are enabled)
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command(), @"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-reference]: Name `y` used when not defined
     --> tests/test_main.py:2:7
      |
    2 | print(y)  # type: ignore (should be an error, because type ignores are disabled)
      |       ^

    Found 1 diagnostic

    ----- stderr -----
    ");

    Ok(())
}

/// Override without analysis options inherit the global analysis options
#[test]
fn overrides_inherit_global() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "pyproject.toml",
            r#"
            [tool.ty.analysis]
            respect-type-ignore-comments = false

            [[tool.ty.overrides]]
            include = ["tests/**"]

            [tool.ty.overrides.rules]
            division-by-zero = "warn"

            [tool.ty.overrides.analysis]
            "#,
        ),
        (
            "main.py",
            r#"
            print(y)  # type: ignore ignore not-respected (global)
            "#,
        ),
        (
            "tests/test_main.py",
            r#"
            print(y)  # type: ignore ignore respected (inherited from global)
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command(), @"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-reference]: Name `y` used when not defined
     --> main.py:2:7
      |
    2 | print(y)  # type: ignore ignore not-respected (global)
      |       ^

    error[unresolved-reference]: Name `y` used when not defined
     --> tests/test_main.py:2:7
      |
    2 | print(y)  # type: ignore ignore respected (inherited from global)
      |       ^

    Found 2 diagnostics

    ----- stderr -----
    ");

    Ok(())
}
