#![cfg(feature = "test-uv")]

use std::process::Command;

use insta_cmd::assert_cmd_snapshot;

use crate::CliTest;

#[test]
fn uses_uv_workspace_root_without_checking_siblings() -> anyhow::Result<()> {
    let case = workspace_case()?;
    case.write_file("shared.py", "value: int = 1")?;
    case.write_file("packages/member/member.py", "import shared")?;

    let mut command = command_with_uv(&case);
    command.current_dir(case.root().join("packages/member"));

    assert_cmd_snapshot!(command, @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn explicit_paths_filter_promoted_workspace() -> anyhow::Result<()> {
    let case = workspace_case()?;
    let mut command = command_with_uv(&case);
    command
        .current_dir(case.root().join("packages/member"))
        .arg(".");

    assert_cmd_snapshot!(command, @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn explicit_project_disables_uv_workspace_discovery() -> anyhow::Result<()> {
    let case = workspace_case()?;
    let mut command = command_with_uv(&case);
    command
        .current_dir(case.root().join("packages/member"))
        .env("UV", case.root().join("missing-uv"))
        .arg("--project")
        .arg(".");

    assert_cmd_snapshot!(command, @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn uv_workspace_discovery_is_opt_in() -> anyhow::Result<()> {
    let case = workspace_case()?;
    let mut command = command_with_uv(&case);
    command
        .current_dir(case.root().join("packages/member"))
        .env_remove("UV");

    assert_cmd_snapshot!(command, @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    ");

    Ok(())
}

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
        ("packages/member/member.py", "value: int = 1"),
        (
            "packages/sibling/pyproject.toml",
            r#"
[project]
name = "sibling"
version = "0.1.0"
requires-python = ">=3.8"
"#,
        ),
        ("packages/sibling/sibling.py", "value: int = 'wrong'"),
    ])
}

fn command_with_uv(case: &CliTest) -> Command {
    let mut command = case.command();
    command
        .env("UV", "uv")
        .env("UV_CACHE_DIR", case.root().join("cache"))
        .env("PATH", std::env::var_os("PATH").unwrap_or_default());

    command
}
