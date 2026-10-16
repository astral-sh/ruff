//! Tests that execute ty from inside a Python environment.
//!
//! Copying the executable can race with any concurrent process spawn: a child can inherit the
//! writable copy descriptor, causing Linux to reject execution with `ETXTBSY`. Keep these tests
//! in their own executable, and hold `TEST_LOCK` for every test so copying and spawning cannot
//! overlap between tests. The other CLI tests can continue to run in parallel.

pub mod common;

use std::{
    path::Path,
    sync::{Mutex, PoisonError},
};

use anyhow::Context as _;
use insta_cmd::assert_cmd_snapshot;

use common::CliTest;

static TEST_LOCK: Mutex<()> = Mutex::new(());

/// ty should include site packages from its own environment when no other environment is found.
#[test]
fn ty_environment_is_only_environment() -> anyhow::Result<()> {
    let _lock = TEST_LOCK.lock().unwrap_or_else(PoisonError::into_inner);

    let ty_venv_site_packages = if cfg!(windows) {
        "ty-venv/Lib/site-packages"
    } else {
        "ty-venv/lib/python3.13/site-packages"
    };

    let ty_executable_path = if cfg!(windows) {
        "ty-venv/Scripts/ty.exe"
    } else {
        "ty-venv/bin/ty"
    };

    let ty_package_path = format!("{ty_venv_site_packages}/ty_package/__init__.py");

    let case = CliTest::with_files([
        (ty_package_path.as_str(), "class TyEnvClass: ..."),
        (
            "ty-venv/pyvenv.cfg",
            r"
            home = ./
            version = 3.13
            ",
        ),
        (
            "test.py",
            r"
            from ty_package import TyEnvClass
            ",
        ),
    ])?;

    let case = case.with_ty_at(ty_executable_path)?;
    assert_cmd_snapshot!(case.command(), @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    ");

    Ok(())
}

/// ty should include site packages from both its own environment and a local `.venv`. The packages
/// from ty's environment should take precedence.
#[test]
fn ty_environment_and_discovered_venv() -> anyhow::Result<()> {
    let _lock = TEST_LOCK.lock().unwrap_or_else(PoisonError::into_inner);

    let ty_venv_site_packages = if cfg!(windows) {
        "ty-venv/Lib/site-packages"
    } else {
        "ty-venv/lib/python3.13/site-packages"
    };

    let ty_executable_path = if cfg!(windows) {
        "ty-venv/Scripts/ty.exe"
    } else {
        "ty-venv/bin/ty"
    };

    let local_venv_site_packages = if cfg!(windows) {
        ".venv/Lib/site-packages"
    } else {
        ".venv/lib/python3.13/site-packages"
    };

    let ty_unique_package = format!("{ty_venv_site_packages}/ty_package/__init__.py");
    let local_unique_package = format!("{local_venv_site_packages}/local_package/__init__.py");
    let ty_conflicting_package = format!("{ty_venv_site_packages}/shared_package/__init__.py");
    let local_conflicting_package =
        format!("{local_venv_site_packages}/shared_package/__init__.py");

    let case = CliTest::with_files([
        (ty_unique_package.as_str(), "class TyEnvClass: ..."),
        (local_unique_package.as_str(), "class LocalClass: ..."),
        (ty_conflicting_package.as_str(), "class FromTyEnv: ..."),
        (
            local_conflicting_package.as_str(),
            "class FromLocalVenv: ...",
        ),
        (
            "ty-venv/pyvenv.cfg",
            r"
            home = ./
            version = 3.13
            ",
        ),
        (
            ".venv/pyvenv.cfg",
            r"
            home = ./
            version = 3.13
            ",
        ),
        (
            "test.py",
            r"
            # Should resolve from ty's environment
            from ty_package import TyEnvClass
            # Should resolve from local .venv
            from local_package import LocalClass
            # Should resolve from ty's environment (takes precedence)
            from shared_package import FromTyEnv
            # Should NOT resolve (shadowed by ty's environment version)
            from shared_package import FromLocalVenv
            ",
        ),
    ])?
    .with_ty_at(ty_executable_path)?;

    assert_cmd_snapshot!(case.command(), @"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-import]: Module `shared_package` has no member `FromLocalVenv`
     --> test.py:9:28
      |
    9 | from shared_package import FromLocalVenv
      |                            ^^^^^^^^^^^^^

    Found 1 diagnostic

    ----- stderr -----
    ");

    Ok(())
}

/// When `VIRTUAL_ENV` is set, ty should *not* discover its own environment's site-packages.
#[test]
fn ty_environment_and_active_environment() -> anyhow::Result<()> {
    let _lock = TEST_LOCK.lock().unwrap_or_else(PoisonError::into_inner);

    let ty_venv_site_packages = if cfg!(windows) {
        "ty-venv/Lib/site-packages"
    } else {
        "ty-venv/lib/python3.13/site-packages"
    };

    let ty_executable_path = if cfg!(windows) {
        "ty-venv/Scripts/ty.exe"
    } else {
        "ty-venv/bin/ty"
    };

    let active_venv_site_packages = if cfg!(windows) {
        "active-venv/Lib/site-packages"
    } else {
        "active-venv/lib/python3.13/site-packages"
    };

    let ty_package_path = format!("{ty_venv_site_packages}/ty_package/__init__.py");
    let active_package_path = format!("{active_venv_site_packages}/active_package/__init__.py");

    let case = CliTest::with_files([
        (ty_package_path.as_str(), "class TyEnvClass: ..."),
        (
            "ty-venv/pyvenv.cfg",
            r"
            home = ./
            version = 3.13
            ",
        ),
        (active_package_path.as_str(), "class ActiveClass: ..."),
        (
            "active-venv/pyvenv.cfg",
            r"
            home = ./
            version = 3.13
            ",
        ),
        (
            "test.py",
            r"
            from ty_package import TyEnvClass
            from active_package import ActiveClass
            ",
        ),
    ])?
    .with_ty_at(ty_executable_path)?
    .with_filter(&site_packages_filter("3.13"), "<site-packages>");

    assert_cmd_snapshot!(
        case.command()
            .env("VIRTUAL_ENV", case.root().join("active-venv")),
        @"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-import]: Cannot resolve imported module `ty_package`
     --> test.py:2:6
      |
    2 | from ty_package import TyEnvClass
      |      ^^^^^^^^^^
    info: Searched in the following paths during module resolution:
    info:   1. <temp_dir>/ (first-party code)
    info:   2. vendored://stdlib (stdlib typeshed stubs vendored by ty)
    info:   3. <temp_dir>/active-venv/<site-packages> (site-packages)
    info: make sure your Python environment is properly configured: https://docs.astral.sh/ty/modules/#python-environment

    Found 1 diagnostic

    ----- stderr -----
    "
    );

    Ok(())
}

/// When ty is installed in a system environment rather than a virtual environment, it should
/// include the environment's site-packages in its search path.
#[test]
fn ty_environment_is_system_not_virtual() -> anyhow::Result<()> {
    let _lock = TEST_LOCK.lock().unwrap_or_else(PoisonError::into_inner);

    let ty_system_site_packages = if cfg!(windows) {
        "system-python/Lib/site-packages"
    } else {
        "system-python/lib/python3.13/site-packages"
    };

    let ty_executable_path = if cfg!(windows) {
        "system-python/Scripts/ty.exe"
    } else {
        "system-python/bin/ty"
    };

    let ty_package_path = format!("{ty_system_site_packages}/system_package/__init__.py");

    let case = CliTest::with_files([
        // Package in system Python installation (should be discovered)
        (ty_package_path.as_str(), "class SystemClass: ..."),
        // Note: NO pyvenv.cfg - this is a system installation, not a venv
        (
            "test.py",
            r"
            from system_package import SystemClass
            ",
        ),
    ])?
    .with_ty_at(ty_executable_path)?;

    assert_cmd_snapshot!(case.command(), @"
    success: true
    exit_code: 0
    ----- stdout -----
    All checks passed!

    ----- stderr -----
    ");

    Ok(())
}

/// When ty is installed in a system environment and there's also a local `.venv`,
/// the system environment's site-packages should not be included at all.
/// This is the opposite of when ty is installed in a virtual environment (like `uvx --with ...`),
/// where ty's venv takes priority but both are included.
#[test]
fn ty_system_environment_and_local_venv() -> anyhow::Result<()> {
    let _lock = TEST_LOCK.lock().unwrap_or_else(PoisonError::into_inner);

    let ty_system_site_packages = if cfg!(windows) {
        "system-python/Lib/site-packages"
    } else {
        "system-python/lib/python3.13/site-packages"
    };

    let ty_executable_path = if cfg!(windows) {
        "system-python/Scripts/ty.exe"
    } else {
        "system-python/bin/ty"
    };

    let local_venv_site_packages = if cfg!(windows) {
        ".venv/Lib/site-packages"
    } else {
        ".venv/lib/python3.13/site-packages"
    };

    let ty_unique_package = format!("{ty_system_site_packages}/system_package/__init__.py");
    let local_unique_package = format!("{local_venv_site_packages}/local_package/__init__.py");

    let case = CliTest::with_files([
        (ty_unique_package.as_str(), "class SystemEnvClass: ..."),
        (local_unique_package.as_str(), "class LocalClass: ..."),
        // Note: NO pyvenv.cfg for system-python - this is a system installation, not a venv
        (
            ".venv/pyvenv.cfg",
            r"
            home = ./
            version = 3.13
            ",
        ),
        (
            "test.py",
            r"
            # Should NOT resolve (system Python site-packages excluded when .venv exists)
            from system_package import SystemEnvClass
            # Should resolve from local .venv
            from local_package import LocalClass
            ",
        ),
    ])?
    .with_ty_at(ty_executable_path)?
    .with_filter(&site_packages_filter("3.13"), "<site-packages>");

    assert_cmd_snapshot!(case.command().env_remove("VIRTUAL_ENV"), @"
    success: false
    exit_code: 1
    ----- stdout -----
    error[unresolved-import]: Cannot resolve imported module `system_package`
     --> test.py:3:6
      |
    3 | from system_package import SystemEnvClass
      |      ^^^^^^^^^^^^^^
    info: Searched in the following paths during module resolution:
    info:   1. <temp_dir>/ (first-party code)
    info:   2. vendored://stdlib (stdlib typeshed stubs vendored by ty)
    info:   3. <temp_dir>/.venv/<site-packages> (site-packages)
    info: make sure your Python environment is properly configured: https://docs.astral.sh/ty/modules/#python-environment

    Found 1 diagnostic

    ----- stderr -----
    ");

    Ok(())
}

#[test]
fn find_does_not_fall_back_to_path_or_own_executable() -> anyhow::Result<()> {
    let _lock = TEST_LOCK.lock().unwrap_or_else(PoisonError::into_inner);

    let case = CliTest::with_file("own environment/pyvenv.cfg", "home = .\n")?;
    let own_ty = case.root().join(if cfg!(windows) {
        "own environment/Scripts/ty.exe"
    } else {
        "own environment/bin/ty"
    });
    let case = case.with_ty_at(&own_ty)?;
    let path = own_ty.parent().context("ty must have a parent")?;

    assert_cmd_snapshot!(case.command_with_subcommand("server").arg("--find-executable").env("PATH", path), @"
    success: false
    exit_code: 1
    ----- stdout -----

    ----- stderr -----
    ");

    Ok(())
}

impl CliTest {
    /// Return [`Self`] with the ty binary copied to the specified path instead.
    fn with_ty_at(mut self, dest_path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let dest_path = dest_path.as_ref();
        let dest_path = self.project_dir.join(dest_path);

        Self::ensure_parent_directory(&dest_path)?;
        std::fs::copy(&self.ty_binary_path, &dest_path)
            .with_context(|| format!("Failed to copy ty binary to `{}`", dest_path.display()))?;

        self.ty_binary_path = dest_path;
        Ok(self)
    }
}

fn site_packages_filter(python_version: &str) -> String {
    if cfg!(windows) {
        "Lib/site-packages".to_string()
    } else {
        format!("lib/python{}/site-packages", regex::escape(python_version))
    }
}
