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

#[cfg(feature = "test-uv")]
fn command_with_uv(case: &CliTest, virtual_env: Option<&Path>) -> anyhow::Result<Option<Command>> {
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

    // TODO: Remove this capability check once CI's pinned uv includes environment metadata by
    // default.
    let mut metadata = Command::new("uv");
    metadata
        .current_dir(case.root())
        .args(["workspace", "metadata", "--frozen", "--active"])
        .env("UV_CACHE_DIR", case.root().join("cache"))
        .env("UV_OFFLINE", "1")
        .env("UV_PYTHON_DOWNLOADS", "never");
    if let Some(virtual_env) = virtual_env {
        metadata.env("VIRTUAL_ENV", virtual_env);
    }
    let metadata = metadata.output()?;
    anyhow::ensure!(
        metadata.status.success(),
        "failed to query uv workspace metadata: {}",
        String::from_utf8_lossy(&metadata.stderr)
    );
    if !String::from_utf8_lossy(&metadata.stdout).contains("\"environment\"") {
        return Ok(None);
    }

    let mut command = case.command();
    command
        .env("TY_UV", "1")
        .env("UV", "uv")
        .env("UV_CACHE_DIR", case.root().join("cache"))
        .env("UV_OFFLINE", "1")
        .env("UV_PYTHON_DOWNLOADS", "never")
        .env("PATH", std::env::var_os("PATH").unwrap_or_default());
    if let Some(virtual_env) = virtual_env {
        command.env("VIRTUAL_ENV", virtual_env);
    }

    Ok(Some(command))
}

#[cfg(feature = "test-uv")]
#[test]
fn uv_check_uses_test_ty_binary() -> anyhow::Result<()> {
    let case = workspace_case()?;
    case.write_file("packages/member/member.py", "value: int = 'wrong'")?;

    let output = Command::new("uv")
        .current_dir(case.root().join("packages/member"))
        .arg("check")
        .env("TY", &case.ty_binary_path)
        .env("UV_CACHE_DIR", case.root().join("cache"))
        .env("UV_OFFLINE", "1")
        .env("UV_PYTHON_DOWNLOADS", "never")
        .output()?;
    let stdout = String::from_utf8(output.stdout)?;

    assert!(!output.status.success());
    assert!(stdout.contains("invalid-assignment"));
    assert!(stdout.contains("member.py"));

    Ok(())
}

#[cfg(feature = "test-uv")]
#[test]
fn uses_uv_workspace_root_without_checking_siblings() -> anyhow::Result<()> {
    let case = workspace_case()?;
    case.write_file("shared.py", "value: int = 1")?;
    case.write_file(
        "packages/member/member.py",
        "import shared\nvalue: int = 'wrong'",
    )?;

    let Some(mut command) = command_with_uv(&case, None)? else {
        return Ok(());
    };
    command
        .current_dir(case.root().join("packages/member"))
        .arg(".");

    assert_cmd_snapshot!(command, @r#"
    success: false
    exit_code: 1
    ----- stdout -----
    error[invalid-assignment]: Object of type `Literal["wrong"]` is not assignable to `int`
     --> member.py:2:8
      |
    2 | value: int = 'wrong'
      |        ---   ^^^^^^^ Incompatible value of type `Literal["wrong"]`
      |        |
      |        Declared type
      |

    Found 1 diagnostic

    ----- stderr -----
    "#);
    assert!(case.root().join(".venv").is_dir());

    Ok(())
}

#[cfg(feature = "test-uv")]
#[test]
fn explicit_file_path_disables_uv_workspace_discovery() -> anyhow::Result<()> {
    let case = workspace_case()?;
    case.write_file("shared.py", "value: int = 1")?;
    case.write_file("packages/member/member.py", "import shared")?;

    let Some(mut command) = command_with_uv(&case, None)? else {
        return Ok(());
    };
    command
        .current_dir(case.root().join("packages/member"))
        .arg("member.py");

    let output = command.output()?;
    let stdout = String::from_utf8(output.stdout)?;
    assert!(!output.status.success());
    assert!(stdout.contains("Cannot resolve imported module `shared`"));

    Ok(())
}

#[cfg(feature = "test-uv")]
#[test]
fn explicit_directory_path_uses_uv_workspace_discovery() -> anyhow::Result<()> {
    let case = workspace_case()?;
    case.write_file("shared.py", "value: int = 1")?;
    case.write_file("packages/member/member.py", "import shared")?;

    let Some(mut command) = command_with_uv(&case, None)? else {
        return Ok(());
    };
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
    assert!(case.root().join(".venv").is_dir());

    Ok(())
}

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
    case.write_file("packages/member/member.py", "value: int = 'wrong'")?;

    let Some(mut command) = command_with_uv(&case, None)? else {
        return Ok(());
    };
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
        ("../external-package/member.py", "value: int = 'wrong'"),
    ])?;

    let Some(mut command) = command_with_uv(&case, None)? else {
        return Ok(());
    };
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
    case.write_file("packages/member/nested/nested.py", "value: int = 'wrong'")?;

    let Some(mut command) = command_with_uv(&case, None)? else {
        return Ok(());
    };
    command.args(["--exclude", "packages/member/nested", "packages/member"]);

    assert_cmd_snapshot!(command, @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    ");

    Ok(())
}

#[cfg(feature = "test-uv")]
#[test]
fn forwards_active_environment_to_uv() -> anyhow::Result<()> {
    let case = workspace_case()?;
    let environment = case.root().join("isolated");
    let Some(mut command) = command_with_uv(&case, Some(&environment))? else {
        return Ok(());
    };
    command
        .current_dir(case.root().join("packages/member"))
        .arg(".")
        .env_remove("UV_PROJECT_ENVIRONMENT");

    assert_cmd_snapshot!(command, @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    ");

    assert!(environment.is_dir());
    assert!(!case.root().join(".venv").exists());

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

#[cfg(feature = "test-uv")]
#[test]
fn finds_uv_on_path_without_uv_environment_variable() -> anyhow::Result<()> {
    let case = workspace_case()?;
    case.write_file("shared.py", "value: int = 1")?;
    case.write_file("packages/member/member.py", "import shared")?;

    let Some(mut command) = command_with_uv(&case, None)? else {
        return Ok(());
    };
    command
        .current_dir(case.root().join("packages/member"))
        .arg(".")
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

#[cfg(feature = "test-uv")]
#[test]
fn reports_uv_workspace_python_version_source() -> anyhow::Result<()> {
    let case = workspace_case()?;
    case.write_file("packages/member/member.py", "frozendict")?;

    for output_format in ["full", "concise"] {
        let Some(mut command) = command_with_uv(&case, None)? else {
            return Ok(());
        };
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
