#![cfg(feature = "test-uv")]

use std::process::Command;

use insta_cmd::assert_cmd_snapshot;

use crate::CliTest;

fn command_with_uv(case: &CliTest) -> Command {
    let mut command = case.command();
    command
        .env("TY_UV", "1")
        .env("UV", "uv")
        .env("UV_CACHE_DIR", case.root().join("cache"))
        .env("PATH", std::env::var_os("PATH").unwrap_or_default());

    command
}

#[test]
fn uses_uv_workspace_root_without_checking_siblings() -> anyhow::Result<()> {
    let case = workspace_case()?;
    case.write_file("shared.py", "value: int = 1")?;
    case.write_file(
        "packages/member/member.py",
        "import shared\nvalue: int = 'wrong'",
    )?;
    case.write_file("packages/member/src/nested.py", "")?;

    let mut command = command_with_uv(&case);
    command.current_dir(case.root().join("packages/member/src"));

    assert_cmd_snapshot!(command, @r#"
    success: false
    exit_code: 1
    ----- stdout -----
    error[invalid-assignment]: Object of type `Literal["wrong"]` is not assignable to `int`
     --> <temp_dir>/packages/member/member.py:2:8
      |
    2 | value: int = 'wrong'
      |        ---   ^^^^^^^ Incompatible value of type `Literal["wrong"]`
      |        |
      |        Declared type
      |

    Found 1 diagnostic

    ----- stderr -----
    "#);

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
fn explicit_project_uses_environment_from_uv_metadata() -> anyhow::Result<()> {
    let case = workspace_case()?;
    let environment = case.root().join("uv-venv");
    case.write_file("packages/member/member.py", "import dependency")?;
    case.write_file(site_packages_path("uv-venv", "dependency.py"), "")?;

    let mut command = case.command();
    command
        .current_dir(case.root().join("packages/member"))
        .env("UV", case.root().join("missing-uv"))
        .env("TY_UV_METADATA", "1")
        .arg("--project")
        .arg(".");

    assert_cmd_snapshot!(command.pass_stdin(workspace_metadata_with_environment(&case, &environment)), @"
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
    case.write_file("shared.py", "value: int = 1")?;
    case.write_file("packages/member/member.py", "import shared")?;

    let mut command = case.command();
    command
        .current_dir(case.root().join("packages/member"))
        .env("UV", "uv")
        .env_remove("TY_UV");

    assert_cmd_snapshot!(command, @"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-import]: Cannot resolve imported module `shared`
     --> member.py:1:8
      |
    1 | import shared
      |        ^^^^^^
      |
    info: Searched in the following paths during module resolution:
    info:   1. <temp_dir>/packages/member (first-party code)
    info:   2. vendored://stdlib (stdlib typeshed stubs vendored by ty)
    info: make sure your Python environment is properly configured: https://docs.astral.sh/ty/modules/#python-environment

    Found 1 diagnostic

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn finds_uv_on_path_without_uv_environment_variable() -> anyhow::Result<()> {
    let case = workspace_case()?;
    case.write_file("shared.py", "value: int = 1")?;
    case.write_file("packages/member/member.py", "import shared")?;

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

#[test]
fn can_read_uv_workspace_metadata_from_stdin() -> anyhow::Result<()> {
    let case = workspace_case()?;
    case.write_file("shared.py", "value: int = 1")?;
    case.write_file("packages/member/member.py", "import shared")?;

    let mut command = case.command();
    command
        .current_dir(case.root().join("packages/member"))
        .env("UV", case.root().join("missing-uv"))
        .env("TY_UV_METADATA", "false")
        .arg("--uv-metadata");

    assert_cmd_snapshot!(command.pass_stdin(workspace_metadata(&case)), @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn uv_metadata_environment_variable_reads_metadata_from_stdin() -> anyhow::Result<()> {
    let case = workspace_case()?;
    case.write_file("shared.py", "value: int = 1")?;
    case.write_file("packages/member/member.py", "import shared")?;

    let mut command = case.command();
    command
        .current_dir(case.root().join("packages/member"))
        .env("UV", case.root().join("missing-uv"))
        .env("TY_UV_METADATA", "1");

    assert_cmd_snapshot!(command.pass_stdin(workspace_metadata(&case)), @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn uses_python_environment_from_uv_metadata() -> anyhow::Result<()> {
    let case = workspace_case()?;
    let environment = case.root().join("uv-venv");
    case.write_file("packages/member/member.py", "import dependency")?;
    case.write_file(site_packages_path("uv-venv", "dependency.py"), "")?;

    let mut command = case.command();
    command
        .current_dir(case.root().join("packages/member"))
        .env("UV", case.root().join("missing-uv"))
        .env("TY_UV_METADATA", "1");

    assert_cmd_snapshot!(command.pass_stdin(workspace_metadata_with_environment(&case, &environment)), @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn python_argument_overrides_uv_metadata_environment() -> anyhow::Result<()> {
    let case = workspace_case()?;
    let environment = case.root().join("uv-venv");
    case.write_file("packages/member/member.py", "import dependency")?;
    case.write_file(site_packages_path("uv-venv", "other.py"), "")?;
    case.write_file(site_packages_path("explicit-venv", "dependency.py"), "")?;

    let mut command = case.command();
    command
        .current_dir(case.root().join("packages/member"))
        .env("UV", case.root().join("missing-uv"))
        .env("TY_UV_METADATA", "1")
        .arg("--python")
        .arg(case.root().join("explicit-venv"));

    assert_cmd_snapshot!(command.pass_stdin(workspace_metadata_with_environment(&case, &environment)), @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn configured_python_overrides_uv_metadata_environment() -> anyhow::Result<()> {
    let case = workspace_case()?;
    let environment = case.root().join("uv-venv");
    case.write_file("packages/member/member.py", "import dependency")?;
    case.write_file(site_packages_path("uv-venv", "other.py"), "")?;
    case.write_file(site_packages_path("configured-venv", "dependency.py"), "")?;
    case.write_file(
        "ty.toml",
        r#"
        [environment]
        python = "configured-venv"
        "#,
    )?;

    let mut command = case.command();
    command
        .current_dir(case.root().join("packages/member"))
        .env("UV", case.root().join("missing-uv"))
        .env("TY_UV_METADATA", "1");

    assert_cmd_snapshot!(command.pass_stdin(workspace_metadata_with_environment(&case, &environment)), @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    ");

    Ok(())
}

fn workspace_metadata(case: &CliTest) -> String {
    workspace_metadata_json(case, None).to_string()
}

fn workspace_metadata_with_environment(
    case: &CliTest,
    environment: impl AsRef<std::path::Path>,
) -> String {
    workspace_metadata_json(case, Some(environment.as_ref())).to_string()
}

fn workspace_metadata_json(
    case: &CliTest,
    environment: Option<&std::path::Path>,
) -> serde_json::Value {
    let mut metadata = serde_json::json!({
        "schema": {
            "version": "preview",
        },
        "workspace_root": case.root(),
        "members": [
            {
                "path": case.root().join("packages/member"),
            },
            {
                "path": case.root().join("packages/sibling"),
            },
        ],
    });

    if let Some(environment) = environment {
        metadata["environment"] = serde_json::json!({
            "root": environment,
        });
    }

    metadata
}

fn site_packages_path(environment: &str, module: &str) -> String {
    if cfg!(windows) {
        format!("{environment}/Lib/site-packages/{module}")
    } else {
        format!("{environment}/lib/python3.13/site-packages/{module}")
    }
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
