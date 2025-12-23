use crate::db::Db;

use anyhow::{Context, Result, anyhow, bail};
use camino::Utf8Path;
use ruff_db::system::{DbWithWritableSystem as _, OsSystem, SystemPath};
use ruff_python_ast::PythonVersion;
use ty_python_semantic::{PythonEnvironment, PythonPlatform, SysPrefixPathOrigin};

/// Setup a virtual environment in the in-memory filesystem of `db` with
/// the specified dependencies installed.
pub(crate) fn setup_venv(
    db: &mut Db,
    dependencies: &[String],
    python_version: PythonVersion,
    python_platform: &PythonPlatform,
    dest_venv_path: &SystemPath,
    lockfile_path: &Utf8Path,
) -> Result<()> {
    // Create a temporary directory for the project
    let temp_dir = tempfile::Builder::new()
        .prefix("mdtest-venv-")
        .tempdir()
        .context("Failed to create temporary directory for mdtest virtual environment")?;

    // Canonicalize here to fix problems with `.strip_prefix()` later on Windows
    let temp_dir_path = dunce::canonicalize(temp_dir.path())
        .context("Failed to canonicalize temporary directory path")?;

    let temp_path = SystemPath::from_std_path(&temp_dir_path)
        .ok_or_else(|| {
            anyhow!(
                "Temporary directory path is not valid UTF-8: {}",
                temp_dir_path.display()
            )
        })?
        .to_path_buf();

    // Generate a minimal pyproject.toml
    let pyproject_toml = format!(
        r#"[project]
name = "mdtest-deps"
version = "0.1.0"
requires-python = "~={python_version}.0"
dependencies = [
{deps}
]
"#,
        python_version = python_version,
        deps = dependencies
            .iter()
            .map(|dep| format!("    \"{dep}\","))
            .collect::<Vec<_>>()
            .join("\n")
    );

    std::fs::write(
        temp_path.join("pyproject.toml").as_std_path(),
        pyproject_toml,
    )
    .context("Failed to write pyproject.toml")?;

    // Convert PythonPlatform to uv's platform format
    let uv_platform = match python_platform {
        PythonPlatform::Identifier(id) => match id.as_str() {
            "win32" => "windows",
            "darwin" => "macos",
            "linux" => "linux",
            other => other,
        },
        PythonPlatform::All => {
            bail!("For an mdtest with external dependencies, a Python platform must be specified");
        }
    };

    let upgrade_lockfile = std::env::var("MDTEST_UPGRADE_LOCKFILES").is_ok_and(|v| v == "1");
    let use_locked = if upgrade_lockfile {
        // In upgrade mode, we'll generate a new lockfile
        false
    } else if lockfile_path.exists() {
        // Copy existing lockfile to temp directory
        let temp_lockfile = temp_path.join("uv.lock");
        std::fs::copy(lockfile_path, temp_lockfile.as_std_path())
            .with_context(|| format!("Failed to copy lockfile from '{lockfile_path}'"))?;
        true
    } else {
        // No existing lockfile - error in normal mode
        bail!(
            "Lockfile not found at '{lockfile_path}'. Run with `MDTEST_UPGRADE_LOCKFILES=1` to generate it.",
        );
    };

    // Run `uv sync` to install dependencies
    let mut uv_sync = std::process::Command::new("uv");
    uv_sync
        .args(["sync", "--python-platform", uv_platform])
        .current_dir(temp_path.as_std_path());
    if use_locked {
        uv_sync.arg("--locked");
    }

    let uv_sync_output = uv_sync
        .output()
        .context("Failed to run `uv sync`. Is `uv` installed?")?;

    if !uv_sync_output.status.success() {
        let stderr = String::from_utf8_lossy(&uv_sync_output.stderr);

        if use_locked
            && stderr.contains("`uv.lock` needs to be updated, but `--locked` was provided.")
        {
            bail!(
                "Lockfile is out of date. Use one of these commands to regenerate it:\n\
                 \n\
                 uv run crates/ty_python_semantic/mdtest.py -e external/\n\
                 \n\
                 Or using cargo:\n\
                 \n\
                 MDTEST_EXTERNAL=1 MDTEST_UPGRADE_LOCKFILES=1 cargo test -p ty_python_semantic --test mdtest mdtest__external"
            );
        }

        bail!(
            "`uv sync` failed with exit code {:?}:\n{}",
            uv_sync_output.status.code(),
            stderr
        );
    }

    // In upgrade mode, copy the generated lockfile back to the source location
    if upgrade_lockfile {
        let temp_lockfile = temp_path.join("uv.lock");
        let temp_lockfile = temp_lockfile.as_std_path();
        if temp_lockfile.exists() {
            std::fs::copy(temp_lockfile, lockfile_path)
                .with_context(|| format!("Failed to write lockfile to '{lockfile_path}'"))?;
        } else {
            bail!(
                "Expected uv to create a lockfile at '{}'",
                temp_lockfile.display()
            );
        }
    }

    let venv_path = temp_path.join(".venv");

    copy_site_packages_to_db(db, &venv_path, dest_venv_path, python_version)
}

/// Copy the site-packages directory from a real virtual environment to the in-memory filesystem of `db`.
///
/// This recursively copies all files from the venv's site-packages directory into the
/// in-memory filesystem at the specified destination path.
fn copy_site_packages_to_db(
    db: &mut Db,
    venv_path: &SystemPath,
    dest_venv_path: &SystemPath,
    _python_version: PythonVersion,
) -> Result<()> {
    // Discover the site-packages directory in the virtual environment
    let system = OsSystem::new(venv_path);
    let env = PythonEnvironment::new(venv_path, SysPrefixPathOrigin::LocalVenv, &system)
        .context("Failed to create Python environment for temporary virtual environment")?;

    let site_packages_paths = env
        .site_packages_paths(&system)
        .context(format!("Failed to discover site-packages in '{venv_path}'"))?;

    let site_packages_path = site_packages_paths
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("No site-packages directory found in '{venv_path}'"))?;

    // Create the destination directory structure
    let relative_site_packages = site_packages_path.strip_prefix(venv_path).map_err(|_| {
        anyhow!("site-packages path '{site_packages_path}' is not under venv path '{venv_path}'")
    })?;
    let dest_site_packages = dest_venv_path.join(relative_site_packages);
    db.create_directory_all(&dest_site_packages)
        .context("Failed to create site-packages directory in database")?;

    // Recursively copy all files from site-packages
    copy_directory_recursive(db, &site_packages_path, &dest_site_packages)?;

    Ok(())
}

fn copy_directory_recursive(db: &mut Db, src: &SystemPath, dest: &SystemPath) -> Result<()> {
    use std::fs;

    for entry in fs::read_dir(src.as_std_path())
        .with_context(|| format!("Failed to read directory {src}"))?
    {
        let entry = entry.with_context(|| format!("Failed to read directory entry in {src}"))?;
        let entry_path = entry.path();
        let file_type = entry
            .file_type()
            .with_context(|| format!("Failed to get file type for {}", entry_path.display()))?;

        let src_path = SystemPath::from_std_path(&entry_path)
            .ok_or_else(|| anyhow!("Path {} is not valid UTF-8", entry_path.display()))?;

        let file_name = entry.file_name();
        let file_name_str = file_name.to_str().ok_or_else(|| {
            anyhow!(
                "File name {} is not valid UTF-8",
                file_name.to_string_lossy()
            )
        })?;

        let dest_path = dest.join(file_name_str);

        if file_type.is_dir() {
            // Skip __pycache__ directories and other unnecessary directories
            if file_name_str == "__pycache__" || file_name_str.ends_with(".dist-info") {
                continue;
            }

            db.create_directory_all(&dest_path)
                .with_context(|| format!("Failed to create directory {dest_path}"))?;

            copy_directory_recursive(db, src_path, &dest_path)?;
        } else if file_type.is_file() {
            let is_python_source = entry_path.extension().is_some_and(|ext| {
                ext.eq_ignore_ascii_case("py") || ext.eq_ignore_ascii_case("pyi")
            });

            if !is_python_source {
                // Skip all non-Python files (binaries, data files, etc.)
                continue;
            }

            let contents = fs::read_to_string(src_path.as_std_path())
                .with_context(|| format!("Failed to read file {src_path}"))?;

            db.write_file(&dest_path, contents)
                .with_context(|| format!("Failed to write file {dest_path}"))?;
        }
    }

    Ok(())
}
