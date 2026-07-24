//! Integration tests for ty's side of `uv check`.
//!
//! Corresponding uv-side workspace tests live at
//! <https://github.com/astral-sh/uv/blob/main/crates/uv/tests/project/check.rs>.

#[cfg(feature = "test-uv")]
use std::{path::Path, process::Command};

use insta_cmd::assert_cmd_snapshot;

use crate::CliTest;

fn workspace_case() -> anyhow::Result<CliTest> {
    CliTest::with_files([
        (
            "pyproject.toml",
            r#"
[tool.uv.workspace]
members = ["packages/*"]
"#,
        ),
        (
            "packages/member/pyproject.toml",
            r#"
[project]
name = "member"
version = "0.1.0"
requires-python = ">=3.8"
"#,
        ),
        (
            "packages/member/member.py",
            "value: int = 'selected-member'",
        ),
        (
            "packages/sibling/pyproject.toml",
            r#"
[project]
name = "sibling"
version = "0.1.0"
requires-python = ">=3.8"
"#,
        ),
        (
            "packages/sibling/sibling.py",
            "value: int = 'unselected-sibling'",
        ),
    ])
}

#[cfg(feature = "test-uv")]
fn command_with_uv(case: &CliTest, virtual_env: Option<&Path>) -> anyhow::Result<Command> {
    let mut sync = Command::new("uv");
    sync.current_dir(case.root())
        .args(["workspace", "metadata", "--sync"])
        .env("UV_CACHE_DIR", case.root().join("cache"))
        .env("UV_OFFLINE", "1")
        .env("UV_PYTHON_DOWNLOADS", "never");
    if let Some(virtual_env) = virtual_env {
        sync.arg("--active").env("VIRTUAL_ENV", virtual_env);
    }
    anyhow::ensure!(
        sync.output()?.status.success(),
        "failed to prepare uv workspace"
    );

    let mut command = case.command();
    command
        .env("TY_UV", "1")
        .env("UV", "uv")
        .env("UV_CACHE_DIR", case.root().join("cache"))
        .env("UV_OFFLINE", "1")
        .env("UV_PYTHON_DOWNLOADS", "never")
        .env("TY_OUTPUT_FORMAT", "concise")
        .env("PATH", std::env::var_os("PATH").unwrap_or_default());
    if let Some(virtual_env) = virtual_env {
        command.env("VIRTUAL_ENV", virtual_env);
    }

    Ok(command)
}

/// The workspace root provides first-party imports without expanding analysis to unselected
/// sibling members.
#[cfg(feature = "test-uv")]
#[test]
fn uses_uv_workspace_root_without_checking_siblings() -> anyhow::Result<()> {
    let case = workspace_case()?;
    case.write_file("shared.py", "value: int = 'unselected-workspace-root'")?;
    case.write_file(
        "packages/member/member.py",
        "import shared\nvalue: int = 'selected-member'",
    )?;

    let mut command = command_with_uv(&case, None)?;
    command
        .current_dir(case.root().join("packages/member"))
        .arg(".");

    assert_cmd_snapshot!(command, @r#"
    success: false
    exit_code: 1
    ----- stdout -----
    member.py:2:14: error[invalid-assignment] Object of type `Literal["selected-member"]` is not assignable to `int`
    Found 1 diagnostic

    ----- stderr -----
    "#);
    assert!(case.root().join(".venv").is_dir());

    Ok(())
}

/// An explicit file is treated as a script, so workspace discovery stays disabled even when
/// `TY_UV` is set.
#[cfg(feature = "test-uv")]
#[test]
fn explicit_file_path_disables_uv_workspace_discovery() -> anyhow::Result<()> {
    let case = workspace_case()?;
    case.write_file("shared.py", "value: int = 'unselected-workspace-root'")?;
    case.write_file(
        "packages/member/member.py",
        "import shared\nvalue: int = 'selected-script'",
    )?;

    let mut command = command_with_uv(&case, None)?;
    command
        .current_dir(case.root().join("packages/member"))
        .arg("member.py");

    assert_cmd_snapshot!(command, @r#"
    success: false
    exit_code: 1
    ----- stdout -----
    member.py:1:8: error[unresolved-import] Cannot resolve imported module `shared`
    member.py:2:14: error[invalid-assignment] Object of type `Literal["selected-script"]` is not assignable to `int`
    Found 2 diagnostics

    ----- stderr -----
    "#);

    Ok(())
}

/// An explicitly selected member inherits ty rule configuration from the uv workspace root.
#[cfg(feature = "test-uv")]
#[test]
fn explicit_workspace_member_directory_uses_workspace_configuration() -> anyhow::Result<()> {
    let case = workspace_case()?;
    case.write_file(
        "pyproject.toml",
        r#"
[tool.uv.workspace]
members = ["packages/*"]

[tool.ty.rules]
invalid-assignment = "ignore"
"#,
    )?;
    let mut command = command_with_uv(&case, None)?;
    command.arg("packages/member");

    assert_cmd_snapshot!(command, @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    ");

    Ok(())
}

/// Workspace configuration still applies when the selected member lives outside the workspace
/// root's directory tree.
#[cfg(feature = "test-uv")]
#[test]
fn external_workspace_member_uses_workspace_configuration() -> anyhow::Result<()> {
    let case = CliTest::with_files([
        (
            "pyproject.toml",
            r#"
[tool.uv.workspace]
members = ["../external-package"]

[tool.ty.rules]
invalid-assignment = "ignore"
"#,
        ),
        (
            "../external-package/pyproject.toml",
            r#"
[project]
name = "external-package"
version = "0.1.0"
requires-python = ">=3.8"
"#,
        ),
        (
            "../external-package/member.py",
            "value: int = 'selected-external-member'",
        ),
    ])?;

    let mut command = command_with_uv(&case, None)?;
    command
        .args(["--project", "../external-package", "../external-package"])
        .env("UV_PROJECT", case.root());

    assert_cmd_snapshot!(command, @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    ");

    Ok(())
}

/// Excludes passed by `uv check` prevent an unselected nested member from being analyzed.
#[cfg(feature = "test-uv")]
#[test]
fn selected_workspace_member_excludes_nested_member() -> anyhow::Result<()> {
    let case = workspace_case()?;
    case.write_file(
        "pyproject.toml",
        r#"
[tool.uv.workspace]
members = ["packages/*", "packages/member/nested"]
"#,
    )?;
    case.write_file(
        "packages/member/nested/pyproject.toml",
        r#"
[project]
name = "nested"
version = "0.1.0"
requires-python = ">=3.8"
"#,
    )?;
    case.write_file(
        "packages/member/nested/nested.py",
        "value: int = 'unselected-nested-member'",
    )?;

    let mut command = command_with_uv(&case, None)?;
    command.args(["--exclude", "packages/member/nested", "packages/member"]);

    assert_cmd_snapshot!(command, @r#"
    success: false
    exit_code: 1
    ----- stdout -----
    packages/member/member.py:1:14: error[invalid-assignment] Object of type `Literal["selected-member"]` is not assignable to `int`
    Found 1 diagnostic

    ----- stderr -----
    "#);

    Ok(())
}

/// Metadata discovery preserves uv's active isolated environment instead of using an invalid
/// Python environment configured in the workspace.
#[cfg(feature = "test-uv")]
#[test]
fn forwards_active_environment_to_uv() -> anyhow::Result<()> {
    let case = workspace_case()?;
    case.write_file(
        "pyproject.toml",
        r#"
[tool.uv.workspace]
members = ["packages/*"]

[tool.ty.environment]
python = "missing-configured-environment"
"#,
    )?;
    let environment = case.root().join("isolated");
    let mut command = command_with_uv(&case, Some(&environment))?;
    command
        .current_dir(case.root().join("packages/member"))
        .arg(".")
        .env_remove("UV_PROJECT_ENVIRONMENT");

    assert_cmd_snapshot!(command, @r#"
    success: false
    exit_code: 1
    ----- stdout -----
    member.py:1:14: error[invalid-assignment] Object of type `Literal["selected-member"]` is not assignable to `int`
    Found 1 diagnostic

    ----- stderr -----
    "#);

    assert!(environment.is_dir());
    assert!(!case.root().join(".venv").exists());

    Ok(())
}

/// Merely exposing the uv executable must not change ordinary ty project discovery without
/// `TY_UV`.
#[test]
fn uv_workspace_discovery_is_opt_in() -> anyhow::Result<()> {
    let case = workspace_case()?;
    case.write_file("shared.py", "value: int = 'unselected-workspace-root'")?;
    case.write_file(
        "packages/member/member.py",
        "import shared\nvalue: int = 'selected-member'",
    )?;

    let mut command = case.command();
    command
        .current_dir(case.root().join("packages/member"))
        .env("UV", "uv")
        .env("TY_OUTPUT_FORMAT", "concise")
        .env_remove("TY_UV");

    assert_cmd_snapshot!(command, @r#"
    success: false
    exit_code: 1
    ----- stdout -----
    member.py:1:8: error[unresolved-import] Cannot resolve imported module `shared`
    member.py:2:14: error[invalid-assignment] Object of type `Literal["selected-member"]` is not assignable to `int`
    Found 2 diagnostics

    ----- stderr -----
    "#);

    Ok(())
}

/// Failures to invoke uv are visible by default instead of silently disabling integration.
#[test]
fn warns_when_uv_workspace_metadata_cannot_be_loaded() -> anyhow::Result<()> {
    let case = workspace_case()?;
    case.write_file("packages/member/member.py", "value: int = 1")?;

    let mut command = case.command();
    command
        .current_dir(case.root().join("packages/member"))
        .arg(".")
        .env("TY_UV", "1")
        .env("UV", "missing-uv-executable")
        .env("TY_OUTPUT_FORMAT", "concise");

    assert_cmd_snapshot!(command, @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    WARN Failed to invoke `uv workspace metadata`: No such file or directory (os error 2)
    ");

    Ok(())
}

/// Workspace discovery can find uv on `PATH` when the `UV` executable override is absent.
#[cfg(feature = "test-uv")]
#[test]
fn finds_uv_on_path_without_uv_environment_variable() -> anyhow::Result<()> {
    let case = workspace_case()?;
    case.write_file("shared.py", "value: int = 'unselected-workspace-root'")?;
    case.write_file(
        "packages/member/member.py",
        "import shared\nvalue: int = 'selected-member'",
    )?;

    let mut command = command_with_uv(&case, None)?;
    command
        .current_dir(case.root().join("packages/member"))
        .arg(".")
        .env_remove("UV");

    assert_cmd_snapshot!(command, @r#"
    success: false
    exit_code: 1
    ----- stdout -----
    member.py:2:14: error[invalid-assignment] Object of type `Literal["selected-member"]` is not assignable to `int`
    Found 1 diagnostic

    ----- stderr -----
    "#);

    Ok(())
}

/// Version-sensitive diagnostics attribute their assumed Python version to workspace metadata,
/// not to a command-line override.
#[cfg(feature = "test-uv")]
#[test]
fn reports_uv_workspace_python_version_source() -> anyhow::Result<()> {
    let case = workspace_case()?;
    case.write_file("packages/member/member.py", "frozendict")?;

    for output_format in ["full", "concise"] {
        let mut command = command_with_uv(&case, None)?;
        command
            .current_dir(case.root().join("packages/member"))
            .arg(".")
            .arg("--output-format")
            .arg(output_format);

        let output = command.output()?;
        let stdout = String::from_utf8(output.stdout)?;
        assert!(!output.status.success());
        assert!(!stdout.contains("specified on the command line"));
        if output_format == "full" {
            assert!(stdout.contains("provided by uv workspace metadata"));
        }
    }

    Ok(())
}
