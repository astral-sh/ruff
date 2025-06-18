use insta_cmd::assert_cmd_snapshot;

use crate::CliTest;

/// Test exclude CLI argument functionality
#[test]
fn exclude_argument() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "src/main.py",
            r#"
            print(undefined_var)  # error: unresolved-reference
            "#,
        ),
        (
            "tests/test_main.py",
            r#"
            print(another_undefined_var)  # error: unresolved-reference
            "#,
        ),
        (
            "temp_file.py",
            r#"
            print(temp_undefined_var)  # error: unresolved-reference
            "#,
        ),
    ])?;

    // Test that exclude argument is recognized and works
    assert_cmd_snapshot!(case.command().arg("--exclude").arg("tests/"), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-reference]: Name `undefined_var` used when not defined
     --> src/main.py:2:7
      |
    2 | print(undefined_var)  # error: unresolved-reference
      |       ^^^^^^^^^^^^^
      |
    info: rule `unresolved-reference` is enabled by default

    error[unresolved-reference]: Name `temp_undefined_var` used when not defined
     --> temp_file.py:2:7
      |
    2 | print(temp_undefined_var)  # error: unresolved-reference
      |       ^^^^^^^^^^^^^^^^^^
      |
    info: rule `unresolved-reference` is enabled by default

    Found 2 diagnostics

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

    // Test multiple exclude patterns
    assert_cmd_snapshot!(case.command().arg("--exclude").arg("tests/").arg("--exclude").arg("temp_*.py"), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-reference]: Name `undefined_var` used when not defined
     --> src/main.py:2:7
      |
    2 | print(undefined_var)  # error: unresolved-reference
      |       ^^^^^^^^^^^^^
      |
    info: rule `unresolved-reference` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

    Ok(())
}

/// Test configuration file include functionality
#[test]
fn configuration_include() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "src/main.py",
            r#"
            print(undefined_var)  # error: unresolved-reference
            "#,
        ),
        (
            "tests/test_main.py",
            r#"
            print(another_undefined_var)  # error: unresolved-reference
            "#,
        ),
        (
            "other.py",
            r#"
            print(other_undefined_var)  # error: unresolved-reference
            "#,
        ),
    ])?;

    // Test include via configuration - should only check included files
    case.write_file(
        "ty.toml",
        r#"
        [src]
        include = ["src"]
        "#,
    )?;

    assert_cmd_snapshot!(case.command(), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-reference]: Name `undefined_var` used when not defined
     --> src/main.py:2:7
      |
    2 | print(undefined_var)  # error: unresolved-reference
      |       ^^^^^^^^^^^^^
      |
    info: rule `unresolved-reference` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

    // Test multiple include patterns via configuration
    case.write_file(
        "ty.toml",
        r#"
        [src]
        include = ["src", "other.py"]
        "#,
    )?;

    assert_cmd_snapshot!(case.command(), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-reference]: Name `other_undefined_var` used when not defined
     --> other.py:2:7
      |
    2 | print(other_undefined_var)  # error: unresolved-reference
      |       ^^^^^^^^^^^^^^^^^^^
      |
    info: rule `unresolved-reference` is enabled by default

    error[unresolved-reference]: Name `undefined_var` used when not defined
     --> src/main.py:2:7
      |
    2 | print(undefined_var)  # error: unresolved-reference
      |       ^^^^^^^^^^^^^
      |
    info: rule `unresolved-reference` is enabled by default

    Found 2 diagnostics

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

    Ok(())
}

/// Test configuration file exclude functionality
#[test]
fn configuration_exclude() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "src/main.py",
            r#"
            print(undefined_var)  # error: unresolved-reference
            "#,
        ),
        (
            "tests/test_main.py",
            r#"
            print(another_undefined_var)  # error: unresolved-reference
            "#,
        ),
        (
            "temp_file.py",
            r#"
            print(temp_undefined_var)  # error: unresolved-reference
            "#,
        ),
    ])?;

    // Test exclude via configuration
    case.write_file(
        "ty.toml",
        r#"
        [src]
        exclude = ["tests/"]
        "#,
    )?;

    assert_cmd_snapshot!(case.command(), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-reference]: Name `undefined_var` used when not defined
     --> src/main.py:2:7
      |
    2 | print(undefined_var)  # error: unresolved-reference
      |       ^^^^^^^^^^^^^
      |
    info: rule `unresolved-reference` is enabled by default

    error[unresolved-reference]: Name `temp_undefined_var` used when not defined
     --> temp_file.py:2:7
      |
    2 | print(temp_undefined_var)  # error: unresolved-reference
      |       ^^^^^^^^^^^^^^^^^^
      |
    info: rule `unresolved-reference` is enabled by default

    Found 2 diagnostics

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

    // Test multiple exclude patterns via configuration
    case.write_file(
        "ty.toml",
        r#"
        [src]
        exclude = ["tests/", "temp_*.py"]
        "#,
    )?;

    assert_cmd_snapshot!(case.command(), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-reference]: Name `undefined_var` used when not defined
     --> src/main.py:2:7
      |
    2 | print(undefined_var)  # error: unresolved-reference
      |       ^^^^^^^^^^^^^
      |
    info: rule `unresolved-reference` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

    Ok(())
}

/// Test that exclude takes precedence over include in configuration
#[test]
fn exclude_precedence_over_include() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "src/main.py",
            r#"
            print(undefined_var)  # error: unresolved-reference
            "#,
        ),
        (
            "src/test_helper.py",
            r#"
            print(helper_undefined_var)  # error: unresolved-reference
            "#,
        ),
        (
            "other.py",
            r#"
            print(other_undefined_var)  # error: unresolved-reference
            "#,
        ),
    ])?;

    // Include all src files but exclude test files - exclude should win
    case.write_file(
        "ty.toml",
        r#"
        [src]
        include = ["src"]
        exclude = ["**/test_*.py"]
        "#,
    )?;

    assert_cmd_snapshot!(case.command(), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-reference]: Name `undefined_var` used when not defined
     --> src/main.py:2:7
      |
    2 | print(undefined_var)  # error: unresolved-reference
      |       ^^^^^^^^^^^^^
      |
    info: rule `unresolved-reference` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

    Ok(())
}

/// Test that CLI exclude overrides configuration include
#[test]
fn exclude_argument_precedence_include_argument() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "src/main.py",
            r#"
            print(undefined_var)  # error: unresolved-reference
            "#,
        ),
        (
            "tests/test_main.py",
            r#"
            print(another_undefined_var)  # error: unresolved-reference
            "#,
        ),
        (
            "other.py",
            r#"
            print(other_undefined_var)  # error: unresolved-reference
            "#,
        ),
    ])?;

    // Configuration includes all files, but CLI excludes tests
    case.write_file(
        "ty.toml",
        r#"
        [src]
        include = ["src/", "tests/"]
        "#,
    )?;

    assert_cmd_snapshot!(case.command().arg("--exclude").arg("tests/"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-reference]: Name `undefined_var` used when not defined
     --> src/main.py:2:7
      |
    2 | print(undefined_var)  # error: unresolved-reference
      |       ^^^^^^^^^^^^^
      |
    info: rule `unresolved-reference` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    "###);

    Ok(())
}

/// Test that default excludes can be removed using negated patterns
#[test]
fn remove_default_exclude() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "src/main.py",
            r#"
            print(undefined_var)  # error: unresolved-reference
            "#,
        ),
        (
            "dist/generated.py",
            r#"
            print(another_undefined_var)  # error: unresolved-reference
            "#,
        ),
    ])?;

    // By default, 'dist' directory should be excluded (see default excludes)
    assert_cmd_snapshot!(case.command(), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-reference]: Name `undefined_var` used when not defined
     --> src/main.py:2:7
      |
    2 | print(undefined_var)  # error: unresolved-reference
      |       ^^^^^^^^^^^^^
      |
    info: rule `unresolved-reference` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

    // Now override the default exclude by using a negated pattern to re-include 'dist'
    case.write_file(
        "ty.toml",
        r#"
        [src]
        exclude = ["!**/dist/"]
        "#,
    )?;

    assert_cmd_snapshot!(case.command(), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-reference]: Name `another_undefined_var` used when not defined
     --> dist/generated.py:2:7
      |
    2 | print(another_undefined_var)  # error: unresolved-reference
      |       ^^^^^^^^^^^^^^^^^^^^^
      |
    info: rule `unresolved-reference` is enabled by default

    error[unresolved-reference]: Name `undefined_var` used when not defined
     --> src/main.py:2:7
      |
    2 | print(undefined_var)  # error: unresolved-reference
      |       ^^^^^^^^^^^^^
      |
    info: rule `unresolved-reference` is enabled by default

    Found 2 diagnostics

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

    Ok(())
}

/// Test that configuration excludes can be removed via CLI negation
#[test]
fn cli_removes_config_exclude() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "src/main.py",
            r#"
            print(undefined_var)  # error: unresolved-reference
            "#,
        ),
        (
            "build/output.py",
            r#"
            print(build_undefined_var)  # error: unresolved-reference
            "#,
        ),
    ])?;

    // Configuration excludes the build directory
    case.write_file(
        "ty.toml",
        r#"
        [src]
        exclude = ["build/"]
        "#,
    )?;

    // Verify that build/ is excluded by configuration
    assert_cmd_snapshot!(case.command(), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-reference]: Name `undefined_var` used when not defined
     --> src/main.py:2:7
      |
    2 | print(undefined_var)  # error: unresolved-reference
      |       ^^^^^^^^^^^^^
      |
    info: rule `unresolved-reference` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

    // Now remove the configuration exclude via CLI negation
    assert_cmd_snapshot!(case.command().arg("--exclude").arg("!build/"), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-reference]: Name `build_undefined_var` used when not defined
     --> build/output.py:2:7
      |
    2 | print(build_undefined_var)  # error: unresolved-reference
      |       ^^^^^^^^^^^^^^^^^^^
      |
    info: rule `unresolved-reference` is enabled by default

    error[unresolved-reference]: Name `undefined_var` used when not defined
     --> src/main.py:2:7
      |
    2 | print(undefined_var)  # error: unresolved-reference
      |       ^^^^^^^^^^^^^
      |
    info: rule `unresolved-reference` is enabled by default

    Found 2 diagnostics

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

    Ok(())
}

/// Test behavior when explicitly checking a path that matches an exclude pattern
#[test]
fn explicit_path_overrides_exclude() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "src/main.py",
            r#"
            print(undefined_var)  # error: unresolved-reference
            "#,
        ),
        (
            "tests/generated.py",
            r#"
            print(dist_undefined_var)  # error: unresolved-reference
            "#,
        ),
        (
            "dist/other.py",
            r#"
            print(other_undefined_var)  # error: unresolved-reference
            "#,
        ),
        (
            "ty.toml",
            r#"
            [src]
            exclude = ["tests/generated.py"]
            "#,
        ),
    ])?;

    // dist is excluded by default and `tests/generated` is excluded in the project, so only src/main.py should be checked
    assert_cmd_snapshot!(case.command(), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-reference]: Name `undefined_var` used when not defined
     --> src/main.py:2:7
      |
    2 | print(undefined_var)  # error: unresolved-reference
      |       ^^^^^^^^^^^^^
      |
    info: rule `unresolved-reference` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

    // Explicitly checking a file in an excluded directory should still check that file
    assert_cmd_snapshot!(case.command().arg("tests/generated.py"), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-reference]: Name `dist_undefined_var` used when not defined
     --> tests/generated.py:2:7
      |
    2 | print(dist_undefined_var)  # error: unresolved-reference
      |       ^^^^^^^^^^^^^^^^^^
      |
    info: rule `unresolved-reference` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

    // Explicitly checking the entire excluded directory should check all files in it
    assert_cmd_snapshot!(case.command().arg("dist/"), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-reference]: Name `other_undefined_var` used when not defined
     --> dist/other.py:2:7
      |
    2 | print(other_undefined_var)  # error: unresolved-reference
      |       ^^^^^^^^^^^^^^^^^^^
      |
    info: rule `unresolved-reference` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ");

    Ok(())
}

#[test]
fn invalid_include_pattern() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "src/main.py",
            r#"
            print(undefined_var)  # error: unresolved-reference
            "#,
        ),
        (
            "ty.toml",
            r#"
            [src]
            include = [
                "src/**test/"
            ]
            "#,
        ),
    ])?;

    // By default, dist/ is excluded, so only src/main.py should be checked
    assert_cmd_snapshot!(case.command(), @r#"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ty failed
      Cause: error[invalid-glob]: Invalid include pattern
     --> ty.toml:4:5
      |
    2 | [src]
    3 | include = [
    4 |     "src/**test/"
      |     ^^^^^^^^^^^^^ Too many stars at position 5
    5 | ]
      |
    "#);

    Ok(())
}

#[test]
fn invalid_include_pattern_concise_output() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "src/main.py",
            r#"
            print(undefined_var)  # error: unresolved-reference
            "#,
        ),
        (
            "ty.toml",
            r#"
            [src]
            include = [
                "src/**test/"
            ]
            "#,
        ),
    ])?;

    // By default, dist/ is excluded, so only src/main.py should be checked
    assert_cmd_snapshot!(case.command().arg("--output-format").arg("concise"), @r"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ty failed
      Cause: error[invalid-glob] ty.toml:4:5: Invalid include pattern: Too many stars at position 5
    ");

    Ok(())
}

#[test]
fn invalid_exclude_pattern() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "src/main.py",
            r#"
            print(undefined_var)  # error: unresolved-reference
            "#,
        ),
        (
            "ty.toml",
            r#"
            [src]
            exclude = [
                "../src"
            ]
            "#,
        ),
    ])?;

    // By default, dist/ is excluded, so only src/main.py should be checked
    assert_cmd_snapshot!(case.command(), @r#"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    WARN ty is pre-release software and not ready for production use. Expect to encounter bugs, missing features, and fatal errors.
    ty failed
      Cause: error[invalid-glob]: Invalid exclude pattern
     --> ty.toml:4:5
      |
    2 | [src]
    3 | exclude = [
    4 |     "../src"
      |     ^^^^^^^^ The parent directory operator (`..`) at position 1 is not allowed
    5 | ]
      |
    "#);

    Ok(())
}
