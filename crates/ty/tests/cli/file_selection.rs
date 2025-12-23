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

    error[unresolved-reference]: Name `temp_undefined_var` used when not defined
     --> temp_file.py:2:7
      |
    2 | print(temp_undefined_var)  # error: unresolved-reference
      |       ^^^^^^^^^^^^^^^^^^
      |
    info: rule `unresolved-reference` is enabled by default

    Found 2 diagnostics

    ----- stderr -----
    "###);

    // Test multiple exclude patterns
    assert_cmd_snapshot!(case.command().arg("--exclude").arg("tests/").arg("--exclude").arg("temp_*.py"), @r###"
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
    "###);

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

    assert_cmd_snapshot!(case.command(), @r###"
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
    "###);

    // Test multiple include patterns via configuration
    case.write_file(
        "ty.toml",
        r#"
        [src]
        include = ["src", "other.py"]
        "#,
    )?;

    assert_cmd_snapshot!(case.command(), @r###"
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
    "###);

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

    assert_cmd_snapshot!(case.command(), @r###"
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
    "###);

    // Test multiple exclude patterns via configuration
    case.write_file(
        "ty.toml",
        r#"
        [src]
        exclude = ["tests/", "temp_*.py"]
        "#,
    )?;

    assert_cmd_snapshot!(case.command(), @r###"
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
    "###);

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

    assert_cmd_snapshot!(case.command(), @r###"
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
    "###);

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
    assert_cmd_snapshot!(case.command(), @r###"
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
    "###);

    // Now override the default exclude by using a negated pattern to re-include 'dist'
    case.write_file(
        "ty.toml",
        r#"
        [src]
        exclude = ["!**/dist/"]
        "#,
    )?;

    assert_cmd_snapshot!(case.command(), @r###"
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
    "###);

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
    assert_cmd_snapshot!(case.command(), @r###"
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
    "###);

    // Now remove the configuration exclude via CLI negation
    assert_cmd_snapshot!(case.command().arg("--exclude").arg("!build/"), @r###"
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
    "###);

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
    assert_cmd_snapshot!(case.command(), @r###"
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
    "###);

    // Explicitly checking a file in an excluded directory should still check that file
    assert_cmd_snapshot!(case.command().arg("tests/generated.py"), @r###"
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
    "###);

    // Explicitly checking the entire excluded directory should check all files in it
    assert_cmd_snapshot!(case.command().arg("dist/"), @r###"
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
    "###);

    Ok(())
}

/// Test behavior when explicitly checking a path that matches an exclude pattern and `--force-exclude` is provided
#[test]
fn explicit_path_overrides_exclude_force_exclude() -> anyhow::Result<()> {
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

    // Explicitly checking a file in an excluded directory should still check that file
    assert_cmd_snapshot!(case.command().arg("tests/generated.py").arg("src/main.py"), @r"
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

    error[unresolved-reference]: Name `dist_undefined_var` used when not defined
     --> tests/generated.py:2:7
      |
    2 | print(dist_undefined_var)  # error: unresolved-reference
      |       ^^^^^^^^^^^^^^^^^^
      |
    info: rule `unresolved-reference` is enabled by default

    Found 2 diagnostics

    ----- stderr -----
    ");

    // Except when `--force-exclude` is set.
    assert_cmd_snapshot!(case.command().arg("tests/generated.py").arg("src/main.py").arg("--force-exclude"), @r"
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
    ");

    // Explicitly checking the entire excluded directory should check all files in it
    assert_cmd_snapshot!(case.command().arg("dist/").arg("src/main.py"), @r"
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

    error[unresolved-reference]: Name `undefined_var` used when not defined
     --> src/main.py:2:7
      |
    2 | print(undefined_var)  # error: unresolved-reference
      |       ^^^^^^^^^^^^^
      |
    info: rule `unresolved-reference` is enabled by default

    Found 2 diagnostics

    ----- stderr -----
    ");

    // Except when using `--force-exclude`
    assert_cmd_snapshot!(case.command().arg("dist/").arg("src/main.py").arg("--force-exclude"), @r"
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
    ");

    Ok(())
}

#[test]
fn cli_and_configuration_exclude() -> anyhow::Result<()> {
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
            "my_dist/other.py",
            r#"
            print(other_undefined_var)  # error: unresolved-reference
            "#,
        ),
        (
            "ty.toml",
            r#"
            [src]
            exclude = ["tests/"]
            "#,
        ),
    ])?;

    assert_cmd_snapshot!(case.command(), @r"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-reference]: Name `other_undefined_var` used when not defined
     --> my_dist/other.py:2:7
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
    ");

    assert_cmd_snapshot!(case.command().arg("--exclude").arg("my_dist/"), @r"
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
    assert_cmd_snapshot!(case.command(), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
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
    "###);

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
    assert_cmd_snapshot!(case.command().arg("--output-format").arg("concise"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    ty failed
      Cause: ty.toml:4:5: error[invalid-glob] Invalid include pattern: Too many stars at position 5
    "###);

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
    assert_cmd_snapshot!(case.command(), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
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
    "###);

    Ok(())
}

/// Test that ty works correctly with Bazel's symlinked file structure
#[test]
#[cfg(unix)]
fn bazel_symlinked_files() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        // Original source files in the project
        (
            "main.py",
            r#"
import library

result = library.process_data()
print(undefined_var)  # error: unresolved-reference
            "#,
        ),
        (
            "library.py",
            r#"
def process_data():
    return missing_value  # error: unresolved-reference
            "#,
        ),
        // Another source file that won't be symlinked
        (
            "other.py",
            r#"
print(other_undefined)  # error: unresolved-reference
            "#,
        ),
    ])?;

    // Create Bazel-style symlinks pointing to the actual source files
    // Bazel typically creates symlinks in bazel-out/k8-fastbuild/bin/ that point to actual sources
    std::fs::create_dir_all(case.project_dir.join("bazel-out/k8-fastbuild/bin"))?;

    // Use absolute paths to ensure the symlinks work correctly
    case.write_symlink(
        case.project_dir.join("main.py"),
        "bazel-out/k8-fastbuild/bin/main.py",
    )?;
    case.write_symlink(
        case.project_dir.join("library.py"),
        "bazel-out/k8-fastbuild/bin/library.py",
    )?;

    // Change to the bazel-out directory and run ty from there
    // The symlinks should be followed and errors should be found
    assert_cmd_snapshot!(case.command().current_dir(case.project_dir.join("bazel-out/k8-fastbuild/bin")), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-reference]: Name `missing_value` used when not defined
     --> library.py:3:12
      |
    2 | def process_data():
    3 |     return missing_value  # error: unresolved-reference
      |            ^^^^^^^^^^^^^
      |
    info: rule `unresolved-reference` is enabled by default

    error[unresolved-reference]: Name `undefined_var` used when not defined
     --> main.py:5:7
      |
    4 | result = library.process_data()
    5 | print(undefined_var)  # error: unresolved-reference
      |       ^^^^^^^^^^^^^
      |
    info: rule `unresolved-reference` is enabled by default

    Found 2 diagnostics

    ----- stderr -----
    "###);

    // Test that when checking a specific symlinked file from the bazel-out directory, it works correctly
    assert_cmd_snapshot!(case.command().current_dir(case.project_dir.join("bazel-out/k8-fastbuild/bin")).arg("main.py"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-reference]: Name `undefined_var` used when not defined
     --> main.py:5:7
      |
    4 | result = library.process_data()
    5 | print(undefined_var)  # error: unresolved-reference
      |       ^^^^^^^^^^^^^
      |
    info: rule `unresolved-reference` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    "###);

    Ok(())
}

/// Test that exclude patterns match on symlink source names, not target names
#[test]
#[cfg(unix)]
fn exclude_symlink_source_not_target() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        // Target files with generic names
        (
            "src/module.py",
            r#"
def process():
    return undefined_var  # error: unresolved-reference
            "#,
        ),
        (
            "src/utils.py",
            r#"
def helper():
    return missing_value  # error: unresolved-reference
            "#,
        ),
        (
            "regular.py",
            r#"
print(regular_undefined)  # error: unresolved-reference
            "#,
        ),
    ])?;

    // Create symlinks with names that differ from their targets
    // This simulates build systems that rename files during symlinking
    case.write_symlink("src/module.py", "generated_module.py")?;
    case.write_symlink("src/utils.py", "generated_utils.py")?;

    // Exclude pattern should match on the symlink name (generated_*), not the target name
    assert_cmd_snapshot!(case.command().arg("--exclude").arg("generated_*.py"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-reference]: Name `regular_undefined` used when not defined
     --> regular.py:2:7
      |
    2 | print(regular_undefined)  # error: unresolved-reference
      |       ^^^^^^^^^^^^^^^^^
      |
    info: rule `unresolved-reference` is enabled by default

    error[unresolved-reference]: Name `undefined_var` used when not defined
     --> src/module.py:3:12
      |
    2 | def process():
    3 |     return undefined_var  # error: unresolved-reference
      |            ^^^^^^^^^^^^^
      |
    info: rule `unresolved-reference` is enabled by default

    error[unresolved-reference]: Name `missing_value` used when not defined
     --> src/utils.py:3:12
      |
    2 | def helper():
    3 |     return missing_value  # error: unresolved-reference
      |            ^^^^^^^^^^^^^
      |
    info: rule `unresolved-reference` is enabled by default

    Found 3 diagnostics

    ----- stderr -----
    "###);

    // Exclude pattern on target path should not affect symlinks with different names
    assert_cmd_snapshot!(case.command().arg("--exclude").arg("src/*.py"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-reference]: Name `undefined_var` used when not defined
     --> generated_module.py:3:12
      |
    2 | def process():
    3 |     return undefined_var  # error: unresolved-reference
      |            ^^^^^^^^^^^^^
      |
    info: rule `unresolved-reference` is enabled by default

    error[unresolved-reference]: Name `missing_value` used when not defined
     --> generated_utils.py:3:12
      |
    2 | def helper():
    3 |     return missing_value  # error: unresolved-reference
      |            ^^^^^^^^^^^^^
      |
    info: rule `unresolved-reference` is enabled by default

    error[unresolved-reference]: Name `regular_undefined` used when not defined
     --> regular.py:2:7
      |
    2 | print(regular_undefined)  # error: unresolved-reference
      |       ^^^^^^^^^^^^^^^^^
      |
    info: rule `unresolved-reference` is enabled by default

    Found 3 diagnostics

    ----- stderr -----
    "###);

    // Test that explicitly passing a symlink always checks it, even if excluded
    assert_cmd_snapshot!(case.command().arg("--exclude").arg("generated_*.py").arg("generated_module.py"), @r###"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-reference]: Name `undefined_var` used when not defined
     --> generated_module.py:3:12
      |
    2 | def process():
    3 |     return undefined_var  # error: unresolved-reference
      |            ^^^^^^^^^^^^^
      |
    info: rule `unresolved-reference` is enabled by default

    Found 1 diagnostic

    ----- stderr -----
    "###);

    Ok(())
}
