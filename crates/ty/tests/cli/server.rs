use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Context as _;
use insta_cmd::assert_cmd_snapshot;

use crate::CliTest;

#[test]
fn find_uses_discovered_project_root() -> anyhow::Result<()> {
    let case = CliTest::new()?.with_filter(r"/Scripts/ty\b", "/bin/ty");
    venv_with_ty(&case, "project with spaces/.venv")?;
    venv_with_ty(&case, "project with spaces/src/.venv")?;
    let project = case.root().join("project with spaces");
    case.write_file(project.join("ty.toml"), "")?;

    assert_cmd_snapshot!(find_command(&case).current_dir(project.join("src")), @"
    success: true
    exit_code: 0
    ----- stdout -----
    <temp_dir>/project with spaces/.venv/bin/ty

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn find_uses_configured_environment() -> anyhow::Result<()> {
    let case = CliTest::new()?.with_filter(r"/Scripts/ty\b", "/bin/ty");
    venv_with_ty(&case, "project with spaces/configured environment")?;
    venv_with_ty(&case, "project with spaces/.venv")?;
    let project = case.root().join("project with spaces");
    fs::create_dir(project.join("src"))?;
    case.write_file(
        project.join("pyproject.toml"),
        "[tool.ty.environment]\npython = 'configured environment'\n",
    )?;

    assert_cmd_snapshot!(find_command(&case)
        .current_dir(project.join("src"))
        .env("VIRTUAL_ENV", project.join(".venv")), @"
    success: true
    exit_code: 0
    ----- stdout -----
    <temp_dir>/project with spaces/configured environment/bin/ty

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn find_uses_configured_interpreter() -> anyhow::Result<()> {
    let case = CliTest::new()?.with_filter(r"/Scripts/ty\b", "/bin/ty");
    venv_with_ty(&case, "configured environment")?;
    let interpreter = if cfg!(windows) {
        "configured environment/Scripts/python.exe"
    } else {
        "configured environment/bin/python3"
    };
    write_executable(&case, interpreter)?;
    case.write_file(
        "ty.toml",
        &format!("[environment]\npython = '{interpreter}'\n"),
    )?;

    assert_cmd_snapshot!(find_command(&case), @"
    success: true
    exit_code: 0
    ----- stdout -----
    <temp_dir>/configured environment/bin/ty

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn find_does_not_fall_back_from_configured_environment() -> anyhow::Result<()> {
    let case = CliTest::with_file(
        "ty.toml",
        "[environment]\npython = 'configured environment'\n",
    )?;
    venv_with_ty(&case, ".venv")?;

    assert_cmd_snapshot!(find_command(&case), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    ");

    case.write_file("configured environment/pyvenv.cfg", "home = .\n")?;

    assert_cmd_snapshot!(find_command(&case), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn find_uses_cwd_when_project_discovery_fails() -> anyhow::Result<()> {
    let case = CliTest::with_file("pyproject.toml", "not valid TOML [")?
        .with_filter(r"/Scripts/ty\b", "/bin/ty");
    venv_with_ty(&case, "src/.venv")?;

    assert_cmd_snapshot!(find_command(&case).current_dir(case.root().join("src")), @"
    success: true
    exit_code: 0
    ----- stdout -----
    <temp_dir>/src/.venv/bin/ty

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn find_does_not_fall_back_to_path_or_own_executable() -> anyhow::Result<()> {
    let case = CliTest::new()?;
    let own_ty = venv_with_ty(&case, "own environment")?;
    let case = case.with_ty_at(&own_ty)?;
    let path = own_ty.parent().context("ty must have a parent")?;

    assert_cmd_snapshot!(find_command(&case).env("PATH", path), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn find_rejects_wrong_layout() -> anyhow::Result<()> {
    let case = CliTest::with_file(".venv/pyvenv.cfg", "home = .\n")?;
    let other_layout = if cfg!(windows) {
        "bin/ty"
    } else {
        "Scripts/ty.exe"
    };
    write_executable(&case, Path::new(".venv").join(other_layout))?;

    assert_cmd_snapshot!(find_command(&case), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn find_rejects_directory() -> anyhow::Result<()> {
    let case = CliTest::with_file(".venv/pyvenv.cfg", "home = .\n")?;
    fs::create_dir_all(case.root().join(ty_path(".venv")))?;

    assert_cmd_snapshot!(find_command(&case), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    ");

    Ok(())
}

#[cfg(unix)]
#[test]
fn find_follows_symlinks() -> anyhow::Result<()> {
    let case = CliTest::with_file(".venv/pyvenv.cfg", "home = .\n")?;
    let target = write_executable(&case, "target with spaces")?;
    case.write_symlink(&target, ty_path(".venv"))?;

    assert_cmd_snapshot!(find_command(&case), @"
    success: true
    exit_code: 0
    ----- stdout -----
    <temp_dir>/.venv/bin/ty

    ----- stderr -----
    ");

    Ok(())
}

#[cfg(unix)]
#[test]
fn find_rejects_non_executable_file() -> anyhow::Result<()> {
    let case = CliTest::new()?;
    let candidate = venv_with_ty(&case, ".venv")?;
    fs::set_permissions(&candidate, fs::Permissions::from_mode(0o644))?;

    assert_cmd_snapshot!(find_command(&case), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    ");

    Ok(())
}

#[cfg(unix)]
#[test]
fn find_rejects_broken_symlink() -> anyhow::Result<()> {
    let case = CliTest::with_file(".venv/pyvenv.cfg", "home = .\n")?;
    case.write_symlink("missing", ty_path(".venv"))?;

    assert_cmd_snapshot!(find_command(&case), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn find_returns_no_match_on_python_environment_error() -> anyhow::Result<()> {
    let case = CliTest::new()?;

    assert_cmd_snapshot!(find_command(&case).env("VIRTUAL_ENV", "missing"), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    ");

    Ok(())
}

fn find_command(case: &CliTest) -> Command {
    let mut command = case.command_with_subcommand("server");
    command.arg("--find-executable");
    command
}

fn venv_with_ty(case: &CliTest, prefix: &str) -> anyhow::Result<PathBuf> {
    case.write_file(Path::new(prefix).join("pyvenv.cfg"), "home = .\n")?;
    write_executable(case, ty_path(prefix))
}

fn ty_path(prefix: impl AsRef<Path>) -> PathBuf {
    prefix.as_ref().join(if cfg!(windows) {
        "Scripts/ty.exe"
    } else {
        "bin/ty"
    })
}

fn write_executable(case: &CliTest, path: impl AsRef<Path>) -> anyhow::Result<PathBuf> {
    let path = case.root().join(path);
    case.write_file(&path, "")?;
    #[cfg(unix)]
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}
