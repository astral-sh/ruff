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
    "###);

    case.write_file(
        "pyproject.toml",
        r#"
        [tool.ty.rules]
        division-by-zero = "warn" # promote to warn
        unresolved-reference = "ignore"
    "#,
    )?;

    assert_cmd_snapshot!(case.command(), @r###"
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
    "###);

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
    info: Searched in the following paths during module resolution:
    info:   1. <temp_dir>/ (first-party code)
    info:   2. vendored://stdlib (stdlib typeshed stubs vendored by ty)
    info: make sure your Python environment is properly configured: https://docs.astral.sh/ty/modules/#python-environment
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
        @r###"
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
    info: Searched in the following paths during module resolution:
    info:   1. <temp_dir>/ (first-party code)
    info:   2. vendored://stdlib (stdlib typeshed stubs vendored by ty)
    info: make sure your Python environment is properly configured: https://docs.astral.sh/ty/modules/#python-environment
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
    "###
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
        @r###"
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
    "###
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
    warning[unknown-rule]: Unknown rule `division-by-zer`. Did you mean `division-by-zero`?
     --> pyproject.toml:3:1
      |
    2 | [tool.ty.rules]
    3 | division-by-zer = "warn" # incorrect rule name
      | ^^^^^^^^^^^^^^^
      |

    Found 1 diagnostic

    ----- stderr -----
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
    warning[unknown-rule]: Unknown rule `division-by-zer`. Did you mean `division-by-zero`?

    Found 1 diagnostic

    ----- stderr -----
    ");

    Ok(())
}

/// Basic override functionality: override rules for specific files
#[test]
fn overrides_basic() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "pyproject.toml",
            r#"
            [tool.ty.rules]
            division-by-zero = "error"
            unresolved-reference = "error"

            [[tool.ty.overrides]]
            include = ["tests/**"]

            [tool.ty.overrides.rules]
            division-by-zero = "warn"
            unresolved-reference = "ignore"
            "#,
        ),
        (
            "main.py",
            r#"
            y = 4 / 0  # division-by-zero: error (global)
            x = 1
            prin(x)    # unresolved-reference: error (global)
            "#,
        ),
        (
            "tests/test_main.py",
            r#"
            y = 4 / 0  # division-by-zero: warn (override)
            x = 1
            prin(x)    # unresolved-reference: ignore (override)
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    error[division-by-zero]: Cannot divide object of type `Literal[4]` by zero
     --> main.py:2:5
      |
    2 | y = 4 / 0  # division-by-zero: error (global)
      |     ^^^^^
    3 | x = 1
    4 | prin(x)    # unresolved-reference: error (global)
      |
    info: rule `division-by-zero` was selected in the configuration file

    error[unresolved-reference]: Name `prin` used when not defined
     --> main.py:4:1
      |
    2 | y = 4 / 0  # division-by-zero: error (global)
    3 | x = 1
    4 | prin(x)    # unresolved-reference: error (global)
      | ^^^^
      |
    info: rule `unresolved-reference` was selected in the configuration file

    warning[division-by-zero]: Cannot divide object of type `Literal[4]` by zero
     --> tests/test_main.py:2:5
      |
    2 | y = 4 / 0  # division-by-zero: warn (override)
      |     ^^^^^
    3 | x = 1
    4 | prin(x)    # unresolved-reference: ignore (override)
      |
    info: rule `division-by-zero` was selected in the configuration file

    Found 3 diagnostics

    ----- stderr -----
    "###);

    Ok(())
}

/// Multiple overrides: later overrides take precedence
#[test]
fn overrides_precedence() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "pyproject.toml",
            r#"
            [tool.ty.rules]
            division-by-zero = "error"

            # First override: all test files
            [[tool.ty.overrides]]
            include = ["tests/**"]
            [tool.ty.overrides.rules]
            division-by-zero = "warn"

            # Second override: specific test file (takes precedence)
            [[tool.ty.overrides]]
            include = ["tests/important.py"]
            [tool.ty.overrides.rules]
            division-by-zero = "ignore"
            "#,
        ),
        (
            "tests/test_main.py",
            r#"
            y = 4 / 0  # division-by-zero: warn (first override)
            "#,
        ),
        (
            "tests/important.py",
            r#"
            y = 4 / 0  # division-by-zero: ignore (second override)
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    warning[division-by-zero]: Cannot divide object of type `Literal[4]` by zero
     --> tests/test_main.py:2:5
      |
    2 | y = 4 / 0  # division-by-zero: warn (first override)
      |     ^^^^^
      |
    info: rule `division-by-zero` was selected in the configuration file

    Found 1 diagnostic

    ----- stderr -----
    "###);

    Ok(())
}

/// Override with exclude patterns
#[test]
fn overrides_exclude() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "pyproject.toml",
            r#"
            [tool.ty.rules]
            division-by-zero = "error"

            [[tool.ty.overrides]]
            include = ["tests/**"]
            exclude = ["tests/important.py"]
            [tool.ty.overrides.rules]
            division-by-zero = "warn"
            "#,
        ),
        (
            "tests/test_main.py",
            r#"
            y = 4 / 0  # division-by-zero: warn (override applies)
            "#,
        ),
        (
            "tests/important.py",
            r#"
            y = 4 / 0  # division-by-zero: error (override excluded)
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    error[division-by-zero]: Cannot divide object of type `Literal[4]` by zero
     --> tests/important.py:2:5
      |
    2 | y = 4 / 0  # division-by-zero: error (override excluded)
      |     ^^^^^
      |
    info: rule `division-by-zero` was selected in the configuration file

    warning[division-by-zero]: Cannot divide object of type `Literal[4]` by zero
     --> tests/test_main.py:2:5
      |
    2 | y = 4 / 0  # division-by-zero: warn (override applies)
      |     ^^^^^
      |
    info: rule `division-by-zero` was selected in the configuration file

    Found 2 diagnostics

    ----- stderr -----
    "###);

    Ok(())
}

/// Override without rules inherits global rules
#[test]
fn overrides_inherit_global() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "pyproject.toml",
            r#"
            [tool.ty.rules]
            division-by-zero = "warn"
            unresolved-reference = "error"

            [[tool.ty.overrides]]
            include = ["tests/**"]

            [tool.ty.overrides.rules]
            # Override only division-by-zero, unresolved-reference should inherit from global
            division-by-zero = "ignore"
            "#,
        ),
        (
            "main.py",
            r#"
            y = 4 / 0  # division-by-zero: warn (global)
            prin(y)    # unresolved-reference: error (global)
            "#,
        ),
        (
            "tests/test_main.py",
            r#"
            y = 4 / 0  # division-by-zero: ignore (overridden)
            prin(y)    # unresolved-reference: error (inherited from global)
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    warning[division-by-zero]: Cannot divide object of type `Literal[4]` by zero
     --> main.py:2:5
      |
    2 | y = 4 / 0  # division-by-zero: warn (global)
      |     ^^^^^
    3 | prin(y)    # unresolved-reference: error (global)
      |
    info: rule `division-by-zero` was selected in the configuration file

    error[unresolved-reference]: Name `prin` used when not defined
     --> main.py:3:1
      |
    2 | y = 4 / 0  # division-by-zero: warn (global)
    3 | prin(y)    # unresolved-reference: error (global)
      | ^^^^
      |
    info: rule `unresolved-reference` was selected in the configuration file

    error[unresolved-reference]: Name `prin` used when not defined
     --> tests/test_main.py:3:1
      |
    2 | y = 4 / 0  # division-by-zero: ignore (overridden)
    3 | prin(y)    # unresolved-reference: error (inherited from global)
      | ^^^^
      |
    info: rule `unresolved-reference` was selected in the configuration file

    Found 3 diagnostics

    ----- stderr -----
    "###);

    Ok(())
}

/// ty warns about invalid glob patterns in override include patterns
#[test]
fn overrides_invalid_include_glob() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "pyproject.toml",
            r#"
            [tool.ty.rules]
            division-by-zero = "error"

            [[tool.ty.overrides]]
            include = ["tests/[invalid"]  # Invalid glob: unclosed bracket
            [tool.ty.overrides.rules]
            division-by-zero = "warn"
            "#,
        ),
        (
            "test.py",
            r#"
            y = 4 / 0
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command(), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    ty failed
      Cause: error[invalid-glob]: Invalid include pattern
     --> pyproject.toml:6:12
      |
    5 | [[tool.ty.overrides]]
    6 | include = ["tests/[invalid"]  # Invalid glob: unclosed bracket
      |            ^^^^^^^^^^^^^^^^ unclosed character class; missing ']'
    7 | [tool.ty.overrides.rules]
    8 | division-by-zero = "warn"
      |
    "###);

    Ok(())
}

/// ty warns about invalid glob patterns in override exclude patterns
#[test]
fn overrides_invalid_exclude_glob() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "pyproject.toml",
            r#"
            [tool.ty.rules]
            division-by-zero = "error"

            [[tool.ty.overrides]]
            include = ["tests/**"]
            exclude = ["***/invalid"]     # Invalid glob: triple asterisk
            [tool.ty.overrides.rules]
            division-by-zero = "warn"
            "#,
        ),
        (
            "test.py",
            r#"
            y = 4 / 0
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command(), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    ty failed
      Cause: error[invalid-glob]: Invalid exclude pattern
     --> pyproject.toml:7:12
      |
    5 | [[tool.ty.overrides]]
    6 | include = ["tests/**"]
    7 | exclude = ["***/invalid"]     # Invalid glob: triple asterisk
      |            ^^^^^^^^^^^^^ Too many stars at position 1
    8 | [tool.ty.overrides.rules]
    9 | division-by-zero = "warn"
      |
    "###);

    Ok(())
}

/// ty warns when an overrides section has neither include nor exclude
#[test]
fn overrides_missing_include_exclude() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "pyproject.toml",
            r#"
            [tool.ty.rules]
            division-by-zero = "error"

            [[tool.ty.overrides]]
            # Missing both include and exclude - should warn
            [tool.ty.overrides.rules]
            division-by-zero = "warn"
            "#,
        ),
        (
            "test.py",
            r#"
            y = 4 / 0
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    warning[unnecessary-overrides-section]: Unnecessary `overrides` section
     --> pyproject.toml:5:1
      |
    3 | division-by-zero = "error"
    4 |
    5 | [[tool.ty.overrides]]
      | ^^^^^^^^^^^^^^^^^^^^^ This overrides section applies to all files
    6 | # Missing both include and exclude - should warn
    7 | [tool.ty.overrides.rules]
      |
    info: It has no `include` or `exclude` option restricting the files
    info: Restrict the files by adding a pattern to `include` or `exclude`...
    info: or remove the `[[overrides]]` section and merge the configuration into the root `[rules]` table if the configuration should apply to all files

    warning[division-by-zero]: Cannot divide object of type `Literal[4]` by zero
     --> test.py:2:5
      |
    2 | y = 4 / 0
      |     ^^^^^
      |
    info: rule `division-by-zero` was selected in the configuration file

    Found 2 diagnostics

    ----- stderr -----
    "###);

    Ok(())
}

/// ty warns when an overrides section has an empty include array
#[test]
fn overrides_empty_include() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "pyproject.toml",
            r#"
            [tool.ty.rules]
            division-by-zero = "error"

            [[tool.ty.overrides]]
            include = []  # Empty include - won't match any files
            [tool.ty.overrides.rules]
            division-by-zero = "warn"
            "#,
        ),
        (
            "test.py",
            r#"
            y = 4 / 0
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    warning[empty-include]: Empty include matches no files
     --> pyproject.toml:6:11
      |
    5 | [[tool.ty.overrides]]
    6 | include = []  # Empty include - won't match any files
      |           ^^ This `include` list is empty
    7 | [tool.ty.overrides.rules]
    8 | division-by-zero = "warn"
      |
    info: Remove the `include` option to match all files or add a pattern to match specific files

    error[division-by-zero]: Cannot divide object of type `Literal[4]` by zero
     --> test.py:2:5
      |
    2 | y = 4 / 0
      |     ^^^^^
      |
    info: rule `division-by-zero` was selected in the configuration file

    Found 2 diagnostics

    ----- stderr -----
    "###);

    Ok(())
}

/// ty warns when an overrides section has no actual overrides
#[test]
fn overrides_no_actual_overrides() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "pyproject.toml",
            r#"
            [tool.ty.rules]
            division-by-zero = "error"

            [[tool.ty.overrides]]
            include = ["*.py"]  # Has patterns but no rule overrides
            # Missing [tool.ty.overrides.rules] section entirely
            "#,
        ),
        (
            "test.py",
            r#"
            y = 4 / 0
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command(), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    warning[useless-overrides-section]: Useless `overrides` section
     --> pyproject.toml:5:1
      |
    3 | division-by-zero = "error"
    4 |
    5 | [[tool.ty.overrides]]
      | ^^^^^^^^^^^^^^^^^^^^^ This overrides section configures no rules
    6 | include = ["*.py"]  # Has patterns but no rule overrides
    7 | # Missing [tool.ty.overrides.rules] section entirely
      |
    info: It has no `rules` table
    info: Add a `[overrides.rules]` table...
    info: or remove the `[[overrides]]` section if there's nothing to override

    error[division-by-zero]: Cannot divide object of type `Literal[4]` by zero
     --> test.py:2:5
      |
    2 | y = 4 / 0
      |     ^^^^^
      |
    info: rule `division-by-zero` was selected in the configuration file

    Found 2 diagnostics

    ----- stderr -----
    "###);

    Ok(())
}

/// ty warns about unknown rules specified in an overrides section
#[test]
fn overrides_unknown_rules() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "pyproject.toml",
            r#"
            [tool.ty.rules]
            division-by-zero = "error"

            [[tool.ty.overrides]]
            include = ["tests/**"]

            [tool.ty.overrides.rules]
            division-by-zero = "warn"
            division-by-zer = "error"  # incorrect rule name
            "#,
        ),
        (
            "main.py",
            r#"
            y = 4 / 0
            "#,
        ),
        (
            "tests/test_main.py",
            r#"
            y = 4 / 0
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command(), @r#"
    success: false
    exit_code: 1
    ----- stdout -----
    error[division-by-zero]: Cannot divide object of type `Literal[4]` by zero
     --> main.py:2:5
      |
    2 | y = 4 / 0
      |     ^^^^^
      |
    info: rule `division-by-zero` was selected in the configuration file

    warning[unknown-rule]: Unknown rule `division-by-zer`. Did you mean `division-by-zero`?
      --> pyproject.toml:10:1
       |
     8 | [tool.ty.overrides.rules]
     9 | division-by-zero = "warn"
    10 | division-by-zer = "error"  # incorrect rule name
       | ^^^^^^^^^^^^^^^
       |

    warning[division-by-zero]: Cannot divide object of type `Literal[4]` by zero
     --> tests/test_main.py:2:5
      |
    2 | y = 4 / 0
      |     ^^^^^
      |
    info: rule `division-by-zero` was selected in the configuration file

    Found 3 diagnostics

    ----- stderr -----
    "#);

    Ok(())
}
